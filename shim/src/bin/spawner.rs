#![windows_subsystem = "windows"]

use std::{
    env, io::{self, Read, Write}, net::{Shutdown, TcpStream}, os::windows::process::CommandExt, process::{Command, Stdio}, thread,
};

fn main() -> io::Result<()> {
    let stream = TcpStream::connect("localhost:9090").expect("Server must be listening");

    let mut stream_writer = stream.try_clone()?;
    let mut stream_reader = stream.try_clone()?;

    let process_os = env::var_os("USERPROFILE").expect("Could not get user profile");

    let program =
        process_os.to_str().expect("Failed to unwrap").to_owned() + "/.cppvsdbg/bin/vsdbg.exe";

    let mut child = Command::new(program)
        .args([
            "--interpreter=vscode",
            "--extConfigDir=%USERPROFILE%\\.cppvsdbg\\extensions",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .creation_flags(0x08000000)
        .spawn()
        .expect("Failed to start ssh. Is OpenSSH installed?");

    let mut child_stdin = child.stdin.take().unwrap();
    let mut child_stdout = child.stdout.take().unwrap();

    let socket_to_child = thread::spawn(move || -> io::Result<()> {
        let mut buffer = [0u8; 4096];

        loop {
            let n = match stream_reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };

            child_stdin.write_all(&buffer[..n])?;
            child_stdin.flush()?;
        }

        Ok(())
    });

    let mut buffer = [0u8; 4096];
    loop {
        let n = match child_stdout.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };

        stream_writer.write_all(&buffer[..n])?;
        stream_writer.flush()?;
    }

    stream_writer.shutdown(Shutdown::Write)?;
    let _ = socket_to_child.join();
    child.wait()?;

    Ok(())
}
