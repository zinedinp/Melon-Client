//! NVIDIA's management library (`libnvidia-ml`, NVML), loaded at run time
//! for the benchmark: how busy the GPU was while a phase ran. Without an
//! NVIDIA driver the library is missing and the summary leaves it out.
//!
//! NVML's utilization is the share of the driver's last sample period (1/6
//! to 1 s, depending on the GPU) in which the GPU had work: `gpu` while any
//! shader ran, `memory` while the memory controller read or wrote VRAM. It
//! says *that* the GPU was busy, not how fully: one warp running counts the
//! same as all of them. NVML needs no profiling rights.

use std::ffi::{CStr, c_char, c_uint, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// How often the sampling thread asks.
const INTERVAL: Duration = Duration::from_millis(100);

/// `nvmlReturn_t`: 0 is `NVML_SUCCESS`.
type Status = i32;
type DeviceHandle = *mut c_void;

#[repr(C)]
#[derive(Default)]
struct Utilization {
    gpu: c_uint,
    memory: c_uint,
}

#[repr(C)]
#[derive(Default)]
struct Memory {
    total: u64,
    free: u64,
    used: u64,
}

/// `nvmlClockType_t`.
const CLOCK_SM: c_uint = 1;
const CLOCK_MEM: c_uint = 2;

/// One reading.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GpuSample {
    /// Percent of the sample period with a shader running.
    pub gpu_util: u32,
    /// Percent of the sample period with VRAM being read or written.
    pub memory_util: u32,
    pub vram_used: u64,
    pub sm_mhz: u32,
    pub memory_mhz: u32,
    pub power_mw: u32,
}

/// The loaded library and the device it reads.
struct Nvml {
    /// Keeps the functions below valid.
    _library: libloading::Library,
    device: DeviceHandle,
    utilization: unsafe extern "C" fn(DeviceHandle, *mut Utilization) -> Status,
    memory: unsafe extern "C" fn(DeviceHandle, *mut Memory) -> Status,
    clock: unsafe extern "C" fn(DeviceHandle, c_uint, *mut c_uint) -> Status,
    power: unsafe extern "C" fn(DeviceHandle, *mut c_uint) -> Status,
    shutdown: unsafe extern "C" fn() -> Status,
}

// SAFETY: NVML is thread-safe, and the handle is only an opaque id.
unsafe impl Send for Nvml {}

/// NVML as the driver installs it: System32 on Windows, the loader's path on Linux.
const LIBRARY: Option<&str> = if cfg!(windows) {
    Some("nvml.dll")
} else if cfg!(target_os = "linux") {
    Some("libnvidia-ml.so.1")
} else {
    None
};

impl Nvml {
    /// Loads NVML and picks the GPU whose name contains `device_name` (the
    /// one Vulkan renders on), or the first one.
    fn open(device_name: &str) -> Option<Self> {
        // SAFETY: loading the system's NVML, whose functions are declared
        // below with the signatures of `nvml.h`.
        unsafe {
            let library = libloading::Library::new(LIBRARY?).ok()?;
            let init: libloading::Symbol<unsafe extern "C" fn() -> Status> =
                library.get(b"nvmlInit_v2").ok()?;
            if init() != 0 {
                return None;
            }
            let count: libloading::Symbol<unsafe extern "C" fn(*mut c_uint) -> Status> =
                library.get(b"nvmlDeviceGetCount_v2").ok()?;
            let by_index: libloading::Symbol<
                unsafe extern "C" fn(c_uint, *mut DeviceHandle) -> Status,
            > = library.get(b"nvmlDeviceGetHandleByIndex_v2").ok()?;
            let name_of: libloading::Symbol<
                unsafe extern "C" fn(DeviceHandle, *mut c_char, c_uint) -> Status,
            > = library.get(b"nvmlDeviceGetName").ok()?;
            let mut devices = 0;
            if count(&mut devices) != 0 || devices == 0 {
                return None;
            }
            let mut chosen = None;
            for index in 0..devices {
                let mut device = std::ptr::null_mut();
                if by_index(index, &mut device) != 0 {
                    continue;
                }
                let mut name = [0 as c_char; 96];
                let named = name_of(device, name.as_mut_ptr(), name.len() as c_uint) == 0
                    && CStr::from_ptr(name.as_ptr())
                        .to_string_lossy()
                        .contains(device_name.trim());
                if named || chosen.is_none() {
                    chosen = Some(device);
                }
                if named {
                    break;
                }
            }
            let device = chosen?;
            Some(Self {
                utilization: *library.get(b"nvmlDeviceGetUtilizationRates").ok()?,
                memory: *library.get(b"nvmlDeviceGetMemoryInfo").ok()?,
                clock: *library.get(b"nvmlDeviceGetClockInfo").ok()?,
                power: *library.get(b"nvmlDeviceGetPowerUsage").ok()?,
                shutdown: *library.get(b"nvmlShutdown").ok()?,
                device,
                _library: library,
            })
        }
    }

    /// One reading; a value NVML can't give stays 0.
    fn sample(&self) -> GpuSample {
        let mut utilization = Utilization::default();
        let mut memory = Memory::default();
        let (mut sm, mut mem, mut power) = (0, 0, 0);
        // SAFETY: the out-pointers live for the call; a failed call leaves
        // its default.
        unsafe {
            (self.utilization)(self.device, &mut utilization);
            (self.memory)(self.device, &mut memory);
            (self.clock)(self.device, CLOCK_SM, &mut sm);
            (self.clock)(self.device, CLOCK_MEM, &mut mem);
            (self.power)(self.device, &mut power);
        }
        GpuSample {
            gpu_util: utilization.gpu,
            memory_util: utilization.memory,
            vram_used: memory.used,
            sm_mhz: sm,
            memory_mhz: mem,
            power_mw: power,
        }
    }
}

impl Drop for Nvml {
    fn drop(&mut self) {
        // SAFETY: once, after the last call.
        unsafe { (self.shutdown)() };
    }
}

/// A thread reading NVML every [`INTERVAL`] for as long as it lives.
pub struct GpuMonitor {
    samples: Arc<Mutex<Vec<GpuSample>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl GpuMonitor {
    /// `None` without NVML (no NVIDIA driver).
    pub fn start(device_name: &str) -> Option<Self> {
        let nvml = Nvml::open(device_name)?;
        let samples = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = std::thread::Builder::new()
            .name("nvml".to_owned())
            .spawn({
                let (samples, stop) = (Arc::clone(&samples), Arc::clone(&stop));
                move || {
                    while !stop.load(Ordering::Relaxed) {
                        let sample = nvml.sample();
                        samples
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner)
                            .push(sample);
                        std::thread::sleep(INTERVAL);
                    }
                }
            })
            .ok()?;
        Some(Self {
            samples,
            stop,
            thread: Some(thread),
        })
    }

    /// The readings since the last call.
    pub fn take(&self) -> Vec<GpuSample> {
        std::mem::take(
            &mut *self
                .samples
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        )
    }
}

impl Drop for GpuMonitor {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
