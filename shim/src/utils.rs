#[cfg(debug_assertions)]
use std::process::Command;

#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        #[cfg(debug_assertions)]
        {
            eprintln!($($arg)*);
        }
    };
}

#[cfg(debug_assertions)]
pub fn let_debugger_attach() {
    let pid = std::process::id();

    let url = format!(
        "vscode://vadimcn.vscode-lldb/launch/config?{{''request'':''attach'',''pid'':{}}}",
        pid
    );

    let _ = Command::new("powershell")
        .env_remove("ELECTRON_RUN_AS_NODE")
        .args(&[
            "-NoProfile",
            "-Command",
            &format!("Start-Process '{}'", url),
        ])
        .status();
}
