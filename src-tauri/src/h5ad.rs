mod process;

use self::process::{worker_command, MAX_OUTPUT};
use crate::resources;
use serde::Serialize;
use serde_json::Value;
use std::{
    io,
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::{mpsc, Mutex},
    time::{Duration, Instant},
};

use tempfile::{NamedTempFile, TempDir};

enum RunOutput {
    Outcome {
        context: causalmed_core::evaluation::EvaluationContext,
    },
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

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: u64,
    pub status: String,
    pub percent: u8,
    pub phase: String,
    pub elapsed_ms: u64,
    pub timeout_seconds: u64,
    pub resources: resources::Usage,
    pub execution_limits: Option<resources::Limits>,
    pub resource_settings: resources::Settings,
    pub resource_settings_invalid: bool,
    pub resource_monitor_supported: bool,
    pub runtime: crate::runtime::Info,
    pub error: Option<String>,
    pub report: Option<Value>,
    pub can_analyze: bool,
    pub operation: String,
    pub last_export: Option<Value>,
    pub comparison: Option<Value>,
    pub outcome_report: Option<Value>,
    pub source_linked: bool,
    pub project_key: String,
    pub source_name: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    status: String,
    percent: u8,
    phase: String,
    elapsed_ms: u64,
    operation: String,
    resources: resources::Usage,
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
        let (resource_settings, resource_settings_invalid) = resources::load(folder);
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
                    resources: resources::Usage::default(),
                    execution_limits: None,
                    resource_settings,
                    resource_settings_invalid,
                    resource_monitor_supported: resources::supported(),
                    runtime: crate::runtime::current().info,
                    error: None,
                    report,
                    can_analyze: source_path.is_some(),
                    operation: "analysis".into(),
                    last_export: None,
                    comparison: None,
                    outcome_report: None,
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
        self.run_worker_command(
            worker_command(python, script, source, &serde_json::json!({}), "", &job),
            RunOutput::Relink {
                source: source.into(),
                sha256: sha256.into(),
            },
        );
    }
    pub fn progress(&self, project_key: &str, id: u64) -> Option<Progress> {
        let inner = self.inner.lock().unwrap();
        let state = &inner.snapshot;
        if state.project_key != project_key || state.id != id || !active(&state.status) {
            return None;
        }
        Some(Progress {
            status: state.status.clone(),
            percent: state.percent,
            phase: state.phase.clone(),
            elapsed_ms: inner.started.map_or(0, |s| s.elapsed().as_millis() as u64),
            operation: state.operation.clone(),
            resources: state.resources.clone(),
        })
    }

    pub(crate) fn require_idle(&self, project: Option<&str>) -> Result<(), String> {
        let inner = self.inner.lock().map_err(|_| "state_unavailable")?;
        let state = &inner.snapshot;
        if project.is_some_and(|key| key != state.project_key) {
            return Err("project_changed".into());
        }
        if active(&state.status) {
            return Err("worker_busy".into());
        }
        Ok(())
    }

    pub(crate) fn require_project(&self, key: &str) -> Result<(), String> {
        let inner = self.inner.lock().map_err(|_| "state_unavailable")?;
        if inner.snapshot.project_key != key {
            return Err("project_changed".into());
        }
        Ok(())
    }

    pub(crate) fn project_key(&self) -> Result<String, String> {
        self.inner
            .lock()
            .map(|inner| inner.snapshot.project_key.clone())
            .map_err(|_| "state_unavailable".into())
    }

    pub fn snapshot(&self) -> Snapshot {
        let inner = self.inner.lock().unwrap();
        let mut snapshot = inner.snapshot.clone();
        snapshot.source_linked = inner.source_path.as_ref().is_some_and(|p| p.is_file());
        snapshot.can_analyze = snapshot.source_linked;
        snapshot.runtime = crate::runtime::current().info;
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
    pub fn set_limits(
        &self,
        limits: resources::Limits,
        expected_revision: u64,
    ) -> Result<(), String> {
        limits.validate()?;
        let mut inner = self.inner.lock().unwrap();
        if active(&inner.snapshot.status) {
            return Err("worker_busy".into());
        }
        if inner.snapshot.resource_settings.revision != expected_revision {
            return Err("resource_settings_changed".into());
        }
        let settings = resources::Settings {
            version: 1,
            revision: expected_revision
                .checked_add(1)
                .ok_or("resource_settings_changed")?,
            limits,
        };
        resources::save(self.folder(), &settings)?;
        inner.snapshot.resource_settings = settings;
        inner.snapshot.resource_settings_invalid = false;
        Ok(())
    }
    pub fn reserve_outcome(&self, timeout_seconds: u64) -> Result<(), String> {
        self.reserve(timeout_seconds)?;
        self.inner.lock().unwrap().snapshot.operation = "inspect_outcome".into();
        Ok(())
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
        inner.snapshot.resources = resources::Usage::default();
        inner.snapshot.execution_limits = Some(inner.snapshot.resource_settings.limits.clone());
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
        self.run_worker_command(
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
        self.run_worker_command(
            worker_command(python, script, &a.source, &serde_json::json!({}), "", &job),
            RunOutput::Compare {
                left_id: a.selection["id"].as_str().unwrap().into(),
                right_id: b.selection["id"].as_str().unwrap().into(),
            },
        );
    }

    pub fn run_outcome(
        &self,
        context: causalmed_core::evaluation::EvaluationContext,
        inputs: Value,
    ) {
        self.inner.lock().unwrap().snapshot.operation = "inspect_outcome".into();
        let job = serde_json::json!({"operation":"inspect_outcome", "contextSha256":causalmed_core::evaluation::context_sha(&context), "context":context, "inputs":inputs});
        let command = worker_command(
            &python_path(),
            worker_script(),
            Path::new(""),
            &serde_json::json!({}),
            "",
            &job,
        );
        self.run_worker_command(command, RunOutput::Outcome { context });
    }
    pub fn run_cohort_export(
        &self,
        python: &Path,
        script: &str,
        source: ExportSource,
        destination: &Path,
        definition: Value,
    ) {
        self.run_export_inner(
            python,
            script,
            source,
            destination,
            "export_cohort",
            Some(definition),
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
        self.run_export_inner(python, script, source, destination, kind, None);
    }
    fn run_export_inner(
        &self,
        python: &Path,
        script: &str,
        source: ExportSource,
        destination: &Path,
        kind: &str,
        definition: Option<Value>,
    ) {
        {
            let mut inner = self.inner.lock().unwrap();
            inner.snapshot.operation = kind.into();
        }
        let extension = if ["export_h5ad", "export_cohort"].contains(&kind) {
            "h5ad"
        } else {
            "zip"
        };
        if !["export_h5ad", "export_qc", "export_cohort"].contains(&kind)
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
        let job = serde_json::json!({"operation": kind, "selectionDir": source.directory, "selection": source.selection, "output": staged.path(), "definition":definition});
        let command = worker_command(
            python,
            script,
            &source.source,
            &serde_json::json!({}),
            "",
            &job,
        );
        let id = source.selection["id"].as_str().unwrap().to_string();
        self.run_worker_command(
            command,
            RunOutput::Export {
                destination: destination.to_path_buf(),
                staged,
                selection_id: id,
                kind: kind.into(),
            },
        );
    }
    fn run_worker_command(&self, command: io::Result<Command>, output: RunOutput) {
        match command {
            Ok(command) => self.run_command_from(command, output),
            Err(_) => self.finish("failed", Some("storage_error")),
        }
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
            if let Err(code) =
                resources::configure(&mut command, &inner.snapshot.resource_settings.limits)
            {
                finish(&mut inner, "failed", Some(&code));
                return;
            }
            if command.get_program() == std::ffi::OsStr::new(crate::runtime::UNAVAILABLE) {
                let code = crate::runtime::current()
                    .info
                    .error
                    .unwrap_or_else(|| "bundled_runtime_missing".into());
                finish(&mut inner, "failed", Some(&code));
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
                Err(error) => {
                    let code = if matches!(error.raw_os_error(), Some(libc::EPERM | libc::EINVAL)) {
                        "resource_setup_failed"
                    } else {
                        "python_unavailable"
                    };
                    finish(&mut inner, "failed", Some(code));
                    return;
                }
            }
        };
        let (rx, reader, errors) = process::read_worker_pipes(stdout, stderr);
        let mut report = None;
        let mut failure = None;
        let mut exited = None;
        let mut last_sample = Instant::now() - Duration::from_secs(1);
        let mut monitor_failures = 0;
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
            if exited.is_none() && last_sample.elapsed() >= Duration::from_millis(50) {
                last_sample = Instant::now();
                let child = inner.child.as_mut().unwrap();
                match child.try_wait() {
                    Ok(Some(status)) => {
                        #[cfg(unix)]
                        {
                            use std::os::unix::process::ExitStatusExt;
                            if status.signal() == Some(libc::SIGXCPU) {
                                failure = Some("worker_cpu_limit".into());
                            }
                        }
                        #[cfg(windows)]
                        match resources::sample_child(child) {
                            Ok(sample) => inner.snapshot.resources.observe(sample),
                            Err(_) => failure = Some("resource_monitor_failed".into()),
                        }
                        exited = Some(status.success());
                    }
                    Ok(None) => match resources::sample_child(child) {
                        Ok(sample) => {
                            monitor_failures = 0;
                            inner.snapshot.resources.observe(sample);
                        }
                        Err(_) => monitor_failures += 1,
                    },
                    Err(_) => failure = Some("worker_failed".into()),
                }
                if monitor_failures >= 3 {
                    failure = Some("resource_monitor_failed".into());
                }
                if let Some(code) = inner
                    .snapshot
                    .resources
                    .exceeded(&inner.snapshot.resource_settings.limits)
                {
                    failure = Some(code.into());
                }
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
                                        | "export_verification_failed"
                                        | "unsupported_export_encoding"
                                        | "vlen_budget_exceeded"
                                        | "worker_memory_limit"
                                        | "resource_setup_failed"),
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
                    Ok(Some(status)) => {
                        #[cfg(unix)]
                        {
                            use std::os::unix::process::ExitStatusExt;
                            if status.signal() == Some(libc::SIGXCPU) {
                                failure = Some("worker_cpu_limit".into());
                            }
                        }
                        #[cfg(windows)]
                        match resources::sample_child(child) {
                            Ok(sample) => inner.snapshot.resources.observe(sample),
                            Err(_) => failure = Some("resource_monitor_failed".into()),
                        }
                        exited = Some(status.success());
                    }
                    Ok(None) => {}
                    Err(_) => {
                        stop_child(&mut inner);
                        finish(&mut inner, "failed", Some("worker_failed"));
                        break;
                    }
                }
            }
            if let Some(code) = failure.as_deref() {
                stop_child(&mut inner);
                finish(&mut inner, "failed", Some(code));
                break;
            }
            // EOF ensures the final result has been consumed, even after process exit.
            if disconnected && exited.is_some() {
                inner.child.take();
                if exited != Some(true) || report.is_none() {
                    finish(&mut inner, "failed", Some("worker_protocol_error"));
                } else {
                    let report = report.take().unwrap();
                    match output {
                        RunOutput::Outcome { context } => {
                            let valid = serde_json::from_value::<
                                causalmed_core::evaluation::OutcomeResult,
                            >(report.clone())
                            .ok()
                            .is_some_and(|r| {
                                causalmed_core::evaluation::validate_outcome(&context, &r).is_ok()
                            });
                            if valid {
                                inner.snapshot.outcome_report = Some(report);
                                inner.snapshot.percent = 100;
                                finish(&mut inner, "completed", None);
                            } else {
                                finish(&mut inner, "failed", Some("worker_protocol_error"));
                            }
                        }
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
        include_str!("../../python/causalmed_worker/scratch.py"),
        "\n",
        include_str!("../../python/causalmed_worker/vlen_io.py"),
        "\n",
        include_str!("../../python/causalmed_worker/backed.py"),
        "\n",
        include_str!("../../python/causalmed_worker/qc.py"),
        "\n",
        include_str!("../../python/causalmed_worker/stream_export.py"),
        "\n",
        include_str!("../../python/causalmed_worker/selection.py"),
        "\n",
        include_str!("../../python/causalmed_worker/outcome.py"),
        "\n",
        include_str!("../../python/causalmed_worker/h5ad.py")
    )
}
pub fn python_path() -> PathBuf {
    crate::runtime::current().python
}

#[cfg(test)]
mod tests;
