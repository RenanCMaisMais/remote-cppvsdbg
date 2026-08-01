use std::env;
use std::io::{self, Read, Write};
use std::process::{Command, Stdio};
use std::thread;

fn main() -> io::Result<()> {
    let host = env::var("VSDBG_SSH_HOST").expect("SSH_HOST not set");

    let mut child = Command::new("ssh")
        .args([
            "-o",
            "StrictHostKeyChecking=no",
            &format!("{}", host),
            "%USERPROFILE%/.cppvsdbg/bin/vsdbg.exe --interpreter=vscode --extConfigDir=%USERPROFILE%\\.cppvsdbg\\extensions",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("Failed to start ssh. Is OpenSSH installed?");

    let mut remote_stdin = child.stdin.take().unwrap();
    let mut remote_stdout = child.stdout.take().unwrap();

    thread::scope(|s| {
        s.spawn(move || {
            let mut buf = [0; 8192];
            let mut local_stdin = io::stdin();
            loop {
                match local_stdin.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let _ = remote_stdin.write_all(&buf[..n]);
                    }
                    Err(_) => break,
                }
            }
        });

        s.spawn(move || {
            let mut buf = [0; 8192];
            let mut local_stdout = io::stdout();
            loop {
                let n = remote_stdout.read(&mut buf).unwrap_or(0);
                if n == 0 {
                    break;
                }
                if local_stdout.write_all(&buf[..n]).is_err() {
                    break;
                }
                let _ = local_stdout.flush();
            }
        });
    });

    // Wait for the ssh process to exit
    let _ = child.wait();
    Ok(())
}
