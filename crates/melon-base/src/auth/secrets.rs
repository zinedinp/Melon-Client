//! The system keyring, through `secret-tool` (libsecret) on Linux.

#[cfg(target_os = "linux")]
mod imp {
    use std::io::Write;
    use std::process::{Command, Stdio};

    const SERVICE: &str = "melon";

    /// Keeps `secret` under `account`. `false` if the keyring refused it.
    pub fn store(account: &str, secret: &str) -> bool {
        let Ok(mut child) = Command::new("secret-tool")
            .args(["store", "--label", "Melon account", "service", SERVICE])
            .args(["account", account])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };
        let written = child
            .stdin
            .take()
            .is_some_and(|mut stdin| stdin.write_all(secret.as_bytes()).is_ok());
        child.wait().is_ok_and(|status| status.success()) && written
    }

    pub fn load(account: &str) -> Option<String> {
        let output = Command::new("secret-tool")
            .args(["lookup", "service", SERVICE, "account", account])
            .stderr(Stdio::null())
            .output()
            .ok()?;
        (output.status.success() && !output.stdout.is_empty())
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }

    pub fn delete(account: &str) {
        let _ = Command::new("secret-tool")
            .args(["clear", "service", SERVICE, "account", account])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    pub fn store(_account: &str, _secret: &str) -> bool {
        false
    }

    pub fn load(_account: &str) -> Option<String> {
        None
    }

    pub fn delete(_account: &str) {}
}

pub use imp::{delete, load, store};
