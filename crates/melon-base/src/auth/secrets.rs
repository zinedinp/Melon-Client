//! The system keyring: `secret-tool` (libsecret) on Linux, the Credential Manager on Windows,
//! the login Keychain on macOS.

const SERVICE: &str = "melon";

#[cfg(target_os = "linux")]
mod imp {
    use std::io::Write;
    use std::process::{Command, Stdio};

    use super::SERVICE;

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

/// Generic credentials `melon/<account>`, `melon/<account>#1`, ...: one holds at most
/// 2560 bytes, less than a Microsoft refresh and access token together.
#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Security::Credentials::{
        CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW,
        CredDeleteW, CredFree, CredReadW, CredWriteW,
    };

    use super::SERVICE;

    /// More parts than any token pair needs; ends the clean-up of a longer old secret.
    const MAX_PARTS: usize = 16;

    fn target(account: &str, part: usize) -> Vec<u16> {
        let name = match part {
            0 => format!("{SERVICE}/{account}"),
            n => format!("{SERVICE}/{account}#{n}"),
        };
        name.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn write(account: &str, part: usize, bytes: &[u8]) -> bool {
        let mut name = target(account, part);
        let credential = CREDENTIALW {
            Type: CRED_TYPE_GENERIC,
            TargetName: name.as_mut_ptr(),
            CredentialBlobSize: bytes.len() as u32,
            CredentialBlob: bytes.as_ptr().cast_mut(),
            Persist: CRED_PERSIST_LOCAL_MACHINE,
            ..Default::default()
        };
        // SAFETY: `credential` points at `name` and `bytes`, both live for the call, which only
        // reads them.
        unsafe { CredWriteW(&credential, 0) != 0 }
    }

    fn read(account: &str, part: usize) -> Option<Vec<u8>> {
        let name = target(account, part);
        let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
        // SAFETY: `name` is NUL-terminated; on success `credential` is a block the system
        // allocated, read once here and given back with `CredFree`.
        unsafe {
            if CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) == 0 {
                return None;
            }
            let blob = &*credential;
            let bytes = if blob.CredentialBlob.is_null() {
                Vec::new()
            } else {
                std::slice::from_raw_parts(blob.CredentialBlob, blob.CredentialBlobSize as usize)
                    .to_vec()
            };
            CredFree(credential.cast());
            Some(bytes)
        }
    }

    fn remove(account: &str, part: usize) -> bool {
        let name = target(account, part);
        // SAFETY: `name` is NUL-terminated and outlives the call.
        unsafe { CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) != 0 }
    }

    /// Keeps `secret` under `account`. `false` if the Credential Manager refused it.
    pub fn store(account: &str, secret: &str) -> bool {
        let parts: Vec<&[u8]> = secret
            .as_bytes()
            .chunks(CRED_MAX_CREDENTIAL_BLOB_SIZE as usize)
            .collect();
        if parts.is_empty() || parts.len() > MAX_PARTS {
            return false;
        }
        if !parts
            .iter()
            .enumerate()
            .all(|(i, part)| write(account, i, part))
        {
            return false;
        }
        for stale in parts.len()..MAX_PARTS {
            if !remove(account, stale) {
                break;
            }
        }
        true
    }

    pub fn load(account: &str) -> Option<String> {
        let mut bytes = read(account, 0)?;
        for part in 1..MAX_PARTS {
            match read(account, part) {
                Some(more) => bytes.extend(more),
                None => break,
            }
        }
        String::from_utf8(bytes).ok().filter(|s| !s.is_empty())
    }

    pub fn delete(account: &str) {
        for part in 0..MAX_PARTS {
            if !remove(account, part) {
                break;
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use security_framework::passwords::{
        delete_generic_password, get_generic_password, set_generic_password,
    };

    use super::SERVICE;

    /// Keeps `secret` under `account`. `false` if the Keychain refused it.
    pub fn store(account: &str, secret: &str) -> bool {
        set_generic_password(SERVICE, account, secret.as_bytes()).is_ok()
    }

    pub fn load(account: &str) -> Option<String> {
        let bytes = get_generic_password(SERVICE, account).ok()?;
        String::from_utf8(bytes).ok().filter(|s| !s.is_empty())
    }

    pub fn delete(account: &str) {
        let _ = delete_generic_password(SERVICE, account);
    }
}

#[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
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

#[cfg(all(test, windows))]
mod tests {
    /// Writes to the real Credential Manager under a throwaway name, and removes it again.
    #[test]
    fn a_secret_longer_than_one_credential_comes_back_whole() {
        let account = format!("test-{}", uuid::Uuid::new_v4());
        let long: String = (0..6000)
            .map(|i| char::from(b'a' + (i % 26) as u8))
            .collect();
        assert!(super::store(&account, &long));
        assert_eq!(super::load(&account).as_deref(), Some(long.as_str()));
        // A shorter secret leaves none of the longer one's parts behind.
        assert!(super::store(&account, "short"));
        assert_eq!(super::load(&account).as_deref(), Some("short"));
        super::delete(&account);
        assert_eq!(super::load(&account), None);
    }
}
