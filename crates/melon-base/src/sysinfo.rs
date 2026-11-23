//! General facts about the machine for logs and bug reports. Nothing about the user: paths that
//! lie in the home folder are shown from `~`.

use std::path::Path;

/// The operating system, e.g. "Windows 11 Pro 24H2 (build 26100.2605)" or
/// "Arch Linux, kernel 6.11.5-arch1-1".
pub fn os() -> String {
    #[cfg(windows)]
    {
        const KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
        let build: u32 = win::string(KEY, "CurrentBuild")
            .and_then(|b| b.parse().ok())
            .unwrap_or(0);
        // ProductName still says "Windows 10" on Windows 11.
        let mut name = win::string(KEY, "ProductName").unwrap_or_else(|| "Windows".into());
        if build >= 22000 {
            name = name.replace("Windows 10", "Windows 11");
        }
        let release = win::string(KEY, "DisplayVersion")
            .map(|v| format!(" {v}"))
            .unwrap_or_default();
        let ubr = win::dword(KEY, "UBR")
            .map(|u| format!(".{u}"))
            .unwrap_or_default();
        format!("{name}{release} (build {build}{ubr})")
    }
    #[cfg(target_os = "linux")]
    {
        let release = std::fs::read_to_string("/etc/os-release").unwrap_or_default();
        let field = |key: &str| {
            release
                .lines()
                .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
                .map(|v| v.trim_matches('"').to_owned())
        };
        let name = field("PRETTY_NAME")
            .or_else(|| field("NAME"))
            .unwrap_or_else(|| "Linux".into());
        let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap_or_default();
        format!("{name}, kernel {}", kernel.trim())
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        std::env::consts::OS.to_owned()
    }
}

/// The CPU's name and how many threads it runs, e.g. "AMD Ryzen 7 5800X3D, 16 threads".
pub fn cpu() -> String {
    #[cfg(windows)]
    let name = win::string(
        r"HARDWARE\DESCRIPTION\System\CentralProcessor\0",
        "ProcessorNameString",
    );
    #[cfg(target_os = "linux")]
    let name = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|info| {
            info.lines().find_map(|l| {
                l.strip_prefix("model name")?
                    .split_once(':')
                    .map(|(_, v)| v.to_owned())
            })
        });
    #[cfg(not(any(windows, target_os = "linux")))]
    let name: Option<String> = None;
    let name = name.map_or_else(
        || std::env::consts::ARCH.to_owned(),
        |n| n.trim().to_owned(),
    );
    match std::thread::available_parallelism() {
        Ok(threads) => format!("{name}, {threads} threads"),
        Err(_) => name,
    }
}

/// Installed memory, e.g. "31.9 GiB".
pub fn memory() -> Option<String> {
    #[cfg(windows)]
    let bytes = win::total_memory();
    #[cfg(target_os = "linux")]
    let bytes = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|info| {
            let kib: u64 = info
                .lines()
                .find_map(|l| l.strip_prefix("MemTotal:"))?
                .trim()
                .trim_end_matches("kB")
                .trim()
                .parse()
                .ok()?;
            Some(kib * 1024)
        });
    #[cfg(not(any(windows, target_os = "linux")))]
    let bytes: Option<u64> = None;
    bytes.map(|b| format!("{:.1} GiB", b as f64 / (1u64 << 30) as f64))
}

/// The C and C++ runtimes a game binary would load, e.g. "glibc 2.40, libstdc++.so.6.0.33".
/// On Windows Melon links them in, so this names a libstdc++ on PATH that could get in between.
pub fn runtimes() -> String {
    #[cfg(windows)]
    {
        let found = std::env::var_os("PATH")
            .into_iter()
            .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
            .map(|dir| dir.join("libstdc++-6.dll"))
            .find(|dll| dll.is_file());
        match found {
            Some(dll) => format!("libstdc++-6.dll on PATH at {}", home_relative(&dll)),
            None => "no libstdc++-6.dll on PATH".to_owned(),
        }
    }
    #[cfg(target_os = "linux")]
    {
        #[cfg(target_env = "gnu")]
        let libc = {
            unsafe extern "C" {
                fn gnu_get_libc_version() -> *const std::ffi::c_char;
            }
            // SAFETY: glibc returns a static, NUL-terminated string.
            let version = unsafe { std::ffi::CStr::from_ptr(gnu_get_libc_version()) };
            format!("glibc {}", version.to_string_lossy())
        };
        #[cfg(not(target_env = "gnu"))]
        let libc = format!("libc {}", std::env::consts::ARCH);
        let dirs = [
            "/usr/lib64",
            "/usr/lib/x86_64-linux-gnu",
            "/lib/x86_64-linux-gnu",
            "/usr/lib",
            "/lib64",
        ];
        // The real file behind the soname tells the version: libstdc++.so.6.0.33.
        let cxx = dirs
            .iter()
            .find_map(|dir| std::fs::canonicalize(Path::new(dir).join("libstdc++.so.6")).ok())
            .and_then(|file| file.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "no libstdc++.so.6 in the usual folders".to_owned());
        format!("{libc}, {cxx}")
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        String::new()
    }
}

/// The desktop session on Linux, e.g. "wayland, KDE"; nothing elsewhere.
pub fn session() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let kind = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".into());
    Some(match std::env::var("XDG_CURRENT_DESKTOP") {
        Ok(desktop) if !desktop.is_empty() => format!("{kind}, {desktop}"),
        _ => kind,
    })
}

/// `path` with the home folder written as `~`, so a log does not carry the user's name.
pub fn home_relative(path: &Path) -> String {
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    match home
        .as_deref()
        .and_then(|home| path.strip_prefix(home).ok())
    {
        Some(rest) => Path::new("~").join(rest).display().to_string(),
        None => path.display().to_string(),
    }
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    type Hkey = isize;
    // (HKEY)(LONG)0x80000002, sign-extended.
    const HKEY_LOCAL_MACHINE: Hkey = 0x8000_0002_u32 as i32 as isize;
    const RRF_RT_REG_SZ: u32 = 0x2;
    const RRF_RT_REG_DWORD: u32 = 0x10;

    #[repr(C)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegGetValueW(
            key: Hkey,
            sub_key: *const u16,
            value: *const u16,
            flags: u32,
            kind: *mut u32,
            data: *mut c_void,
            len: *mut u32,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GlobalMemoryStatusEx(status: *mut MemoryStatusEx) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain([0]).collect()
    }

    /// A string value under HKEY_LOCAL_MACHINE.
    pub fn string(key: &str, value: &str) -> Option<String> {
        let (key, value) = (wide(key), wide(value));
        let mut len = 0u32;
        // SAFETY: the first call only asks for the size; the second fills a buffer of that size.
        unsafe {
            let flags = RRF_RT_REG_SZ;
            let probe = RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                value.as_ptr(),
                flags,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut len,
            );
            if probe != 0 || len == 0 {
                return None;
            }
            let mut buf = vec![0u16; len as usize / 2];
            let read = RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                value.as_ptr(),
                flags,
                std::ptr::null_mut(),
                buf.as_mut_ptr().cast(),
                &mut len,
            );
            if read != 0 {
                return None;
            }
            let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
            Some(String::from_utf16_lossy(&buf[..end]))
        }
    }

    /// A DWORD value under HKEY_LOCAL_MACHINE.
    pub fn dword(key: &str, value: &str) -> Option<u32> {
        let (key, value) = (wide(key), wide(value));
        let (mut data, mut len) = (0u32, 4u32);
        // SAFETY: `data` holds the 4 bytes a DWORD needs.
        let read = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&raw mut data).cast(),
                &mut len,
            )
        };
        (read == 0).then_some(data)
    }

    /// Installed physical memory in bytes.
    pub fn total_memory() -> Option<u64> {
        let mut status = MemoryStatusEx {
            length: size_of::<MemoryStatusEx>() as u32,
            memory_load: 0,
            total_phys: 0,
            avail_phys: 0,
            total_page_file: 0,
            avail_page_file: 0,
            total_virtual: 0,
            avail_virtual: 0,
            avail_extended_virtual: 0,
        };
        // SAFETY: `length` is set as the call requires.
        (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status.total_phys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_machine() {
        let (os, cpu) = (os(), cpu());
        println!(
            "{os}\n{cpu}\n{:?}\n{}\n{:?}",
            memory(),
            runtimes(),
            session()
        );
        assert!(!os.is_empty() && !cpu.is_empty());
        assert!(memory().is_some());
    }

    #[test]
    fn home_becomes_tilde() {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap();
        let inside = Path::new(&home).join("melon").join("x.log");
        assert_eq!(
            home_relative(&inside),
            Path::new("~")
                .join("melon")
                .join("x.log")
                .display()
                .to_string()
        );
        assert_eq!(
            home_relative(Path::new("/opt/x")),
            Path::new("/opt/x").display().to_string()
        );
    }
}
