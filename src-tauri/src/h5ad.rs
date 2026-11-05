use serde::Serialize;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{mpsc, Mutex},
    time::{Duration, Instant},
};

const MAX_OUTPUT: usize = 256 * 1024;
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: u64,
    pub status: String,
    pub percent: u8,
    pub phase: String,
    pub elapsed_ms: u64,
    pub timeout_seconds: u64,
    pub error: Option<String>,
    pub report: Option<Value>,
}
struct Inner {
    snapshot: Snapshot,
    child: Option<Child>,
    cancel: bool,
    started: Option<Instant>,
}
pub struct Worker {
    inner: Mutex<Inner>,
    destination: PathBuf,
}
impl Worker {
    pub fn new(folder: &Path) -> Self {
        let destination = folder.join("h5ad-metadata.json");
        let report = std::fs::read(&destination)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        Self {
            destination,
            inner: Mutex::new(Inner {
                snapshot: Snapshot {
                    id: 0,
                    status: "idle".into(),
                    percent: 0,
                    phase: "runtime".into(),
                    elapsed_ms: 0,
                    timeout_seconds: 120,
                    error: None,
                    report,
                },
                child: None,
                cancel: false,
                started: None,
            }),
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        let inner = self.inner.lock().unwrap();
        let mut snapshot = inner.snapshot.clone();
        if active(&snapshot.status) {
            snapshot.elapsed_ms = inner.started.map_or(0, |s| s.elapsed().as_millis() as u64);
        }
        snapshot
    }
    pub fn reserve(&self, timeout_seconds: u64) -> Result<(), String> {
        if !(1..=3600).contains(&timeout_seconds) {
            return Err("invalid_timeout".into());
        }
        let mut inner = self.inner.lock().unwrap();
        if active(&inner.snapshot.status) {
            return Err("worker_busy".into());
        }
        inner.snapshot.id += 1;
        inner.snapshot.status = "selecting".into();
        inner.snapshot.percent = 0;
        inner.snapshot.phase = "runtime".into();
        inner.snapshot.elapsed_ms = 0;
        inner.snapshot.error = None;
        inner.snapshot.timeout_seconds = timeout_seconds;
        inner.started = None;
        inner.cancel = false;
        Ok(())
    }
    pub fn cancel(&self) {
        let mut inner = self.inner.lock().unwrap();
        if active(&inner.snapshot.status) {
            inner.cancel = true;
            inner.snapshot.status = "cancelling".into();
        }
    }
    pub fn finish(&self, status: &str, error: Option<&str>) {
        let mut inner = self.inner.lock().unwrap();
        stop_child(&mut inner);
        finish(&mut inner, status, error);
    }
    pub fn run(&self, python: &Path, script: &str, path: &Path) {
        let mut command = Command::new(python);
        command
            .args(["-u", "-c", script])
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        self.run_command(command);
    }
    fn run_command(&self, mut command: Command) {
        let (stdout, stderr) = {
            let mut inner = self.inner.lock().unwrap();
            if inner.cancel {
                finish(&mut inner, "cancelled", None);
                return;
            }
            inner.started = Some(Instant::now());
            match command.spawn() {
                Ok(mut child) => {
                    let pipes = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
                    inner.child = Some(child);
                    inner.snapshot.status = "running".into();
                    pipes
                }
                Err(_) => {
                    finish(&mut inner, "failed", Some("python_unavailable"));
                    return;
                }
            }
        };
        let (tx, rx) = mpsc::sync_channel(32);
        let reader = std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut total = 0;
            loop {
                let mut bytes = Vec::new();
                let read = reader
                    .by_ref()
                    .take((MAX_OUTPUT + 1) as u64)
                    .read_until(b'\n', &mut bytes);
                match read {
                    Ok(0) => break,
                    Ok(n) => {
                        total += n;
                        if total > MAX_OUTPUT {
                            let _ = tx.send(Err("worker_output_too_large"));
                            break;
                        }
                        if tx
                            .send(
                                serde_json::from_slice::<Value>(&bytes)
                                    .map_err(|_| "worker_protocol_error"),
                            )
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(_) => {
                        let _ = tx.send(Err("worker_protocol_error"));
                        break;
                    }
                }
            }
        });
        // Drain stderr without retaining unbounded tracebacks or exposing file paths to UI.
        let errors = std::thread::spawn(move || {
            let _ = std::io::copy(&mut BufReader::new(stderr), &mut std::io::sink());
        });
        let mut report = None;
        let mut failure = None;
        let mut exited = None;
        loop {
            let message = rx.recv_timeout(Duration::from_millis(25));
            let mut inner = self.inner.lock().unwrap();
            if inner.cancel {
                stop_child(&mut inner);
                finish(&mut inner, "cancelled", None);
                break;
            }
            if inner.started.unwrap().elapsed()
                >= Duration::from_secs(inner.snapshot.timeout_seconds)
            {
                stop_child(&mut inner);
                finish(&mut inner, "timed_out", None);
                break;
            }
            let disconnected = matches!(&message, Err(mpsc::RecvTimeoutError::Disconnected));
            if let Ok(message) = message {
                match message {
                    Err(code) => failure = Some(code.to_string()),
                    Ok(value) => match value["type"].as_str() {
                        Some("progress") if report.is_none() => {
                            if let (Some(percent), Some(phase)) =
                                (value["percent"].as_u64(), value["phase"].as_str())
                            {
                                if percent <= 99
                                    && [
                                        "runtime",
                                        "checksum",
                                        "opening",
                                        "annotations",
                                        "validating",
                                    ]
                                    .contains(&phase)
                                {
                                    inner.snapshot.percent =
                                        inner.snapshot.percent.max(percent as u8);
                                    inner.snapshot.phase = phase.into();
                                } else {
                                    failure = Some("worker_protocol_error".into());
                                }
                            } else {
                                failure = Some("worker_protocol_error".into());
                            }
                        }
                        Some("result") if report.is_none() => {
                            report = Some(value["report"].clone())
                        }
                        Some("error") => {
                            failure = Some(
                                match value["code"].as_str() {
                                    Some("python_dependencies_missing") => {
                                        "python_dependencies_missing"
                                    }
                                    Some("invalid_h5ad") => "invalid_h5ad",
                                    _ => "worker_failed",
                                }
                                .into(),
                            )
                        }
                        _ => failure = Some("worker_protocol_error".into()),
                    },
                }
            }
            if let Some(code) = failure.as_deref() {
                stop_child(&mut inner);
                finish(&mut inner, "failed", Some(code));
                break;
            }
            if exited.is_none() {
                match inner.child.as_mut().unwrap().try_wait() {
                    Ok(Some(status)) => exited = Some(status.success()),
                    Ok(None) => {}
                    Err(_) => {
                        stop_child(&mut inner);
                        finish(&mut inner, "failed", Some("worker_failed"));
                        break;
                    }
                }
            }
            // EOF ensures the final result has been consumed, even after process exit.
            if disconnected && exited.is_some() {
                inner.child.take();
                if exited != Some(true) || report.as_ref().is_none_or(|r| !valid_report(r)) {
                    finish(&mut inner, "failed", Some("worker_protocol_error"));
                } else {
                    let report = report.take().unwrap();
                    let temporary = self.destination.with_extension("tmp");
                    let saved = serde_json::to_vec(&report)
                        .ok()
                        .and_then(|b| std::fs::write(&temporary, b).ok())
                        .and_then(|_| std::fs::rename(&temporary, &self.destination).ok());
                    if saved.is_some() {
                        inner.snapshot.report = Some(report);
                        inner.snapshot.percent = 100;
                        finish(&mut inner, "completed", None);
                    } else {
                        finish(&mut inner, "failed", Some("storage_error"));
                    }
                }
                break;
            }
        }
        drop(rx); // Unblock a writer if cancellation occurred while its queue was full.
        let _ = reader.join();
        let _ = errors.join();
    }
    pub fn shutdown(&self) {
        self.cancel();
        let mut inner = self.inner.lock().unwrap();
        stop_child(&mut inner);
    }
}
fn active(status: &str) -> bool {
    matches!(status, "selecting" | "running" | "cancelling")
}
fn stop_child(inner: &mut Inner) {
    if let Some(mut child) = inner.child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}
fn finish(inner: &mut Inner, status: &str, error: Option<&str>) {
    inner.snapshot.status = status.into();
    inner.snapshot.elapsed_ms = inner.started.map_or(0, |s| s.elapsed().as_millis() as u64);
    inner.snapshot.error = error.map(str::to_string);
}
fn valid_report(r: &Value) -> bool {
    r["schemaVersion"] == 1
        && r["cells"].as_u64().is_some_and(|n| n > 0)
        && r["genes"].as_u64().is_some_and(|n| n > 0)
        && r["name"].as_str().is_some_and(|s| s.len() <= 2000)
        && r["sha256"]
            .as_str()
            .is_some_and(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
        && r["groups"].is_object()
        && r["obsColumns"].is_array()
        && r["duplicateCellIds"].as_u64().is_some()
        && r["duplicateGeneIds"].as_u64().is_some()
}
pub fn python_path() -> PathBuf {
    if let Some(path) = std::env::var_os("CAUSALMED_PYTHON") {
        return path.into();
    }
    // Development convenience; distributable workers will be separately packaged.
    let local = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.venv/bin/python");
    if local.exists() {
        local
    } else {
        PathBuf::from("python3")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn command(code: &str) -> Command {
        let mut command = Command::new(python_path());
        command
            .args(["-u", "-c", code])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
    fn success() -> String {
        let report = serde_json::json!({"schemaVersion":1,"name":"test.h5ad","cells":10,"genes":20,"sha256":"a".repeat(64),"groups":{},"obsColumns":[],"duplicateCellIds":0,"duplicateGeneIds":0});
        format!("import json\nprint(json.dumps({{'type':'progress','percent':50,'phase':'checksum'}}),flush=True)\nprint({:?},flush=True)", serde_json::json!({"type":"result","report":report}).to_string())
    }
    #[test]
    fn success_is_persisted_only_after_clean_exit_and_restored() {
        let folder = tempfile::tempdir().unwrap();
        let worker = Worker::new(folder.path());
        worker.reserve(10).unwrap();
        worker.run_command(command(&success()));
        assert_eq!(worker.snapshot().status, "completed");
        assert_eq!(worker.snapshot().percent, 100);
        assert_eq!(
            Worker::new(folder.path()).snapshot().report.unwrap()["cells"],
            10
        );
        let previous = std::fs::read(&worker.destination).unwrap();
        worker.reserve(10).unwrap();
        worker.run_command(command(
            &(success() + "\nraise RuntimeError('failure after result')"),
        ));
        assert_eq!(worker.snapshot().status, "failed");
        assert_eq!(std::fs::read(&worker.destination).unwrap(), previous);
    }
    #[test]
    fn timeout_reaps_child_and_allows_retry_without_replacing_report() {
        let folder = tempfile::tempdir().unwrap();
        let worker = Worker::new(folder.path());
        worker.reserve(10).unwrap();
        worker.run_command(command(&success()));
        let previous = std::fs::read(&worker.destination).unwrap();
        worker.reserve(1).unwrap();
        worker.run_command(command("import time; time.sleep(60)"));
        assert_eq!(worker.snapshot().status, "timed_out");
        assert!(worker.inner.lock().unwrap().child.is_none());
        assert_eq!(std::fs::read(&worker.destination).unwrap(), previous);
        worker.reserve(10).unwrap();
        worker.run_command(command(&success()));
        assert_eq!(worker.snapshot().status, "completed");
    }
    #[test]
    fn cancellation_during_running_reaps_child_and_prevents_concurrent_jobs() {
        let folder = tempfile::tempdir().unwrap();
        let worker = Worker::new(folder.path());
        worker.reserve(10).unwrap();
        std::thread::scope(|scope| {
            scope.spawn(|| worker.run_command(command("import json,time\nprint(json.dumps({'type':'progress','percent':25,'phase':'checksum'}),flush=True)\ntime.sleep(60)")));
            let start = Instant::now();
            while worker.snapshot().percent < 25 {
                assert!(start.elapsed() < Duration::from_secs(5));
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(worker.snapshot().phase, "checksum");
            assert!(worker.reserve(10).is_err());
            worker.cancel();
        });
        assert_eq!(worker.snapshot().status, "cancelled");
        assert!(worker.inner.lock().unwrap().child.is_none());
        assert!(!worker.destination.exists());
    }
    #[test]
    fn oversize_and_malformed_worker_output_fail_without_hanging() {
        let folder = tempfile::tempdir().unwrap();
        let worker = Worker::new(folder.path());
        for code in [
            "print('x' * 300000)",
            "print('{}')",
            "import json; print(json.dumps({'type':'result','report':{}}))",
        ] {
            worker.reserve(10).unwrap();
            worker.run_command(command(code));
            assert_eq!(worker.snapshot().status, "failed");
            assert!(!worker.destination.exists());
        }
    }
    #[test]
    fn selection_cancel_and_startup_failure_are_recoverable() {
        let folder = tempfile::tempdir().unwrap();
        let worker = Worker::new(folder.path());
        assert!(worker.reserve(0).is_err());
        worker.reserve(10).unwrap();
        worker.cancel();
        worker.run_command(command(&success()));
        assert_eq!(worker.snapshot().status, "cancelled");
        worker.reserve(10).unwrap();
        worker.run(
            Path::new("/nonexistent/causalmed-python"),
            "",
            Path::new("test.h5ad"),
        );
        assert_eq!(
            worker.snapshot().error.as_deref(),
            Some("python_unavailable")
        );
    }
}
