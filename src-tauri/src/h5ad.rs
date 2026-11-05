use serde::Serialize;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{mpsc, Mutex},
    time::{Duration, Instant},
};

use tempfile::{NamedTempFile, TempDir};

enum RunOutput {
    Relink {
        source: PathBuf,
        sha256: String,
    },
    Compare {
        left_id: String,
        right_id: String,
    },
    Analysis {
        source: Option<PathBuf>,
        artifacts: Option<TempDir>,
    },
    Export {
        destination: PathBuf,
        staged: NamedTempFile,
        selection_id: String,
        kind: String,
    },
}
pub struct ExportSource {
    pub source: PathBuf,
    pub directory: PathBuf,
    pub selection: Value,
}

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
    pub can_analyze: bool,
    pub operation: String,
    pub last_export: Option<Value>,
    pub comparison: Option<Value>,
    pub source_linked: bool,
    pub project_key: String,
    pub source_name: Option<String>,
}
struct Inner {
    snapshot: Snapshot,
    child: Option<Child>,
    cancel: bool,
    started: Option<Instant>,
    source_path: Option<PathBuf>,
}
#[cfg(test)]
thread_local! { pub(crate) static HISTORY_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
pub struct Worker {
    inner: Mutex<Inner>,
    destination: PathBuf,
}
impl Worker {
    pub fn new(folder: &Path) -> Self {
        let destination = folder.join("h5ad-metadata.json");
        let stored: Option<Value> = std::fs::read(&destination)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        let mut source_path = stored
            .as_ref()
            .and_then(|v| v["sourcePath"].as_str())
            .map(PathBuf::from);
        let report = stored.map(|v| {
            if v["report"].is_object() {
                v["report"].clone()
            } else {
                v
            }
        });
        let bindings: Value = std::fs::read(folder.join("source-bindings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(serde_json::json!({}));
        let sha = report
            .as_ref()
            .and_then(|r| r["sha256"].as_str())
            .unwrap_or("");
        if let Some(path) = bindings[sha].as_str() {
            source_path = Some(path.into());
        }
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
                    can_analyze: source_path.is_some(),
                    operation: "analysis".into(),
                    last_export: None,
                    comparison: None,
                    source_linked: false,
                    project_key: folder
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string(),
                    source_name: None,
                },
                child: None,
                cancel: false,
                started: None,
                source_path,
            }),
        }
    }
    pub(crate) fn folder(&self) -> &Path {
        self.destination.parent().unwrap()
    }
    fn bound_path(&self, sha: &str) -> Option<PathBuf> {
        let bindings: Value = std::fs::read(self.folder().join("source-bindings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())?;
        bindings[sha].as_str().map(PathBuf::from)
    }
    pub fn relink_sha(&self, id: &str) -> Result<String, String> {
        let record = self.history_record(id)?;
        record["report"]["sha256"]
            .as_str()
            .map(str::to_owned)
            .ok_or("selection_unavailable".into())
    }
    pub fn run_relink(&self, python: &Path, script: &str, source: &Path, sha256: &str) {
        self.inner.lock().unwrap().snapshot.operation = "relink".into();
        let job = serde_json::json!({"operation":"relink","sha256":sha256});
        self.run_command_from(
            worker_command(python, script, source, &serde_json::json!({}), "", &job),
            RunOutput::Relink {
                source: source.into(),
                sha256: sha256.into(),
            },
        );
    }
    pub fn snapshot(&self) -> Snapshot {
        let inner = self.inner.lock().unwrap();
        let mut snapshot = inner.snapshot.clone();
        snapshot.source_linked = inner.source_path.as_ref().is_some_and(|p| p.is_file());
        snapshot.can_analyze = snapshot.source_linked;
        snapshot.source_name = inner
            .source_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().to_string());
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
        inner.snapshot.operation = "analysis".into();
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
    pub fn analysis_source(&self, sha: &str) -> Result<PathBuf, String> {
        let inner = self.inner.lock().unwrap();
        if active(&inner.snapshot.status) {
            return Err("worker_busy".into());
        }
        if inner
            .snapshot
            .report
            .as_ref()
            .and_then(|r| r["sha256"].as_str())
            != Some(sha)
        {
            return Err("file_changed".into());
        }
        inner
            .source_path
            .clone()
            .ok_or_else(|| "source_unavailable".into())
    }
    pub fn run(&self, python: &Path, script: &str, path: &Path) {
        self.run_analysis(python, script, path, &serde_json::json!({}), "");
    }
    pub fn run_analysis(
        &self,
        python: &Path,
        script: &str,
        path: &Path,
        options: &Value,
        sha: &str,
    ) {
        let runs = self.destination.parent().unwrap().join("qc-runs");
        let artifacts = std::fs::create_dir_all(&runs).and_then(|_| {
            tempfile::Builder::new()
                .prefix(".pending-")
                .tempdir_in(&runs)
        });
        let artifacts = match artifacts {
            Ok(directory) => directory,
            Err(_) => {
                self.finish("failed", Some("storage_error"));
                return;
            }
        };
        let job = serde_json::json!({"artifactDir": artifacts.path()});
        let command = worker_command(python, script, path, options, sha, &job);
        self.run_command_from(
            command,
            RunOutput::Analysis {
                source: Some(path.to_path_buf()),
                artifacts: Some(artifacts),
            },
        );
    }
    pub fn export_source(&self, id: &str) -> Result<ExportSource, String> {
        let inner = self.inner.lock().unwrap();
        if active(&inner.snapshot.status) {
            return Err("worker_busy".into());
        }
        drop(inner);
        let stored = self.history_record(id)?;
        Ok(ExportSource {
            source: self
                .bound_path(stored["report"]["sha256"].as_str().unwrap_or(""))
                .or_else(|| stored["sourcePath"].as_str().map(PathBuf::from))
                .unwrap_or_default(),
            directory: self.destination.parent().unwrap().join("qc-runs").join(id),
            selection: stored["report"]["selection"].clone(),
        })
    }
    pub(crate) fn history_record(&self, id: &str) -> Result<Value, String> {
        if crate::trash::history_trashed(self.folder(), id) {
            return Err("selection_unavailable".into());
        }
        self.archived_record(id)
    }
    pub(crate) fn archived_record(&self, id: &str) -> Result<Value, String> {
        #[cfg(test)]
        HISTORY_READS.with(|n| n.set(n.get() + 1));
        if !safe_id(id) {
            return Err("selection_unavailable".into());
        }
        let directory = self.destination.parent().unwrap().join("qc-runs").join(id);
        let stored: Value = std::fs::read(directory.join("report.json"))
            .ok()
            .filter(|b| b.len() <= MAX_OUTPUT)
            .and_then(|b| serde_json::from_slice(&b).ok())
            .ok_or("selection_unavailable")?;
        let report = &stored["report"];
        let manifest =
            std::fs::read(directory.join("manifest.json")).map_err(|_| "selection_changed")?;
        let m: Value = serde_json::from_slice(&manifest).map_err(|_| "selection_changed")?;
        if !report["qc"].is_object()
            || !report["qc"]["options"].is_object()
            || m["source"]["cells"] != report["cells"]
            || m["source"]["genes"] != report["genes"]
            || report["selection"]["retained"] != report["qc"]["retained"]
            || report["selection"]["excluded"] != report["qc"]["excluded"]
            || report["selection"]["retained"]
                .as_u64()
                .zip(report["selection"]["excluded"].as_u64())
                .and_then(|(a, b)| a.checked_add(b))
                != report["cells"].as_u64()
            || m["qc"] != report["qc"]
            || m["source"]["sha256"] != report["sha256"]
            || m["analysisId"] != id
            || m["selectionSha256"] != report["selection"]["sha256"]
            || report["selection"]["id"] != id
            || Some(causalmed_core::checksum(&manifest).as_str())
                != report["selection"]["manifestSha256"].as_str()
        {
            return Err("selection_changed".into());
        }
        Ok(stored)
    }
    // Migrate previously committed archives using their original manifest and matching source metadata.
    pub(crate) fn visit_history(&self, mut accept: impl FnMut(Value)) {
        self.visit_archives(|r| {
            if r["selection"]["id"]
                .as_str()
                .is_some_and(|id| !crate::trash::history_trashed(self.folder(), id))
            {
                accept(r);
            }
        });
    }
    pub(crate) fn visit_archives(&self, mut accept: impl FnMut(Value)) {
        let current = {
            let inner = self.inner.lock().unwrap();
            serde_json::json!({"report":inner.snapshot.report,"sourcePath":inner.source_path})
        };
        let root = self.destination.parent().unwrap().join("qc-runs");
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let id = entry.file_name().to_string_lossy().to_string();
                if !safe_id(&id) {
                    continue;
                }
                if !entry.path().join("report.json").exists() {
                    if let Ok(bytes) = std::fs::read(entry.path().join("manifest.json")) {
                        if bytes.len() <= MAX_OUTPUT {
                            if let Ok(m) = serde_json::from_slice::<Value>(&bytes) {
                                if m["analysisId"] == id {
                                    let mut old = if m["source"]["sha256"]
                                        == current["report"]["sha256"]
                                    {
                                        current.clone()
                                    } else {
                                        serde_json::json!({"sourcePath":null,"report":{"name":m["source"]["name"],"sha256":m["source"]["sha256"],"cells":m["source"]["cells"],"genes":m["source"]["genes"],"obsColumns":[],"groups":{},"duplicateCellIds":0,"duplicateGeneIds":0}})
                                    };
                                    old["report"]["qc"] = m["qc"].clone();
                                    old["report"]["selection"] = serde_json::json!({"id":id,"createdAt":m["createdAt"],"sha256":m["selectionSha256"],"manifestSha256":causalmed_core::checksum(&bytes),"retained":m["qc"]["retained"],"excluded":m["qc"]["excluded"]});
                                    let _ = std::fs::write(
                                        entry.path().join("report.json"),
                                        old.to_string(),
                                    );
                                }
                            }
                        }
                    }
                }
                if let Ok(stored) = self.archived_record(&id) {
                    accept(stored["report"].clone());
                }
            }
        }
    }
    pub fn history(&self) -> Vec<Value> {
        let mut rows = Vec::new();
        self.visit_history(|report| rows.push(report));
        rows.sort_by(|a, b| {
            b["selection"]["createdAt"]
                .as_str()
                .cmp(&a["selection"]["createdAt"].as_str())
                .then_with(|| {
                    a["selection"]["id"]
                        .as_str()
                        .cmp(&b["selection"]["id"].as_str())
                })
        });
        rows
    }
    pub(crate) fn clear_history_selection(&self, id: &str) -> Result<(), String> {
        let mut inner = self.inner.lock().unwrap();
        if active(&inner.snapshot.status) {
            return Err("worker_busy".into());
        }
        if inner
            .snapshot
            .report
            .as_ref()
            .and_then(|r| r["selection"]["id"].as_str())
            == Some(id)
        {
            let mut report = inner.snapshot.report.clone().unwrap();
            if let Some(obj) = report.as_object_mut() {
                obj.remove("selection");
                obj.remove("qc");
            }
            let stored = serde_json::json!({"report":report,"sourcePath":inner.source_path});
            let mut stage =
                tempfile::NamedTempFile::new_in(self.folder()).map_err(|_| "storage_error")?;
            use std::io::Write;
            stage
                .write_all(stored.to_string().as_bytes())
                .and_then(|_| stage.as_file().sync_all())
                .map_err(|_| "storage_error")?;
            stage
                .persist(&self.destination)
                .map_err(|_| "storage_error")?;
            inner.snapshot.report = Some(report);
            inner.snapshot.last_export = None;
        }
        inner.snapshot.comparison = None;
        Ok(())
    }
    pub(crate) fn history_changed(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.snapshot.id += 1;
        inner.snapshot.operation = "analysis".into();
        inner.snapshot.comparison = None;
        finish(&mut inner, "idle", None);
    }
    pub fn select_history(&self, id: &str) -> Result<(), String> {
        let stored = self.history_record(id)?;
        let mut inner = self.inner.lock().unwrap();
        if active(&inner.snapshot.status) {
            return Err("worker_busy".into());
        }
        let temporary = self.destination.with_extension("tmp");
        std::fs::write(&temporary, stored.to_string())
            .and_then(|_| std::fs::rename(&temporary, &self.destination))
            .map_err(|_| "storage_error")?;
        inner.snapshot.report = Some(stored["report"].clone());
        inner.source_path = self
            .bound_path(stored["report"]["sha256"].as_str().unwrap_or(""))
            .or_else(|| stored["sourcePath"].as_str().map(PathBuf::from));
        inner.snapshot.can_analyze = inner.source_path.is_some();
        inner.snapshot.comparison = None;
        finish(&mut inner, "idle", None);
        Ok(())
    }
    pub fn compare_history(&self, python: &Path, script: &str, a: ExportSource, b: ExportSource) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.snapshot.comparison = None;
            inner.snapshot.operation = "compare".into();
        }
        let job = serde_json::json!({"operation":"compare","leftDir":a.directory,"left":a.selection,"rightDir":b.directory,"right":b.selection});
        self.run_command_from(
            worker_command(python, script, &a.source, &serde_json::json!({}), "", &job),
            RunOutput::Compare {
                left_id: a.selection["id"].as_str().unwrap().into(),
                right_id: b.selection["id"].as_str().unwrap().into(),
            },
        );
    }

    pub fn run_export(
        &self,
        python: &Path,
        script: &str,
        source: ExportSource,
        destination: &Path,
        kind: &str,
    ) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.snapshot.operation = kind.into();
        }
        let extension = if kind == "export_h5ad" { "h5ad" } else { "zip" };
        if !["export_h5ad", "export_qc"].contains(&kind)
            || destination.extension().and_then(|s| s.to_str()) != Some(extension)
        {
            self.finish("failed", Some("invalid_export"));
            return;
        }
        if std::fs::symlink_metadata(destination).is_ok() {
            self.finish("failed", Some("destination_exists"));
            return;
        }
        let staged = match tempfile::Builder::new()
            .prefix(".causalmed-")
            .suffix(".partial")
            .tempfile_in(destination.parent().unwrap_or(Path::new(".")))
        {
            Ok(file) => file,
            Err(_) => {
                self.finish("failed", Some("storage_error"));
                return;
            }
        };
        let job = serde_json::json!({"operation": kind, "selectionDir": source.directory, "selection": source.selection, "output": staged.path()});
        let command = worker_command(
            python,
            script,
            &source.source,
            &serde_json::json!({}),
            "",
            &job,
        );
        let id = source.selection["id"].as_str().unwrap().to_string();
        self.run_command_from(
            command,
            RunOutput::Export {
                destination: destination.to_path_buf(),
                staged,
                selection_id: id,
                kind: kind.into(),
            },
        );
    }
    #[cfg(test)]
    fn run_command(&self, command: Command) {
        self.run_command_from(
            command,
            RunOutput::Analysis {
                source: None,
                artifacts: None,
            },
        );
    }
    fn run_command_from(&self, mut command: Command, output: RunOutput) {
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
                                        "expression",
                                        "selection",
                                        "writing",
                                        "export_verify",
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
                        Some("result" | "export_result") if report.is_none() => {
                            report = Some(value["report"].clone())
                        }
                        Some("error") => {
                            failure = Some(
                                match value["code"].as_str() {
                                    Some("python_dependencies_missing") => {
                                        "python_dependencies_missing"
                                    }
                                    Some("invalid_h5ad") => "invalid_h5ad",
                                    Some(
                                        code @ ("invalid_qc_options"
                                        | "invalid_donor_column"
                                        | "invalid_gene_column"
                                        | "invalid_matrix"
                                        | "file_changed"
                                        | "selection_changed"
                                        | "source_conflict"
                                        | "empty_selection"
                                        | "metadata_collision"
                                        | "invalid_export"
                                        | "export_verification_failed"),
                                    ) => code,
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
                if exited != Some(true) || report.is_none() {
                    finish(&mut inner, "failed", Some("worker_protocol_error"));
                } else {
                    let report = report.take().unwrap();
                    match output {
                        RunOutput::Relink { source, sha256 } => {
                            if report["sha256"] != sha256 {
                                finish(&mut inner, "failed", Some("worker_protocol_error"));
                            } else {
                                let path = self.folder().join("source-bindings.json");
                                let mut bindings: Value = std::fs::read(&path)
                                    .ok()
                                    .and_then(|b| serde_json::from_slice(&b).ok())
                                    .unwrap_or(serde_json::json!({}));
                                bindings[&sha256] = source.to_string_lossy().to_string().into();
                                let saved = (|| -> Result<(), std::io::Error> {
                                    use std::io::Write;
                                    let mut tmp = NamedTempFile::new_in(self.folder())?;
                                    tmp.write_all(bindings.to_string().as_bytes())?;
                                    tmp.as_file().sync_all()?;
                                    tmp.persist(path).map_err(|e| e.error)?;
                                    Ok(())
                                })();
                                if saved.is_err() {
                                    finish(&mut inner, "failed", Some("storage_error"));
                                } else {
                                    if inner
                                        .snapshot
                                        .report
                                        .as_ref()
                                        .is_some_and(|r| r["sha256"] == sha256)
                                    {
                                        inner.source_path = Some(source);
                                        inner.snapshot.can_analyze = true;
                                    }
                                    inner.snapshot.percent = 100;
                                    finish(&mut inner, "completed", None);
                                }
                            }
                        }

                        RunOutput::Compare { left_id, right_id } => {
                            let valid = report["leftId"] == left_id
                                && report["rightId"] == right_id
                                && report["comparable"].as_bool().is_some()
                                && (report["comparable"] == false
                                    || ["bothRetained", "bothExcluded", "added", "removed"]
                                        .iter()
                                        .all(|k| report[k].as_u64().is_some()));
                            if valid {
                                inner.snapshot.comparison = Some(report);
                                inner.snapshot.percent = 100;
                                finish(&mut inner, "completed", None);
                            } else {
                                finish(&mut inner, "failed", Some("worker_protocol_error"));
                            }
                        }
                        RunOutput::Analysis { source, artifacts } => {
                            if !valid_report(&report) {
                                finish(&mut inner, "failed", Some("worker_protocol_error"));
                            } else {
                                let promoted = if report["selection"].is_object() {
                                    artifacts.as_ref().and_then(|directory| {
                                        self.promote_selection(&report, directory).ok()
                                    })
                                } else {
                                    None
                                };
                                if report["selection"].is_object() && promoted.is_none() {
                                    finish(&mut inner, "failed", Some("storage_error"));
                                } else {
                                    let temporary = self.destination.with_extension("tmp");
                                    let stored =
                                        serde_json::json!({"report": report, "sourcePath": source});
                                    let archive_saved = promoted.as_ref().map_or(Ok(()), |dir| {
                                        std::fs::write(dir.join("report.json"), stored.to_string())
                                    });
                                    let saved = archive_saved
                                        .ok()
                                        .and_then(|_| serde_json::to_vec(&stored).ok())
                                        .and_then(|b| std::fs::write(&temporary, b).ok())
                                        .and_then(|_| {
                                            std::fs::rename(&temporary, &self.destination).ok()
                                        });
                                    if saved.is_some() {
                                        crate::search_index::invalidate(self.folder());
                                        inner.snapshot.report = Some(report);
                                        inner.snapshot.can_analyze = source.is_some();
                                        inner.source_path = source;
                                        inner.snapshot.last_export = None;
                                        inner.snapshot.percent = 100;
                                        finish(&mut inner, "completed", None);
                                    } else {
                                        if let Some(path) = promoted {
                                            let _ = std::fs::remove_dir_all(path);
                                        }
                                        finish(&mut inner, "failed", Some("storage_error"));
                                    }
                                }
                            }
                        }
                        RunOutput::Export {
                            destination,
                            staged,
                            selection_id,
                            kind,
                        } => {
                            if !valid_export(&report, &selection_id, &kind)
                                || staged.as_file().metadata().map(|m| m.len()).ok()
                                    != report["bytes"].as_u64()
                            {
                                finish(&mut inner, "failed", Some("worker_protocol_error"));
                            } else {
                                let _ = staged.as_file().sync_all();
                                match staged.persist_noclobber(&destination) {
                                    Ok(_) => {
                                        let mut receipt = report;
                                        receipt["fileName"] = destination
                                            .file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                            .to_string()
                                            .into();
                                        inner.snapshot.last_export = Some(receipt);
                                        inner.snapshot.percent = 100;
                                        finish(&mut inner, "completed", None);
                                    }
                                    Err(error) => {
                                        finish(
                                            &mut inner,
                                            "failed",
                                            Some(
                                                if error.error.kind()
                                                    == std::io::ErrorKind::AlreadyExists
                                                {
                                                    "destination_exists"
                                                } else {
                                                    "storage_error"
                                                },
                                            ),
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                break;
            }
            if disconnected && exited.is_none() {
                drop(inner);
                std::thread::sleep(Duration::from_millis(25));
            }
        }
        drop(rx); // Unblock a writer if cancellation occurred while its queue was full.
        let _ = reader.join();
        let _ = errors.join();
    }
    fn promote_selection(&self, report: &Value, directory: &TempDir) -> Result<PathBuf, ()> {
        let selection = &report["selection"];
        let id = selection["id"]
            .as_str()
            .filter(|id| safe_id(id))
            .ok_or(())?;
        let bytes = std::fs::read(directory.path().join("manifest.json")).map_err(|_| ())?;
        if bytes.len() > MAX_OUTPUT
            || Some(causalmed_core::checksum(&bytes).as_str())
                != selection["manifestSha256"].as_str()
            || !directory.path().join("cells.npz").is_file()
        {
            return Err(());
        }
        let manifest: Value = serde_json::from_slice(&bytes).map_err(|_| ())?;
        if manifest["analysisId"] != selection["id"]
            || manifest["source"]["sha256"] != report["sha256"]
            || manifest["qc"] != report["qc"]
            || manifest["selectionSha256"] != selection["sha256"]
        {
            return Err(());
        }
        let destination = self.destination.parent().unwrap().join("qc-runs").join(id);
        if destination.exists() {
            return Err(());
        }
        // Store private source binding alongside the immutable report before promotion.
        // Caller owns the worker mutex here; source is supplied by the analysis path.
        std::fs::rename(directory.path(), &destination).map_err(|_| ())?;
        Ok(destination)
    }
    pub fn shutdown(&self) {
        self.cancel();
        let mut inner = self.inner.lock().unwrap();
        stop_child(&mut inner);
    }
}
pub(crate) fn safe_id(id: &str) -> bool {
    id.len() == 36 && id.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-')
}
fn worker_command(
    python: &Path,
    script: &str,
    path: &Path,
    options: &Value,
    sha: &str,
    job: &Value,
) -> Command {
    let mut command = Command::new(python);
    command
        .args(["-u", "-c", script])
        .arg(path)
        .arg(options.to_string())
        .arg(sha)
        .arg(job.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command
}
fn valid_export(r: &Value, id: &str, kind: &str) -> bool {
    r["selectionId"] == id
        && r["kind"] == kind
        && r["bytes"].as_u64().is_some_and(|n| n > 0)
        && r["sha256"]
            .as_str()
            .is_some_and(|s| s.len() == 64 && s.bytes().all(|c| c.is_ascii_hexdigit()))
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
pub(crate) fn valid_report(r: &Value) -> bool {
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
pub fn worker_script() -> &'static str {
    concat!(
        include_str!("../../python/causalmed_worker/qc.py"),
        "\n",
        include_str!("../../python/causalmed_worker/selection.py"),
        "\n",
        include_str!("../../python/causalmed_worker/h5ad.py")
    )
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
    #[ignore = "requires CAUSALMED_TEST_H5AD public PBMC3k file and anndata"]
    fn imports_public_h5ad_through_supervised_python_process() {
        let path = std::env::var("CAUSALMED_TEST_H5AD").expect("Set CAUSALMED_TEST_H5AD");
        let folder = tempfile::tempdir().unwrap();
        let worker = Worker::new(folder.path());
        worker.reserve(120).unwrap();
        worker.run(&python_path(), worker_script(), Path::new(&path));
        let snapshot = worker.snapshot();
        assert_eq!(snapshot.status, "completed", "{:?}", snapshot.error);
        let report = snapshot.report.unwrap();
        assert_eq!(report["cells"], 2638);
        assert_eq!(report["genes"], 1838);
        assert_eq!(report["groups"]["louvain"]["distinct"], 8);
        assert_eq!(
            Worker::new(folder.path()).snapshot().report.unwrap()["sha256"],
            report["sha256"]
        );
    }
    #[test]
    #[ignore = "requires CAUSALMED_TEST_DONOR_H5AD and anndata"]
    fn donor_mapping_qc_and_source_reference_survive_restart() {
        let path = std::env::var("CAUSALMED_TEST_DONOR_H5AD").unwrap();
        let folder = tempfile::tempdir().unwrap();
        let worker = Worker::new(folder.path());
        worker.reserve(120).unwrap();
        worker.run(&python_path(), worker_script(), Path::new(&path));
        let report = worker.snapshot().report.unwrap();
        let sha = report["sha256"].as_str().unwrap();
        let restored = Worker::new(folder.path());
        assert!(restored.snapshot().can_analyze);
        let path = restored.analysis_source(sha).unwrap();
        restored.reserve(120).unwrap();
        restored.run_analysis(
            &python_path(),
            worker_script(),
            &path,
            &serde_json::json!({"donorColumn":"replicate","minGenes":600}),
            sha,
        );
        let result = restored.snapshot();
        assert_eq!(result.status, "completed", "{:?}", result.error);
        let result = result.report.unwrap();
        assert_eq!(result["qc"]["donor"]["column"], "replicate");
        assert_eq!(result["qc"]["donor"]["distinct"], 8);
        assert!(
            result["qc"]["retained"].as_u64().unwrap() < report["qc"]["retained"].as_u64().unwrap()
        );
        assert!(restored.analysis_source("wrong-sha").is_err());
        assert_eq!(
            Worker::new(folder.path()).snapshot().report.unwrap()["qc"]["options"]["minGenes"],
            600
        );
    }
    #[test]
    fn failed_or_timed_out_exports_never_publish_and_clean_staging() {
        let folder = tempfile::tempdir().unwrap();
        let worker = Worker::new(folder.path());
        worker.reserve(10).unwrap();
        worker.run_command(command(&success()));
        let previous = std::fs::read(&worker.destination).unwrap();
        for (script, status, timeout) in [
            ("raise RuntimeError('failure')", "failed", 10),
            ("import time; time.sleep(60)", "timed_out", 1),
        ] {
            worker.reserve(timeout).unwrap();
            let staged = tempfile::NamedTempFile::new_in(folder.path()).unwrap();
            std::fs::write(staged.path(), b"partial content").unwrap();
            let temporary_path = staged.path().to_path_buf();
            let destination = folder.path().join("output.h5ad");
            worker.run_command_from(
                command(script),
                RunOutput::Export {
                    destination: destination.clone(),
                    staged,
                    selection_id: "id".into(),
                    kind: "export_h5ad".into(),
                },
            );
            assert_eq!(worker.snapshot().status, status);
            assert!(!destination.exists());
            assert!(!temporary_path.exists());
            assert_eq!(std::fs::read(&worker.destination).unwrap(), previous);
        }
    }
    #[test]
    fn late_existing_destination_is_not_overwritten() {
        let folder = tempfile::tempdir().unwrap();
        let worker = Worker::new(folder.path());
        worker.reserve(10).unwrap();
        let destination = folder.path().join("output.h5ad");
        let staged = tempfile::NamedTempFile::new_in(folder.path()).unwrap();
        std::fs::write(staged.path(), b"new").unwrap();
        std::fs::write(&destination, b"original").unwrap();
        let receipt = serde_json::json!({"kind":"export_h5ad","selectionId":"id","bytes":3,"sha256":"a".repeat(64)});
        let code = format!(
            "print({:?})",
            serde_json::json!({"type":"export_result","report":receipt}).to_string()
        );
        worker.run_command_from(
            command(&code),
            RunOutput::Export {
                destination: destination.clone(),
                staged,
                selection_id: "id".into(),
                kind: "export_h5ad".into(),
            },
        );
        assert_eq!(
            worker.snapshot().error.as_deref(),
            Some("destination_exists")
        );
        assert_eq!(std::fs::read(destination).unwrap(), b"original");
    }
    #[test]
    fn cancellation_cleans_staged_export_without_publishing() {
        let folder = tempfile::tempdir().unwrap();
        let worker = Worker::new(folder.path());
        worker.reserve(10).unwrap();
        let staged = tempfile::NamedTempFile::new_in(folder.path()).unwrap();
        let stage = staged.path().to_path_buf();
        let destination = folder.path().join("cancelled.zip");
        std::thread::scope(|scope| {
            scope.spawn(|| worker.run_command_from(command("import json,time; print(json.dumps({'type':'progress','percent':40,'phase':'writing'}),flush=True); time.sleep(60)"), RunOutput::Export { destination: destination.clone(), staged, selection_id: "id".into(), kind: "export_qc".into() }));
            let start = Instant::now();
            while worker.snapshot().percent < 40 {
                assert!(start.elapsed() < Duration::from_secs(5));
                std::thread::sleep(Duration::from_millis(10));
            }
            worker.cancel();
        });
        assert_eq!(worker.snapshot().status, "cancelled");
        assert!(!stage.exists());
        assert!(!destination.exists());
    }
    #[test]
    #[ignore = "requires CAUSALMED_TEST_DONOR_H5AD and anndata"]
    fn public_selection_archives_and_exports_survive_restart() {
        let path = std::env::var("CAUSALMED_TEST_DONOR_H5AD").unwrap();
        let folder = tempfile::tempdir().unwrap();
        let worker = Worker::new(folder.path());
        worker.reserve(120).unwrap();
        worker.run_analysis(
            &python_path(),
            worker_script(),
            Path::new(&path),
            &serde_json::json!({"donorColumn":"replicate"}),
            "",
        );
        assert_eq!(
            worker.snapshot().status,
            "completed",
            "{:?}",
            worker.snapshot().error
        );
        let report = worker.snapshot().report.unwrap();
        let id = report["selection"]["id"].as_str().unwrap();
        assert!(folder
            .path()
            .join("qc-runs")
            .join(id)
            .join("cells.npz")
            .exists());
        let restored = Worker::new(folder.path());
        assert_eq!(
            restored.snapshot().report.as_ref().unwrap()["selection"]["id"],
            id
        );
        assert!(restored.export_source("wrong-id").is_err());
        for (kind, file) in [("export_qc", "qc.zip"), ("export_h5ad", "retained.h5ad")] {
            let source = restored.export_source(id).unwrap();
            restored.reserve(120).unwrap();
            let destination = folder.path().join(file);
            restored.run_export(&python_path(), worker_script(), source, &destination, kind);
            let snapshot = restored.snapshot();
            assert_eq!(snapshot.status, "completed", "{:?}", snapshot.error);
            assert_eq!(snapshot.last_export.unwrap()["retained"], 24562);
            assert!(destination.is_file());
            assert_eq!(snapshot.report.unwrap()["selection"]["id"], id);
        }
        assert_eq!(
            Worker::new(folder.path()).snapshot().report.unwrap()["qc"]["options"]["donorColumn"],
            "replicate"
        );
        restored.reserve(120).unwrap();
        restored.run_analysis(
            &python_path(),
            worker_script(),
            Path::new(&path),
            &serde_json::json!({"donorColumn":"replicate","minGenes":300}),
            report["sha256"].as_str().unwrap(),
        );
        assert_eq!(restored.snapshot().status, "completed");
        let second = restored.snapshot().report.unwrap()["selection"]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_ne!(id, second);
        assert_eq!(restored.history().len(), 2);
        let a = restored.export_source(id).unwrap();
        let b = restored.export_source(&second).unwrap();
        restored.reserve(120).unwrap();
        restored.compare_history(&python_path(), worker_script(), a, b);
        let c = restored.snapshot().comparison.unwrap();
        assert_eq!(c["comparable"], true);
        assert_eq!(c["added"], 0);
        assert!(c["removed"].as_u64().unwrap() > 0);
        assert_eq!(
            c["bothRetained"].as_u64().unwrap() + c["removed"].as_u64().unwrap(),
            24562
        );
        restored.select_history(id).unwrap();
        assert_eq!(
            Worker::new(folder.path()).snapshot().report.unwrap()["selection"]["id"],
            id
        );
        restored.reserve(120).unwrap();
        assert!(restored.select_history(&second).is_err());
        restored.finish("cancelled", None);
        let source = restored.export_source(id).unwrap();
        restored.reserve(120).unwrap();
        restored.run_export(
            &python_path(),
            worker_script(),
            source,
            &folder.path().join("historical.h5ad"),
            "export_h5ad",
        );
        assert_eq!(restored.snapshot().last_export.unwrap()["retained"], 24562);
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
