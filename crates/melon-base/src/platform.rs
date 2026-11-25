//! Small OS services shared by both binaries.

#[cfg(target_os = "linux")]
use std::io::ErrorKind;
#[cfg(target_os = "linux")]
use std::process::Command;
#[cfg(target_os = "linux")]
use std::process::Output;

/// PowerShell without a console window: the launcher is a GUI program, so Windows would open one.
/// By its full path: its folder isn't always on `PATH`, and then the dialogs never opened.
#[cfg(windows)]
fn powershell() -> std::process::Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let exe = std::path::Path::new(&root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let mut cmd = std::process::Command::new(exe);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Makes PowerShell write its output in UTF-8 instead of the OEM code page.
#[cfg(windows)]
const UTF8_OUTPUT: &str = "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; ";

/// A single-quoted PowerShell string: every quote doubled, the typographic ones too.
#[cfg(any(windows, test))]
fn powershell_string(text: &str) -> String {
    let mut out = String::from("'");
    for c in text.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// A double-quoted AppleScript string: quotes and backslashes escaped.
#[cfg(any(target_os = "macos", test))]
fn applescript_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        if matches!(c, '"' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// Native file dialog (zenity / kdialog / osascript / PowerShell). No Python, no Java.
pub fn pick_file(title: &str, extension: &str) -> Option<std::path::PathBuf> {
    let output = {
        #[cfg(target_os = "linux")]
        {
            let filter = format!("*.{extension}");
            let zenity_filter = format!("{} | *.{}", extension.to_ascii_uppercase(), extension);
            let start = dialog_start_dir();
            let mut kdialog = Command::new("kdialog");
            kdialog.args(["--title", title, "--getopenfilename", &start, &filter]);
            let mut zenity = Command::new("zenity");
            zenity.args([
                "--file-selection",
                "--title",
                title,
                "--file-filter",
                &zenity_filter,
            ]);
            linux_dialog(kdialog, zenity)
        }
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("osascript")
                .args([
                    "-e",
                    &format!(
                        "POSIX path of (choose file of type {{{}}} with prompt {})",
                        applescript_string(extension),
                        applescript_string(title),
                    ),
                ])
                .output()
                .ok()
                .filter(|o| o.status.success())
        }
        #[cfg(windows)]
        {
            let script = format!(
                "{UTF8_OUTPUT}Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.OpenFileDialog; $f.Filter = {filter}; $f.Title = {title}; if ($f.ShowDialog() -eq 'OK') {{ $f.FileName }}",
                filter = powershell_string(&format!("{extension} (*.{extension})|*.{extension}")),
                title = powershell_string(title),
            );
            powershell()
                .args(["-NoProfile", "-Command", &script])
                .output()
                .ok()
                .filter(|o| o.status.success())
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        {
            Option::<std::process::Output>::None
        }
    }?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let path = std::path::PathBuf::from(path);
    path.is_file().then_some(path)
}

/// Native folder dialog (zenity / kdialog / osascript / PowerShell).
pub fn pick_folder(title: &str) -> Option<std::path::PathBuf> {
    let output = {
        #[cfg(target_os = "linux")]
        {
            let start = dialog_start_dir();
            let mut kdialog = Command::new("kdialog");
            kdialog.args(["--title", title, "--getexistingdirectory", &start]);
            let mut zenity = Command::new("zenity");
            zenity.args(["--file-selection", "--directory", "--title", title]);
            linux_dialog(kdialog, zenity)
        }
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("osascript")
                .args([
                    "-e",
                    &format!(
                        "POSIX path of (choose folder with prompt {})",
                        applescript_string(title)
                    ),
                ])
                .output()
                .ok()
                .filter(|o| o.status.success())
        }
        #[cfg(windows)]
        {
            let script = format!(
                "{UTF8_OUTPUT}Add-Type -AssemblyName System.Windows.Forms; $f = New-Object System.Windows.Forms.FolderBrowserDialog; $f.Description = {title}; if ($f.ShowDialog() -eq 'OK') {{ $f.SelectedPath }}",
                title = powershell_string(title),
            );
            powershell()
                .args(["-NoProfile", "-Command", &script])
                .output()
                .ok()
                .filter(|o| o.status.success())
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
        {
            Option::<std::process::Output>::None
        }
    }?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    let path = std::path::PathBuf::from(path);
    let _ = std::fs::create_dir_all(&path);
    path.is_dir().then_some(path)
}

#[cfg(target_os = "linux")]
fn dialog_start_dir() -> String {
    std::env::var("HOME").unwrap_or_else(|_| ".".into())
}

/// KDE/Plasma uses kdialog first so the native file picker opens once.
#[cfg(target_os = "linux")]
fn linux_dialog(kdialog: Command, zenity: Command) -> Option<Output> {
    if linux_is_kde() {
        run_or_fallback(kdialog, zenity)
    } else {
        run_or_fallback(zenity, kdialog)
    }
}

#[cfg(target_os = "linux")]
fn linux_is_kde() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_ascii_uppercase()
        .split(':')
        .any(|d| d == "KDE")
        || std::env::var("DESKTOP_SESSION")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .contains("plasma")
}

/// Missing binary → try `fallback`. The tool ran (ok or cancel) → stop.
#[cfg(target_os = "linux")]
fn run_or_fallback(mut preferred: Command, mut fallback: Command) -> Option<Output> {
    match preferred.output() {
        Ok(out) => out.status.success().then_some(out),
        Err(e) if e.kind() == ErrorKind::NotFound => match fallback.output() {
            Ok(out) => out.status.success().then_some(out),
            Err(_) => None,
        },
        Err(_) => None,
    }
}

/// `Util.getPlatform().openUri`.
pub fn open_url(url: &str) {
    if let Err(e) = open::that(url) {
        crate::warn!("Could not open {url}: {e:#}");
    }
}

/// Makes `path` if it is missing, then opens it in the file manager.
pub fn open_dir(path: &std::path::Path) {
    if let Err(e) = std::fs::create_dir_all(path) {
        crate::warn!("Could not create {}: {e}", path.display());
        return;
    }
    if let Err(e) = open::that(path) {
        crate::warn!("Could not open {}: {e}", path.display());
    }
}

/// One clipboard for the process: on X11 and Wayland copied text lives only
/// as long as the `Clipboard` that set it (unless a clipboard manager takes it).
fn with_clipboard<R>(f: impl FnOnce(&mut arboard::Clipboard) -> R) -> Result<R, arboard::Error> {
    static CLIPBOARD: std::sync::Mutex<Option<arboard::Clipboard>> = std::sync::Mutex::new(None);
    let mut slot = CLIPBOARD
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let clipboard = match &mut *slot {
        Some(clipboard) => clipboard,
        empty => empty.insert(arboard::Clipboard::new()?),
    };
    Ok(f(clipboard))
}

/// `KeyboardHandler.setClipboard`.
pub fn copy_to_clipboard(text: &str) {
    if let Err(e) = with_clipboard(|clipboard| clipboard.set_text(text.to_owned())) {
        crate::warn!("Clipboard unavailable: {e}");
    }
}

/// `KeyboardHandler.getClipboard`: `None` when empty or unavailable.
pub fn clipboard_text() -> Option<String> {
    with_clipboard(|clipboard| clipboard.get_text().ok())
        .ok()
        .flatten()
}

/// The processor's marketing name (`GLX._getCpuInfo()`).
pub fn cpu_name() -> Option<String> {
    #[cfg(target_os = "linux")]
    if let Some(name) = std::fs::read_to_string("/proc/cpuinfo")
        .ok()
        .and_then(|info| {
            info.lines()
                .find(|line| line.starts_with("model name"))
                .and_then(|line| line.split_once(':'))
                .map(|(_, name)| name.trim().to_owned())
        })
    {
        return Some(name);
    }
    cpuid_name()
}

/// The brand string CPUID leaves `0x8000_0002..=4` hold.
#[cfg(target_arch = "x86_64")]
fn cpuid_name() -> Option<String> {
    use std::arch::x86_64::__cpuid;
    if __cpuid(0x8000_0000).eax < 0x8000_0004 {
        return None;
    }
    let mut bytes = Vec::with_capacity(48);
    for leaf in 0x8000_0002..=0x8000_0004 {
        let r = __cpuid(leaf);
        for reg in [r.eax, r.ebx, r.ecx, r.edx] {
            bytes.extend_from_slice(&reg.to_le_bytes());
        }
    }
    let text = String::from_utf8_lossy(&bytes);
    let name = text.trim_matches(|c: char| c == '\0' || c.is_whitespace());
    (!name.is_empty()).then(|| name.split_whitespace().collect::<Vec<_>>().join(" "))
}

#[cfg(not(target_arch = "x86_64"))]
fn cpuid_name() -> Option<String> {
    None
}

/// `PROCESS_MEMORY_COUNTERS` and `K32GetProcessMemoryInfo`, declared once.
#[cfg(windows)]
mod win_memory {
    #[repr(C)]
    #[derive(Default)]
    pub struct ProcessMemoryCounters {
        pub cb: u32,
        pub page_fault_count: u32,
        pub peak_working_set_size: usize,
        pub working_set_size: usize,
        pub quota: [usize; 4],
        pub pagefile_usage: usize,
        pub peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(
            process: isize,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    /// This process's counters.
    pub fn counters() -> Option<ProcessMemoryCounters> {
        let mut c = ProcessMemoryCounters {
            cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
            ..Default::default()
        };
        let ok = unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) };
        (ok != 0).then_some(c)
    }
}

/// Resident set size of this process.
pub fn resident_set_bytes() -> Option<u64> {
    // `VmRSS`: `statm` counts pages, which are 16 or 64 KiB on some ARM kernels.
    #[cfg(target_os = "linux")]
    {
        memory_details().map(|details| details.resident)
    }
    #[cfg(windows)]
    {
        win_memory::counters().map(|c| c.working_set_size as u64)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    None
}

/// `DebugEntryDetailedMemory`'s Rust side: where the process's memory is,
/// in bytes. Linux reads `/proc/self/status`; Windows has no split between
/// anonymous and mapped memory, so those stay `None`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryDetails {
    /// In RAM now (`VmRSS` / working set).
    pub resident: u64,
    /// The most ever in RAM (`VmHWM` / peak working set).
    pub peak_resident: u64,
    /// Private memory committed (`VmData` / pagefile usage): the heap's share.
    pub committed: u64,
    /// Address space reserved (`VmSize`).
    pub reserved: Option<u64>,
    /// In RAM: anonymous (heap, stacks) and backed by files (code, mapped).
    pub anonymous: Option<u64>,
    pub file: Option<u64>,
    pub swapped: Option<u64>,
}

pub fn memory_details() -> Option<MemoryDetails> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        parse_proc_status(&status)
    }
    #[cfg(windows)]
    {
        win_memory::counters().map(|c| MemoryDetails {
            resident: c.working_set_size as u64,
            peak_resident: c.peak_working_set_size as u64,
            committed: c.pagefile_usage as u64,
            ..MemoryDetails::default()
        })
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    None
}

/// `/proc/self/status`'s memory lines, in kB there.
#[cfg(any(target_os = "linux", test))]
fn parse_proc_status(status: &str) -> Option<MemoryDetails> {
    let field = |name: &str| {
        status.lines().find_map(|line| {
            let rest = line.strip_prefix(name)?.strip_prefix(':')?;
            let kb: u64 = rest.trim().trim_end_matches("kB").trim().parse().ok()?;
            Some(kb * 1024)
        })
    };
    let file = match (field("RssFile"), field("RssShmem")) {
        (Some(f), s) => Some(f + s.unwrap_or(0)),
        (None, s) => s,
    };
    Some(MemoryDetails {
        resident: field("VmRSS")?,
        peak_resident: field("VmHWM").unwrap_or(0),
        committed: field("VmData").unwrap_or(0),
        reserved: field("VmSize"),
        anonymous: field("RssAnon"),
        file,
        swapped: field("VmSwap"),
    })
}

#[cfg(test)]
mod memory_tests {
    #[test]
    fn proc_status_reads_in_bytes() {
        let status = "Name:\tmelon\nVmSize:\t  2048 kB\nVmHWM:\t   300 kB\nVmRSS:\t   200 kB\nVmData:\t  1000 kB\nRssAnon:\t  150 kB\nRssFile:\t  40 kB\nRssShmem:\t  10 kB\nVmSwap:\t  0 kB\n";
        let m = super::parse_proc_status(status).unwrap();
        assert_eq!(m.resident, 200 * 1024);
        assert_eq!(m.peak_resident, 300 * 1024);
        assert_eq!(m.committed, 1000 * 1024);
        assert_eq!(m.reserved, Some(2048 * 1024));
        assert_eq!(m.anonymous, Some(150 * 1024));
        assert_eq!(m.file, Some(50 * 1024));
        assert_eq!(m.swapped, Some(0));
    }
}

#[cfg(test)]
mod stack_tests {
    #[test]
    fn the_dropped_stack_is_the_one_cargo_sets() {
        let config = include_str!("../../../.cargo/config.toml");
        let line = format!("RUST_MIN_STACK = \"{}\"", super::CARGO_STACK);
        assert!(config.contains(&line));
    }
}

/// The program's name for the desktop: the `.desktop` file, the app id.
#[cfg(target_os = "linux")]
const APP_ID: &str = "melon";

/// Creates a window in the middle of its monitor.
pub fn create_centered_window(
    event_loop: &winit::event_loop::ActiveEventLoop,
    attributes: winit::window::WindowAttributes,
) -> Result<winit::window::Window, winit::error::OsError> {
    let attributes = attributes.with_window_icon(window_icon());
    // winit sets only `ICON_SMALL` from that; the taskbar shows `ICON_BIG`.
    #[cfg(windows)]
    let attributes = {
        use winit::platform::windows::WindowAttributesExtWindows;
        attributes.with_taskbar_icon(window_icon())
    };
    // TODO(macos): winit ignores window icons there, so the Dock shows none.
    // Ship an `.app` bundle with an `.icns`, or set
    // `NSApplication.applicationIconImage` at startup (`objc2-app-kit`).
    #[cfg(target_os = "linux")]
    let attributes = {
        use winit::platform::wayland::WindowAttributesExtWayland;
        use winit::platform::x11::WindowAttributesExtX11;
        WindowAttributesExtX11::with_name(
            WindowAttributesExtWayland::with_name(attributes, APP_ID, APP_ID),
            APP_ID,
            APP_ID,
        )
    };
    let window = event_loop.create_window(attributes.with_visible(false))?;
    let monitor = window
        .current_monitor()
        .or_else(|| event_loop.primary_monitor());
    if let Some(monitor) = monitor {
        let (origin, area, size) = (monitor.position(), monitor.size(), window.outer_size());
        let centre = |origin: i32, area: u32, size: u32| {
            origin + (i64::from(area) - i64::from(size)).max(0) as i32 / 2
        };
        window.set_outer_position(winit::dpi::PhysicalPosition::new(
            centre(origin.x, area.width, size.width),
            centre(origin.y, area.height, size.height),
        ));
    }
    window.set_visible(true);
    Ok(window)
}

/// How far [`lower_thread_priority`] lowers a thread: nice 10 of 19.
#[cfg(target_os = "linux")]
const BACKGROUND_NICE: i32 = 10;

/// `RUST_MIN_STACK`, as `.cargo/config.toml` sets it for rustc.
const CARGO_STACK: &str = "268435456";

/// Under `cargo run`, drops the compiler's 256 MiB `RUST_MIN_STACK`, so
/// the game's threads get the default stacks a shipped build has. Call
/// before any thread starts.
pub fn drop_cargo_stack() {
    if std::env::var_os("CARGO").is_some()
        && std::env::var("RUST_MIN_STACK").is_ok_and(|size| size == CARGO_STACK)
    {
        // SAFETY: no other thread exists yet, so none reads the environment.
        unsafe { std::env::remove_var("RUST_MIN_STACK") };
    }
}

/// The name and time on a core (user and kernel, in ns) of every thread of
/// this process. The calling thread is named `melon`, as Linux names the main
/// thread after the process.
#[cfg(windows)]
pub fn thread_cpu_times() -> Vec<(String, u64)> {
    win_threads::all()
}

/// The calling thread's time on a core: user and kernel, in ns.
#[cfg(windows)]
pub fn current_thread_cpu() -> Option<(u64, u64)> {
    win_threads::current()
}

/// Kernel32's thread snapshot and times, declared once.
#[cfg(windows)]
mod win_threads {
    use std::ffi::c_void;

    #[repr(C)]
    struct ThreadEntry32 {
        size: u32,
        usage: u32,
        thread_id: u32,
        owner_process_id: u32,
        base_priority: i32,
        delta_priority: i32,
        flags: u32,
    }

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct FileTime {
        low: u32,
        high: u32,
    }

    impl FileTime {
        fn nanos(self) -> u64 {
            ((u64::from(self.high) << 32) | u64::from(self.low)) * 100
        }
    }

    const TH32CS_SNAPTHREAD: u32 = 0x4;
    const THREAD_QUERY_LIMITED_INFORMATION: u32 = 0x0800;
    const INVALID_HANDLE_VALUE: isize = -1;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, process: u32) -> isize;
        fn Thread32First(snapshot: isize, entry: *mut ThreadEntry32) -> i32;
        fn Thread32Next(snapshot: isize, entry: *mut ThreadEntry32) -> i32;
        fn OpenThread(access: u32, inherit: i32, id: u32) -> isize;
        fn GetThreadTimes(
            thread: isize,
            creation: *mut FileTime,
            exit: *mut FileTime,
            kernel: *mut FileTime,
            user: *mut FileTime,
        ) -> i32;
        fn CloseHandle(handle: isize) -> i32;
        fn GetCurrentProcessId() -> u32;
        fn GetCurrentThreadId() -> u32;
        fn GetCurrentThread() -> isize;
        fn SetThreadPriority(thread: isize, priority: i32) -> i32;
        fn GetModuleHandleW(name: *const u16) -> isize;
        fn GetProcAddress(module: isize, name: *const u8) -> *const c_void;
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }

    /// `GetThreadDescription` (Windows 10 1607 on), looked up as std does.
    type GetThreadDescription = unsafe extern "system" fn(isize, *mut *mut u16) -> i32;

    fn get_thread_description() -> Option<GetThreadDescription> {
        static FOUND: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
        let address = (*FOUND.get_or_init(|| {
            let name: Vec<u16> = "kernel32.dll\0".encode_utf16().collect();
            unsafe {
                let module = GetModuleHandleW(name.as_ptr());
                if module == 0 {
                    return None;
                }
                let proc = GetProcAddress(module, c"GetThreadDescription".as_ptr().cast());
                (!proc.is_null()).then_some(proc as usize)
            }
        }))?;
        // SAFETY: the address is kernel32's `GetThreadDescription`.
        Some(unsafe { std::mem::transmute::<usize, GetThreadDescription>(address) })
    }

    /// User and kernel ns of an open thread handle.
    fn times(thread: isize) -> Option<(u64, u64)> {
        let mut t = [FileTime::default(); 4];
        let [creation, exit, kernel, user] = &mut t;
        let ok = unsafe { GetThreadTimes(thread, creation, exit, kernel, user) };
        (ok != 0).then(|| (user.nanos(), kernel.nanos()))
    }

    fn name(thread: isize) -> Option<String> {
        let describe = get_thread_description()?;
        let mut text: *mut u16 = std::ptr::null_mut();
        // SAFETY: on success `text` is a NUL-terminated string to `LocalFree`.
        unsafe {
            if describe(thread, &mut text) < 0 || text.is_null() {
                return None;
            }
            let len = (0..).take_while(|&i| *text.add(i) != 0).count();
            let name = String::from_utf16_lossy(std::slice::from_raw_parts(text, len));
            LocalFree(text.cast());
            Some(name)
        }
    }

    pub fn current() -> Option<(u64, u64)> {
        times(unsafe { GetCurrentThread() })
    }

    /// `THREAD_PRIORITY_BELOW_NORMAL` / `THREAD_PRIORITY_ABOVE_NORMAL`.
    pub const BELOW_NORMAL: i32 = -1;
    pub const ABOVE_NORMAL: i32 = 1;

    /// Sets the calling thread's priority; `what` names it in the warning.
    pub fn set_priority(priority: i32, what: &str) {
        if unsafe { SetThreadPriority(GetCurrentThread(), priority) } == 0 {
            crate::warn!(
                "Thread priority not {what}: {}",
                std::io::Error::last_os_error()
            );
        }
    }

    pub fn all() -> Vec<(String, u64)> {
        let mut out = Vec::new();
        unsafe {
            let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0);
            if snapshot == INVALID_HANDLE_VALUE {
                return out;
            }
            let process = GetCurrentProcessId();
            let me = GetCurrentThreadId();
            let mut entry = ThreadEntry32 {
                size: std::mem::size_of::<ThreadEntry32>() as u32,
                usage: 0,
                thread_id: 0,
                owner_process_id: 0,
                base_priority: 0,
                delta_priority: 0,
                flags: 0,
            };
            let mut more = Thread32First(snapshot, &mut entry) != 0;
            while more {
                if entry.owner_process_id == process {
                    let thread = OpenThread(THREAD_QUERY_LIMITED_INFORMATION, 0, entry.thread_id);
                    if thread != 0 {
                        if let Some((user, kernel)) = times(thread) {
                            let name = if entry.thread_id == me {
                                "melon".to_owned()
                            } else {
                                name(thread)
                                    .filter(|n| !n.is_empty())
                                    .unwrap_or_else(|| "unnamed".to_owned())
                            };
                            out.push((name, user + kernel));
                        }
                        CloseHandle(thread);
                    }
                }
                more = Thread32Next(snapshot, &mut entry) != 0;
            }
            CloseHandle(snapshot);
        }
        out
    }
}

/// Lets the calling thread give way when the CPU is busy.
pub fn lower_thread_priority() {
    #[cfg(target_os = "linux")]
    {
        unsafe extern "C" {
            fn getpriority(which: i32, who: u32) -> i32;
            fn setpriority(which: i32, who: u32, prio: i32) -> i32;
        }
        const PRIO_PROCESS: i32 = 0;
        unsafe {
            if getpriority(PRIO_PROCESS, 0) >= BACKGROUND_NICE {
                return;
            }
            if setpriority(PRIO_PROCESS, 0, BACKGROUND_NICE) != 0 {
                crate::warn!(
                    "Thread priority not lowered: {}",
                    std::io::Error::last_os_error()
                );
            }
        }
    }
    #[cfg(windows)]
    win_threads::set_priority(win_threads::BELOW_NORMAL, "lowered");
}

/// Ranks the calling thread above the throughput threads: one a frame waits
/// on (the main thread, the urgent meshers). Windows only, and only with
/// more than 4 cores, so the server tick still gets one; Linux can't raise a
/// thread without privileges and lowers the others instead.
pub fn raise_frame_thread_priority() {
    #[cfg(windows)]
    if std::thread::available_parallelism().is_ok_and(|n| n.get() > 4) {
        win_threads::set_priority(win_threads::ABOVE_NORMAL, "raised");
    }
}

#[cfg(test)]
mod dialog_script_tests {
    #[test]
    fn applescript_titles_escape_quotes_and_backslashes() {
        assert_eq!(super::applescript_string("Pick"), r#""Pick""#);
        assert_eq!(
            super::applescript_string(r#"A "skin" in C:\x"#),
            r#""A \"skin\" in C:\\x""#
        );
    }

    #[test]
    fn powershell_titles_double_every_quote() {
        assert_eq!(super::powershell_string("it's"), "'it''s'");
        assert_eq!(
            super::powershell_string("l\u{2019}image"),
            "'l\u{2019}\u{2019}image'"
        );
    }

    /// A path with an umlaut comes back intact, not in the OEM code page.
    #[cfg(windows)]
    #[test]
    fn powershell_output_is_utf8() {
        let script = format!("{}\"C:\\Users\\M$([char]0xFC)ller\"", super::UTF8_OUTPUT);
        let output = super::powershell()
            .args(["-NoProfile", "-Command", &script])
            .output()
            .expect("PowerShell runs");
        let text = String::from_utf8(output.stdout).expect("UTF-8 output");
        assert_eq!(text.trim(), "C:\\Users\\M\u{FC}ller");
    }
}

#[cfg(all(test, target_os = "linux"))]
mod dialog_tests {
    #[test]
    fn kde_desktop_is_detected_from_xdg() {
        let _env = crate::ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let prev_desktop = std::env::var_os("XDG_CURRENT_DESKTOP");
        let prev_session = std::env::var_os("DESKTOP_SESSION");
        unsafe {
            std::env::set_var("XDG_CURRENT_DESKTOP", "KDE");
            std::env::remove_var("DESKTOP_SESSION");
        }
        assert!(super::linux_is_kde());
        unsafe { std::env::set_var("XDG_CURRENT_DESKTOP", "GNOME") };
        assert!(!super::linux_is_kde());
        match prev_desktop {
            Some(v) => unsafe { std::env::set_var("XDG_CURRENT_DESKTOP", v) },
            None => unsafe { std::env::remove_var("XDG_CURRENT_DESKTOP") },
        }
        match prev_session {
            Some(v) => unsafe { std::env::set_var("DESKTOP_SESSION", v) },
            None => unsafe { std::env::remove_var("DESKTOP_SESSION") },
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    unsafe extern "C" {
        fn getpriority(which: i32, who: u32) -> i32;
    }

    fn own_nice() -> i32 {
        unsafe { getpriority(0, 0) }
    }

    #[test]
    fn lowers_only_the_calling_thread() {
        let before = own_nice();
        let lowered = std::thread::spawn(|| {
            super::lower_thread_priority();
            own_nice()
        })
        .join()
        .unwrap();
        assert_eq!(lowered, before.max(super::BACKGROUND_NICE));
        assert_eq!(own_nice(), before);
    }
}

#[cfg(all(test, any(windows, target_arch = "x86_64")))]
mod host_tests {
    #[test]
    fn names_the_cpu_and_the_memory_in_use() {
        let name = super::cpu_name().expect("CPU-Name");
        assert!(name.len() > 3, "{name:?}");
        #[cfg(any(windows, target_os = "linux"))]
        assert!(super::resident_set_bytes().is_some_and(|b| b > 1 << 20));
        crate::warn!("CPU: {name}");
    }
}

/// The program's icon.
fn window_icon() -> Option<winit::window::Icon> {
    let icon = image::load_from_memory(include_bytes!("icon.png"))
        .ok()?
        .into_rgba8();
    let (width, height) = icon.dimensions();
    winit::window::Icon::from_rgba(icon.into_raw(), width, height).ok()
}
