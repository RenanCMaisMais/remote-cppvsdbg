use std::env;
use std::io::{self, ErrorKind, Read, Write};
use std::net::TcpListener;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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

fn run_interactive(host: &str) -> io::Result<()> {
    const COMMAND: &str = r#"
        $TaskName = \"TempElevatedSessionTask\"
        
        $UsersGroup = (New-Object System.Security.Principal.SecurityIdentifier('S-1-5-32-545')).Translate([System.Security.Principal.NTAccount]).Value
    
        $Action = New-ScheduledTaskAction -Execute \"cmd.exe\" -Argument \"/c start %USERPROFILE%/.cppvsdbg/spawner/spawner.exe\"
        $Principal = New-ScheduledTaskPrincipal -GroupId $UsersGroup -RunLevel Highest
    
        Register-ScheduledTask -TaskName $TaskName -Action $Action -Principal $Principal
        Start-ScheduledTask -TaskName $TaskName
    
        Start-Sleep -Seconds 2
        Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
        Start-Sleep -Seconds 5
    "#;

    let listener = TcpListener::bind("localhost:9090")?;
    listener.set_nonblocking(true)?;

    let timeout = Duration::from_secs(10);
    let start_time = Instant::now();

    // For some reason the command is only getting parsed up to the first newline, so flattening it is required
    let flat_command: &str = &COMMAND.replace("\n", ";");

    let mut child = Command::new("ssh")
        .args([
            "-o",
            "StrictHostKeyChecking=no",
            "-R",
            "9090:localhost:9090",
            &host,
            "powershell -Command ",
            &flat_command,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("The SSH client should be installed");

    let (stream, _addr) = loop {
        match listener.accept() {
            Ok((stream, addr)) => break (stream, addr),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                if start_time.elapsed() >= timeout {
                    child.kill()?;
                    return Err(io::Error::from(ErrorKind::TimedOut));
                }
                std::thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(e) => {
                child.kill()?;
                return Err(e);
            }
        }
    };

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
    child.wait()?;

    Ok(())
}

fn main() -> io::Result<()> {
    let host =
        env::var("VSDBG_SSH_HOST").expect("env variable `SSH_HOST` must be set for this to work");
    let interactive = match env::var("VSDBG_INTERACTIVE") {
        Ok(value) if value == "TRUE" => true,
        _ => false,
    };

    if interactive {
        return run_interactive(&host);
    } else {
        return run(&host);
    }
}
