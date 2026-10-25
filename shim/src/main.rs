use core::panic;
use std::io::{self, ErrorKind, Read, Write};
use std::mem::MaybeUninit;
use std::net::{TcpListener, TcpStream};
use std::os::windows::io::{AsRawSocket, FromRawSocket, RawSocket};
use std::process::{Command, Stdio};
use std::thread::{self};
use std::time::{Duration, Instant};
use std::{env, format};
use tokio::net::windows::named_pipe::{
    ClientOptions, NamedPipeClient, NamedPipeServer, ServerOptions,
};
use windows::Win32::Networking::WinSock::{
    FROM_PROTOCOL_INFO, INVALID_SOCKET, SOCKET, WSA_FLAG_OVERLAPPED, WSADATA, WSADuplicateSocketW,
    WSAGetLastError, WSAPROTOCOL_INFOW, WSASocketW, WSAStartup,
};

mod utils;

#[derive(Debug)]
enum Pipe {
    Client(NamedPipeClient),
    Server(NamedPipeServer),
}

const PIPE_NAME: &str = r"\\.\pipe\remote-cppvsdbg";

fn run(host: &str) -> io::Result<()> {
    Command::new("ssh")
        .args([
            "-o",
            "StrictHostKeyChecking=no",
            &host,
            "%USERPROFILE%/.cppvsdbg/bin/vsdbg.exe --interpreter=vscode --extConfigDir=%USERPROFILE%\\.cppvsdbg\\extensions",
        ])
        .spawn()
        .expect("The SSH client should be installed").wait()?;

    Ok(())
}

fn duplicate_socket(stream: &TcpStream, target_pid: u32) -> Result<WSAPROTOCOL_INFOW, String> {
    let raw_socket = SOCKET(stream.as_raw_socket() as usize);
    let mut proto_info = MaybeUninit::<WSAPROTOCOL_INFOW>::uninit();

    let result = unsafe { WSADuplicateSocketW(raw_socket, target_pid, proto_info.as_mut_ptr()) };

    if result != 0 {
        let err = unsafe { WSAGetLastError() };
        return Err(format!(
            "WSADuplicateSocketW failed with error code: {:?}",
            err
        ));
    }

    Ok(unsafe { proto_info.assume_init() })
}

async fn connect(host: &str) -> io::Result<TcpStream> {
    const COMMAND: &str = r#"
        $TaskName = \"TempElevatedSessionTask\"
        
        $UsersGroup = (New-Object System.Security.Principal.SecurityIdentifier('S-1-5-32-545')).Translate([System.Security.Principal.NTAccount]).Value
    
        $Action = New-ScheduledTaskAction -Execute \"%USERPROFILE%/.cppvsdbg/spawner/spawner.exe\"
        $Principal = New-ScheduledTaskPrincipal -GroupId $UsersGroup -RunLevel Highest
    
        Register-ScheduledTask -TaskName $TaskName -Action $Action -Principal $Principal
        Start-ScheduledTask -TaskName $TaskName
    
        Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
    "#;

    let result = ServerOptions::new()
        .first_pipe_instance(true)
        .create(PIPE_NAME);

    let pipe = match result {
        Ok(pipe) => Pipe::Server(pipe),
        Err(err) if err.kind() == io::ErrorKind::PermissionDenied => {
            let pipe = ClientOptions::new().open(PIPE_NAME)?;
            Pipe::Client(pipe)
        }
        Err(err) => return Err(err),
    };

    let spawn_child = |host: &str| {
        // For some reason the command is only getting parsed up to the first newline, so flattening it is required
        let flat_command = COMMAND.replace("\n", ";");

        let result = Command::new("ssh")
            .args([
                "-o",
                "StrictHostKeyChecking=no",
                "-R",
                "9090:127.0.0.1:9090",
                host,
                "powershell -Command ",
                &flat_command,
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("The SSH client should be installed");

        if !result.success() {
            panic!("failed to spawn child");
        }
    };

    match pipe {
        Pipe::Client(client) => {
            let mut protocol_info = MaybeUninit::<WSAPROTOCOL_INFOW>::uninit();
            let protocol_info_bytes = unsafe {
                std::slice::from_raw_parts_mut(
                    protocol_info.as_mut_ptr() as *mut u8,
                    std::mem::size_of::<WSAPROTOCOL_INFOW>(),
                )
            };

            let pid = std::process::id();
            loop {
                client.writable().await?;

                match client.try_write(&pid.to_le_bytes()) {
                    Ok(n) if n == 4 => {
                        break;
                    }
                    Ok(n) => {
                        panic!("wrote wrong size! {} bytes", n);
                    }
                    Err(err) => return Err(err),
                }
            }

            loop {
                client.readable().await?;

                match client.try_read(protocol_info_bytes) {
                    Ok(n) if n != std::mem::size_of::<WSAPROTOCOL_INFOW>() => {
                        panic!("wrong size");
                    }
                    Ok(_) => {
                        break;
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                    Err(e) => return Err(e),
                }
            }

            let protocol_info = unsafe { protocol_info.assume_init() };

            // 2. Recreate the socket descriptor using WSASocketW
            let raw_socket = unsafe {
                let mut data: WSADATA = Default::default();
                let buf = [2u8, 2];
                let version = u16::from_le_bytes(buf);

                let err = WSAStartup(version, &mut data);
                if err != 0 {
                    panic!("failed to initialize");
                }

                WSASocketW(
                    FROM_PROTOCOL_INFO,
                    FROM_PROTOCOL_INFO,
                    FROM_PROTOCOL_INFO,
                    Some(&protocol_info as *const _),
                    0,
                    WSA_FLAG_OVERLAPPED,
                )
            };

            match raw_socket {
                Ok(INVALID_SOCKET) => {
                    panic!("socket is invalid");
                }
                Ok(socket) => Ok(unsafe { TcpStream::from_raw_socket(socket.0 as RawSocket) }),
                Err(err) => {
                    let error = unsafe { WSAGetLastError() }.0;
                    panic!("WSASocketW returned error {} {}", err, error);
                }
            }
        }
        Pipe::Server(mut server) => {
            let listener = TcpListener::bind("127.0.0.1:9090")?;
            listener.set_nonblocking(true)?;

            let timeout = Duration::from_secs(10);
            let start_time = Instant::now();

            spawn_child(host);

            let (stream, _addr) = loop {
                match listener.accept() {
                    Ok((stream, addr)) => break (stream, addr),
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        if start_time.elapsed() >= timeout {
                            return Err(io::Error::from(ErrorKind::TimedOut));
                        }
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            };

            listener.set_nonblocking(false)?;

            let host = host.to_owned();
            let _handle: tokio::task::JoinHandle<io::Result<()>> = tokio::spawn(async move {
                loop {
                    server.connect().await.unwrap();
                    let connected_client = server;

                    server = ServerOptions::new().create(PIPE_NAME).unwrap();

                    spawn_child(&host);

                    let (stream, _addr) = listener.accept()?;

                    let pid: u32 = loop {
                        connected_client.readable().await.unwrap();

                        let mut buf = [0u8; 4];

                        match connected_client.try_read(&mut buf) {
                            Ok(n) if n == 4 => break u32::from_le_bytes(buf),
                            Ok(n) => {
                                panic!("read wrong size! {} bytes", n);
                            }
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                continue;
                            }
                            Err(e) => return Err(e),
                        }
                    };

                    let protocol_info = duplicate_socket(&stream, pid).unwrap();

                    let bytes: &[u8] = unsafe {
                        std::slice::from_raw_parts(
                            (&protocol_info as *const WSAPROTOCOL_INFOW) as *const u8,
                            std::mem::size_of::<WSAPROTOCOL_INFOW>(),
                        )
                    };

                    loop {
                        connected_client.writable().await.unwrap();

                        match connected_client.try_write(bytes) {
                            Ok(_) => break,
                            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                                continue;
                            }
                            Err(e) => return Err(e),
                        }
                    }
                }
            });

            Ok(stream)
        }
    }
}

async fn run_interactive(host: &str) -> io::Result<()> {
    let stream = connect(&host).await?;

    stream.set_nonblocking(false)?;

    let mut socket_reader = stream.try_clone()?;
    let mut socket_writer = stream.try_clone()?;

    let stdin_thread = thread::spawn(move || -> io::Result<()> {
        let mut stdin = io::stdin();
        let mut buffer = [0u8; 4096];

        loop {
            let n = match stdin.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };

            socket_writer.write_all(&buffer[..n])?;
            socket_writer.flush()?;
        }

        Ok(())
    });

    let mut stdout = io::stdout();
    let mut buffer = [0u8; 4096];

    loop {
        let n = match socket_reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };

        stdout.write_all(&buffer[..n])?;
        stdout.flush()?;
    }

    stdin_thread.join().unwrap()?;

    Ok(())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    #[cfg(debug_assertions)]
    {
        utils::let_debugger_attach();
    }

    let host =
        env::var("VSDBG_SSH_HOST").expect("env variable `SSH_HOST` must be set for this to work");
    let interactive = match env::var("VSDBG_INTERACTIVE") {
        Ok(value) if value == "TRUE" => true,
        _ => false,
    };

    if interactive {
        return run_interactive(&host).await;
    } else {
        return run(&host);
    }
}
