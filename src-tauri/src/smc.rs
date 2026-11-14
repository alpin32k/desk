//! macOS: SMC keys through the AppleSMC driver (IOKit), as Stats and smcFanControl
//! read them. Reading needs no rights; writing needs root (the `desk-fans`
//! helper). Shared with the helper through `#[path]`: no `tr!`, nothing of desk.
//! Only the key types the fans use on Apple Silicon: `ui8 ` and `flt `
//! (little-endian f32). Any other type is an error, so a Mac that keeps its
//! fan keys differently (Intel's `fpe2`) counts as one desk cannot drive.

use crate::fan_control::Smc;
use std::ffi::{c_char, c_void};

type KernReturn = i32;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> *mut c_void;
    fn IOServiceGetMatchingService(main_port: u32, matching: *mut c_void) -> u32;
    fn IOServiceOpen(service: u32, owning_task: u32, kind: u32, connect: *mut u32) -> KernReturn;
    fn IOServiceClose(connect: u32) -> KernReturn;
    fn IOObjectRelease(object: u32) -> KernReturn;
    fn IOConnectCallStructMethod(
        connection: u32,
        selector: u32,
        input: *const c_void,
        input_size: usize,
        output: *mut c_void,
        output_size: *mut usize,
    ) -> KernReturn;
}

unsafe extern "C" {
    /// What C's `mach_task_self()` macro reads.
    static mach_task_self_: u32;
}

/// The driver's one struct method for keys.
const KERNEL_INDEX: u32 = 2;
const CMD_READ_BYTES: u8 = 5;
const CMD_WRITE_BYTES: u8 = 6;
const CMD_READ_KEYINFO: u8 = 9;
const TYPE_UI8: u32 = u32::from_be_bytes(*b"ui8 ");
const TYPE_FLT: u32 = u32::from_be_bytes(*b"flt ");

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Version {
    major: u8,
    minor: u8,
    build: u8,
    reserved: u8,
    release: u16,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct PLimit {
    version: u16,
    length: u16,
    cpu: u32,
    gpu: u32,
    mem: u32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct KeyInfo {
    data_size: u32,
    data_type: u32,
    attributes: u8,
}

/// AppleSMC's `SMCKeyData_t` (80 bytes).
#[repr(C)]
#[derive(Default, Clone, Copy)]
struct KeyData {
    key: u32,
    vers: Version,
    p_limit: PLimit,
    key_info: KeyInfo,
    result: u8,
    status: u8,
    data8: u8,
    data32: u32,
    bytes: [u8; 32],
}

fn fourcc(key: &str) -> Result<u32, String> {
    let b: [u8; 4] = key.as_bytes().try_into().map_err(|_| format!("{key}: not a 4-character key"))?;
    Ok(u32::from_be_bytes(b))
}

/// An open connection to the SMC driver.
pub struct IoSmc {
    conn: u32,
}

impl IoSmc {
    pub fn open() -> Result<IoSmc, String> {
        // SAFETY: IOKit calls with valid arguments; the matching dictionary is
        // consumed by IOServiceGetMatchingService, the service released here.
        unsafe {
            let service = IOServiceGetMatchingService(0, IOServiceMatching(c"AppleSMC".as_ptr()));
            if service == 0 {
                return Err("no AppleSMC".into());
            }
            let mut conn = 0;
            let kr = IOServiceOpen(service, mach_task_self_, 0, &mut conn);
            IOObjectRelease(service);
            if kr != 0 {
                return Err(format!("AppleSMC: open failed ({kr:#x})"));
            }
            Ok(IoSmc { conn })
        }
    }

    fn call(&self, input: &KeyData) -> Result<KeyData, KernReturn> {
        let mut output = KeyData::default();
        let mut size = size_of::<KeyData>();
        // SAFETY: both buffers are KeyData, the size the driver expects.
        let kr = unsafe {
            IOConnectCallStructMethod(
                self.conn,
                KERNEL_INDEX,
                (input as *const KeyData).cast(),
                size_of::<KeyData>(),
                (&mut output as *mut KeyData).cast(),
                &mut size,
            )
        };
        if kr != 0 { Err(kr) } else { Ok(output) }
    }

    /// The key's size and type.
    fn info(&self, key: &str) -> Result<KeyInfo, String> {
        let input = KeyData {
            key: fourcc(key)?,
            data8: CMD_READ_KEYINFO,
            ..Default::default()
        };
        match self.call(&input) {
            Ok(out) if out.result == 0 => Ok(out.key_info),
            Ok(out) => Err(format!("{key}: no such key ({})", out.result)),
            Err(kr) => Err(format!("{key}: read failed ({kr:#x})")),
        }
    }

    fn read_typed(&self, key: &str, kind: u32) -> Result<[u8; 32], String> {
        let info = self.info(key)?;
        if info.data_type != kind {
            return Err(format!("{key}: unexpected type"));
        }
        let input = KeyData {
            key: fourcc(key)?,
            key_info: info,
            data8: CMD_READ_BYTES,
            ..Default::default()
        };
        match self.call(&input) {
            Ok(out) if out.result == 0 => Ok(out.bytes),
            Ok(out) => Err(format!("{key}: read failed ({})", out.result)),
            Err(kr) => Err(format!("{key}: read failed ({kr:#x})")),
        }
    }

    fn write_typed(&self, key: &str, kind: u32, value: &[u8]) -> Result<(), String> {
        let info = self.info(key)?;
        if info.data_type != kind || info.data_size as usize != value.len() {
            return Err(format!("{key}: unexpected type"));
        }
        let mut input = KeyData {
            key: fourcc(key)?,
            key_info: info,
            data8: CMD_WRITE_BYTES,
            ..Default::default()
        };
        input.bytes[..value.len()].copy_from_slice(value);
        match self.call(&input) {
            Ok(out) if out.result == 0 => Ok(()),
            Ok(out) => Err(format!("{key}: write refused ({})", out.result)),
            Err(kr) => Err(format!("{key}: write refused, needs root ({kr:#x})")),
        }
    }
}

impl Smc for IoSmc {
    fn read_u8(&mut self, key: &str) -> Result<u8, String> {
        Ok(self.read_typed(key, TYPE_UI8)?[0])
    }

    fn read_f32(&mut self, key: &str) -> Result<f32, String> {
        let b = self.read_typed(key, TYPE_FLT)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn write_u8(&mut self, key: &str, value: u8) -> Result<(), String> {
        self.write_typed(key, TYPE_UI8, &[value])
    }

    fn write_f32(&mut self, key: &str, value: f32) -> Result<(), String> {
        self.write_typed(key, TYPE_FLT, &value.to_le_bytes())
    }
}

impl Drop for IoSmc {
    fn drop(&mut self) {
        // SAFETY: the connection was opened by `open`.
        unsafe {
            IOServiceClose(self.conn);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fan_control::{self, Smc};

    #[test]
    fn smc_struct_is_80_bytes() {
        assert_eq!(std::mem::size_of::<KeyData>(), 80);
    }

    #[test]
    fn reads_this_mac() {
        let Ok(mut smc) = IoSmc::open() else {
            return; // no SMC here (a VM)
        };
        let fans = fan_control::read(&mut smc).unwrap();
        eprintln!("fans: {fans:?}");
        assert!(fans.iter().all(|f| f.max_rpm > 0.0), "{fans:?}");
    }

    #[test]
    fn writing_without_root_is_refused() {
        // SAFETY: plain syscall.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let Ok(mut smc) = IoSmc::open() else {
            return;
        };
        // 0 is today's value, so even a write that went through changes nothing.
        assert!(smc.write_u8("Ftst", 0).is_err());
    }
}
