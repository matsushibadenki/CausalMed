//! Per-project, machine-local worker budgets. RSS is sampled, not an allocator cap.
use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path, process::Command};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Limits {
    pub memory_mib: u64,
    pub cpu_seconds: u64,
    pub threads: u32,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            memory_mib: 2048,
            cpu_seconds: 300,
            threads: 2,
        }
    }
}
impl Limits {
    pub fn validate(&self) -> Result<(), String> {
        if !(64..=65536).contains(&self.memory_mib)
            || !(1..=3600).contains(&self.cpu_seconds)
            || !(1..=8).contains(&self.threads)
        {
            return Err("invalid_resource_limits".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub version: u32,
    pub revision: u64,
    pub limits: Limits,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            revision: 0,
            limits: Limits::default(),
        }
    }
}
pub fn load(folder: &Path) -> (Settings, bool) {
    let path = folder.join("worker-settings.json");
    if !path.exists() {
        return (Settings::default(), false);
    }
    let loaded = (|| {
        if std::fs::metadata(&path).ok()?.len() > 4096 {
            return None;
        }
        let s: Settings = serde_json::from_slice(&std::fs::read(path).ok()?).ok()?;
        (s.version == 1 && s.limits.validate().is_ok()).then_some(s)
    })();
    match loaded {
        Some(s) => (s, false),
        None => (Settings::default(), true),
    }
}
pub fn save(folder: &Path, settings: &Settings) -> Result<(), String> {
    settings.limits.validate()?;
    let write = (|| -> std::io::Result<()> {
        let mut file = tempfile::NamedTempFile::new_in(folder)?;
        file.write_all(&serde_json::to_vec(settings)?)?;
        file.as_file().sync_all()?;
        file.persist(folder.join("worker-settings.json"))
            .map_err(|e| e.error)?;
        Ok(())
    })();
    write.map_err(|_| "storage_error".into())
}
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub resident_bytes: u64,
    pub peak_resident_bytes: u64,
    pub cpu_millis: u64,
    pub samples: u64,
}
impl Usage {
    pub fn observe(&mut self, sample: Sample) {
        self.resident_bytes = sample.resident_bytes;
        self.peak_resident_bytes = self.peak_resident_bytes.max(sample.resident_bytes);
        self.cpu_millis = self.cpu_millis.max(sample.cpu_millis);
        self.samples += 1;
    }
    pub fn exceeded(&self, limits: &Limits) -> Option<&'static str> {
        if self.peak_resident_bytes > limits.memory_mib * 1024 * 1024 {
            Some("worker_memory_limit")
        } else if self.cpu_millis >= limits.cpu_seconds * 1000 {
            Some("worker_cpu_limit")
        } else {
            None
        }
    }
}
pub struct Sample {
    pub resident_bytes: u64,
    pub cpu_millis: u64,
}
pub fn supported() -> bool {
    cfg!(any(target_os = "macos", target_os = "linux"))
}
#[cfg(target_os = "macos")]
pub fn sample(pid: u32) -> Result<Sample, ()> {
    let mut info = std::mem::MaybeUninit::<libc::rusage_info_v2>::zeroed();
    // libproc writes exactly RUSAGE_INFO_V2 into this owned, aligned buffer.
    let result = unsafe {
        libc::proc_pid_rusage(pid as i32, libc::RUSAGE_INFO_V2, info.as_mut_ptr().cast())
    };
    if result != 0 {
        return Err(());
    }
    let info = unsafe { info.assume_init() };
    let mut timebase = mach2::mach_time::mach_timebase_info { numer: 0, denom: 0 };
    if unsafe { mach2::mach_time::mach_timebase_info(&mut timebase) } != 0 || timebase.denom == 0 {
        return Err(());
    }
    // proc_pid_rusage returns Mach absolute time, including on Apple silicon.
    let ticks = info.ri_user_time as u128 + info.ri_system_time as u128;
    let millis = ticks * timebase.numer as u128 / timebase.denom as u128 / 1_000_000;
    Ok(Sample {
        resident_bytes: info.ri_resident_size,
        cpu_millis: millis.min(u64::MAX as u128) as u64,
    })
}
#[cfg(target_os = "linux")]
pub fn sample(pid: u32) -> Result<Sample, ()> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).map_err(|_| ())?;
    let rss = status
        .lines()
        .find_map(|l| {
            l.strip_prefix("VmRSS:")
                .and_then(|v| v.split_whitespace().next()?.parse::<u64>().ok())
        })
        .ok_or(())?;
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).map_err(|_| ())?;
    let fields: Vec<_> = stat
        .rsplit_once(") ")
        .ok_or(())?
        .1
        .split_whitespace()
        .collect();
    let ticks = fields
        .get(11)
        .ok_or(())?
        .parse::<u64>()
        .map_err(|_| ())?
        .saturating_add(fields.get(12).ok_or(())?.parse::<u64>().map_err(|_| ())?);
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if hz <= 0 {
        return Err(());
    }
    Ok(Sample {
        resident_bytes: rss.saturating_mul(1024),
        cpu_millis: ticks.saturating_mul(1000) / hz as u64,
    })
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn sample(_: u32) -> Result<Sample, ()> {
    Err(())
}

pub fn configure(command: &mut Command, limits: &Limits) -> Result<(), String> {
    limits.validate()?;
    if !supported() {
        return Err("resource_monitor_unsupported".into());
    }
    for key in [
        "OMP_NUM_THREADS",
        "OPENBLAS_NUM_THREADS",
        "MKL_NUM_THREADS",
        "NUMEXPR_NUM_THREADS",
        "VECLIB_MAXIMUM_THREADS",
        "BLIS_NUM_THREADS",
    ] {
        command.env(key, limits.threads.to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let seconds = limits.cpu_seconds;
        // Only async-signal-safe setrlimit runs between fork and exec. A grace second
        // lets the parent label ordinary CPU-budget breaches before kernel fallback.
        unsafe {
            command.pre_exec(move || {
                let budget = libc::rlimit {
                    rlim_cur: (seconds + 1) as libc::rlim_t,
                    rlim_max: (seconds + 2) as libc::rlim_t,
                };
                if libc::setrlimit(libc::RLIMIT_CPU, &budget) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_are_bounded_atomic_and_project_local() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.revision = 1;
        s.limits.memory_mib = 512;
        save(a.path(), &s).unwrap();
        assert_eq!(load(a.path()).0.limits.memory_mib, 512);
        assert_eq!(load(b.path()).0.limits.memory_mib, 2048);
        s.limits.memory_mib = 0;
        assert!(save(a.path(), &s).is_err());
        assert_eq!(load(a.path()).0.limits.memory_mib, 512);
        std::fs::write(a.path().join("worker-settings.json"), b"{}").unwrap();
        assert!(load(a.path()).1);
        assert_eq!(load(a.path()).0.limits, Limits::default());
    }
    #[test]
    fn native_usage_is_available_and_limits_are_distinct() {
        if !supported() {
            return;
        }
        let s = sample(std::process::id()).unwrap();
        assert!(s.resident_bytes > 0);
        let mut usage = Usage::default();
        usage.observe(Sample {
            resident_bytes: 129 * 1024 * 1024,
            cpu_millis: 0,
        });
        let l = Limits {
            memory_mib: 128,
            cpu_seconds: 1,
            threads: 1,
        };
        assert_eq!(usage.exceeded(&l), Some("worker_memory_limit"));
        usage.peak_resident_bytes = 1;
        usage.cpu_millis = 1000;
        assert_eq!(usage.exceeded(&l), Some("worker_cpu_limit"));
    }
}
