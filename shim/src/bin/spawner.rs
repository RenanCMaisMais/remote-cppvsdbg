#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    env,
    io::{self, Read, Write},
    net::{Shutdown, TcpStream},
    process::{Command, Stdio},
    thread::{self},
};

#[cfg(not(debug_assertions))]
use std::os::windows::process::CommandExt;

#[path = "../utils.rs"]
mod utils;

fn main() -> io::Result<()> {
    #[cfg(debug_assertions)]
    {
        utils::let_debugger_attach();
    }
    debug_log!("[main] started");
    let stream = TcpStream::connect("127.0.0.1:9090").expect("Server must be listening");
    debug_log!("[main] connected to socket");

    let mut stream_writer = stream.try_clone()?;
    let mut stream_reader = stream.try_clone()?;

    let process_os = env::var_os("USERPROFILE").expect("Could not get user profile");

    let program =
        process_os.to_str().expect("Failed to unwrap").to_owned() + "/.cppvsdbg/bin/vsdbg.exe";

    let mut command = Command::new(program);
    command
        .args([
            "--interpreter=vscode",
            "--extConfigDir=%USERPROFILE%\\.cppvsdbg\\extensions",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    #[cfg(not(debug_assertions))]
    {
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .expect("Failed to start ssh. Is OpenSSH installed?");

    let mut child_stdin = child.stdin.take().unwrap();
    let mut child_stdout = child.stdout.take().unwrap();

    let socket_to_child = thread::spawn(move || -> io::Result<()> {
        let mut buffer = [0u8; 4096];

        loop {
            let n = match stream_reader.read(&mut buffer) {
                Ok(0) => {
                    debug_log!("[socket->child] EOF from server; closing child stdin");
                    drop(child_stdin);
                    return Ok(());
                }
                Ok(n) => {
                    debug_log!("[socket->child] read {} bytes", n);
                    n
                }
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {
                    debug_log!("[socket->child] interrupted, retrying");
                    continue;
                }
                Err(e) => {
                    debug_log!("[socket->child] read error: {e}");
                    return Err(e);
                }
            };

            if let Err(e) = child_stdin.write_all(&buffer[..n]) {
                debug_log!("[socket->child] write to child failed: {e}");
                if matches!(
                    e.kind(),
                    io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
                ) {
                    return Ok(());
                }
                return Err(e);
            }

            child_stdin.flush()?;
        }
    });

    let mut buffer = [0u8; 4096];
    loop {
        let n = match child_stdout.read(&mut buffer) {
            Ok(0) => {
                debug_log!("[child->socket] child stdout closed");
                break;
            }
            Ok(n) => {
                debug_log!("[child->socket] got {} bytes from child", n);
                n
            }
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {
                debug_log!("[child->socket] interrupted, retrying");
                continue;
            }
            Err(e) => {
                debug_log!("[child->socket] child read error: {e}");
                if matches!(
                    e.kind(),
                    io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
                ) {
                    break;
                }
                return Err(e);
            }
        };

        if let Err(e) = stream_writer.write_all(&buffer[..n]) {
            debug_log!("[child->socket] write to socket failed: {e}");
            if matches!(
                e.kind(),
                io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset
            ) {
                break;
            }
            return Err(e);
        }

        stream_writer.flush()?;
    }

    debug_log!("[main] shutting down socket write side");
    stream_writer.shutdown(Shutdown::Write)?;
    debug_log!("[main] joining socket_to_child");
    match socket_to_child.join() {
        Ok(result) => match result {
            Ok(a) => a,
            Err(_err) => {
                debug_log!("socket -> child thread returned error: {}", _err);
            }
        },
        Err(_) => {
            debug_log!("socket -> child thread panicked!");
        }
    }
    debug_log!("[main] waiting for child");
    child.wait()?;

    debug_log!("[main] exiting");
    Ok(())
}
