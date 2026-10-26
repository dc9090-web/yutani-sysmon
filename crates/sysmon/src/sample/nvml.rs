//! NVIDIA's proprietary driver, through NVML (`libnvidia-ml.so.1`, shipped
//! with the driver). Its sysfs has no busy, VRAM or sensor files.
//!
//! The library is opened on the first reading of an awake NVIDIA card,
//! never at start: initialising NVML wakes a runtime-suspended GPU.

use std::ffi::{CString, c_char, c_int, c_uint, c_void};
use std::sync::OnceLock;

use libloading::Library;

type Device = *mut c_void;
type Ret = c_int;
const SUCCESS: Ret = 0;
const TEMPERATURE_GPU: c_int = 0;
const CLOCK_GRAPHICS: c_int = 0;

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

struct Nvml {
    // Keeps the symbols below valid.
    _lib: Library,
    by_bus_id: unsafe extern "C" fn(*const c_char, *mut Device) -> Ret,
    name: unsafe extern "C" fn(Device, *mut c_char, c_uint) -> Ret,
    utilization: unsafe extern "C" fn(Device, *mut Utilization) -> Ret,
    memory: unsafe extern "C" fn(Device, *mut Memory) -> Ret,
    temperature: unsafe extern "C" fn(Device, c_int, *mut c_uint) -> Ret,
    clock: unsafe extern "C" fn(Device, c_int, *mut c_uint) -> Ret,
    power: unsafe extern "C" fn(Device, *mut c_uint) -> Ret,
    power_limit: unsafe extern "C" fn(Device, *mut c_uint) -> Ret,
}

/// What one call reads. Every field is optional: older or consumer cards
/// leave some unsupported.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Reading {
    pub busy: Option<f32>,
    pub temp: Option<f32>,
    pub vram_used: Option<u64>,
    pub vram_total: Option<u64>,
    pub clock_mhz: Option<f32>,
    pub power_w: Option<f32>,
    pub cap_w: Option<f32>,
}

fn nvml() -> Option<&'static Nvml> {
    static NVML: OnceLock<Option<Nvml>> = OnceLock::new();
    NVML.get_or_init(|| match load() {
        Ok(n) => Some(n),
        Err(e) => {
            tracing::warn!("NVIDIA GPU found but NVML isn't usable: {e}");
            None
        }
    })
    .as_ref()
}

fn load() -> Result<Nvml, String> {
    // SAFETY: NVML is a plain C library with no load-time side effects
    // beyond its own init, which we call explicitly.
    let lib = unsafe { Library::new("libnvidia-ml.so.1") }.map_err(|e| e.to_string())?;
    // SAFETY: the signatures match nvml.h (v2 entry points where they exist).
    unsafe {
        let init: unsafe extern "C" fn() -> Ret = *lib.get(b"nvmlInit_v2\0").map_err(|e| e.to_string())?;
        let ret = init();
        if ret != SUCCESS {
            return Err(format!("nvmlInit_v2 returned {ret}"));
        }
        macro_rules! sym {
            ($name:literal) => {
                *lib.get(concat!($name, "\0").as_bytes()).map_err(|e| e.to_string())?
            };
        }
        Ok(Nvml {
            by_bus_id: sym!("nvmlDeviceGetHandleByPciBusId_v2"),
            name: sym!("nvmlDeviceGetName"),
            utilization: sym!("nvmlDeviceGetUtilizationRates"),
            memory: sym!("nvmlDeviceGetMemoryInfo"),
            temperature: sym!("nvmlDeviceGetTemperature"),
            clock: sym!("nvmlDeviceGetClockInfo"),
            power: sym!("nvmlDeviceGetPowerUsage"),
            power_limit: sym!("nvmlDeviceGetEnforcedPowerLimit"),
            _lib: lib,
        })
    }
}

/// sysfs `0000:01:00.0` → NVML's `00000000:01:00.0`.
fn bus_id(slot: &str) -> String {
    match slot.split_once(':') {
        Some((domain, rest)) if domain.len() < 8 => format!("{domain:0>8}:{rest}"),
        _ => slot.to_owned(),
    }
}

fn device(n: &Nvml, slot: &str) -> Option<Device> {
    let id = CString::new(bus_id(slot)).ok()?;
    let mut dev: Device = std::ptr::null_mut();
    // SAFETY: `id` is NUL-terminated and `dev` is a valid out pointer.
    (unsafe { (n.by_bus_id)(id.as_ptr(), &mut dev) } == SUCCESS).then_some(dev)
}

/// Busy and temperature; with `full`, also VRAM, clock and power.
pub fn read(slot: &str, full: bool) -> Reading {
    let Some(n) = nvml() else { return Reading::default() };
    let Some(dev) = device(n, slot) else { return Reading::default() };
    let u32_of = |f: &dyn Fn(*mut c_uint) -> Ret| {
        let mut v: c_uint = 0;
        (f(&mut v) == SUCCESS).then_some(v)
    };
    let mut r = Reading::default();
    // SAFETY (all calls below): `dev` came from NVML and the out pointers
    // are valid for the struct or integer each call writes.
    let mut util = Utilization::default();
    if unsafe { (n.utilization)(dev, &mut util) } == SUCCESS {
        r.busy = Some(util.gpu.min(100) as f32);
    }
    r.temp = u32_of(&|p| unsafe { (n.temperature)(dev, TEMPERATURE_GPU, p) }).map(|c| c as f32);
    if full {
        let mut mem = Memory::default();
        if unsafe { (n.memory)(dev, &mut mem) } == SUCCESS {
            (r.vram_used, r.vram_total) = (Some(mem.used), Some(mem.total));
        }
        r.clock_mhz = u32_of(&|p| unsafe { (n.clock)(dev, CLOCK_GRAPHICS, p) }).map(|m| m as f32);
        r.power_w = u32_of(&|p| unsafe { (n.power)(dev, p) }).map(|mw| mw as f32 / 1000.0);
        r.cap_w = u32_of(&|p| unsafe { (n.power_limit)(dev, p) }).map(|mw| mw as f32 / 1000.0);
    }
    r
}

/// The product name, e.g. "NVIDIA GeForce RTX 4070".
pub fn name(slot: &str) -> Option<String> {
    let n = nvml()?;
    let dev = device(n, slot)?;
    let mut buf = [0 as c_char; 96];
    // SAFETY: NVML writes at most `len` bytes including the NUL.
    if unsafe { (n.name)(dev, buf.as_mut_ptr(), buf.len() as c_uint) } != SUCCESS {
        return None;
    }
    // SAFETY: NUL-terminated on success, within `buf`.
    let s = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr()) }.to_string_lossy().trim().to_owned();
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bus_ids() {
        assert_eq!(bus_id("0000:01:00.0"), "00000000:01:00.0");
        assert_eq!(bus_id("00000000:01:00.0"), "00000000:01:00.0");
        assert_eq!(bus_id("weird"), "weird");
    }
}
