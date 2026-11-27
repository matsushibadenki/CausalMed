//! Process accounting for an already-owned Windows child handle.
use super::Sample;
use std::{mem::size_of, os::windows::io::AsRawHandle, process::Child};
use windows_sys::Win32::{
    Foundation::{FILETIME, HANDLE},
    System::{
        ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS},
        Threading::GetProcessTimes,
    },
};

pub(super) fn sample_child(child: &Child) -> Result<Sample, ()> {
    sample_handle(child.as_raw_handle())
}

fn filetime_ticks(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

fn sample_handle(handle: HANDLE) -> Result<Sample, ()> {
    let size = size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    let mut memory = PROCESS_MEMORY_COUNTERS {
        cb: size,
        ..Default::default()
    };
    let mut created = FILETIME::default();
    let mut exited = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();

    // The child owns the handle for this call. Each API writes to an initialized
    // structure of the exact type and size required by Win32.
    let read = unsafe {
        K32GetProcessMemoryInfo(handle, &mut memory, size) != 0
            && GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) != 0
    };
    if !read {
        return Err(());
    }
    Ok(Sample {
        resident_bytes: memory.WorkingSetSize as u64,
        peak_resident_bytes: memory.PeakWorkingSetSize as u64,
        cpu_millis: filetime_ticks(kernel).saturating_add(filetime_ticks(user)) / 10_000,
    })
}

#[cfg(test)]
pub(super) fn sample_current_process() -> Result<Sample, ()> {
    use windows_sys::Win32::System::Threading::GetCurrentProcess;
    // The pseudo-handle refers to the calling process and needs no close.
    sample_handle(unsafe { GetCurrentProcess() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn accounting_remains_available_after_child_exit() {
        let mut child = Command::new("cmd.exe")
            .args(["/D", "/C", "exit", "0"])
            .spawn()
            .unwrap();
        assert!(child.wait().unwrap().success());
        assert!(sample_child(&child).unwrap().peak_resident_bytes > 0);
    }
}
