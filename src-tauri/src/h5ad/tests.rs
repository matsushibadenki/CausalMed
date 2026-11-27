use super::*;
use std::process::Stdio;

#[test]
fn progress_is_bounded_and_scoped_to_the_active_job() {
    let folder = tempfile::tempdir().unwrap();
    let worker = Worker::new(folder.path());
    let initial = worker.snapshot();
    assert!(worker.progress(&initial.project_key, initial.id).is_none());

    worker.reserve(120).unwrap();
    let running = worker.snapshot();
    assert!(worker.progress(&running.project_key, initial.id).is_none());
    assert!(worker.progress("other-project", running.id).is_none());
    let progress = worker.progress(&running.project_key, running.id).unwrap();
    let encoded = serde_json::to_string(&progress).unwrap();
    assert!(encoded.len() < 512);
    assert!(!encoded.contains("report"));

    worker.finish("completed", None);
    assert!(worker.progress(&running.project_key, running.id).is_none());
}
fn command(code: &str) -> Command {
    let mut command = Command::new(python_path());
    command
        .args(["-I", "-B", "-u", "-c", code])
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
            ("import json,sys; print(json.dumps({'type':'error','code':'unsupported_export_encoding'})); sys.exit(1)", "failed", 10),
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
            if script.contains("unsupported_export_encoding") {
                assert_eq!(worker.snapshot().error.as_deref(), Some("unsupported_export_encoding"));
            }
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
fn embedded_worker_source_can_exceed_the_linux_exec_argument_limit() {
    let folder = tempfile::tempdir().unwrap();
    let worker = Worker::new(folder.path());
    let source = format!("#{}\n{}", "x".repeat(128 * 1024), success());
    let command = worker_command(
        &python_path(),
        &source,
        Path::new(""),
        &serde_json::json!({}),
        "",
        &serde_json::json!({}),
    )
    .unwrap();
    worker.reserve(10).unwrap();
    worker.run_command(command);
    assert_eq!(
        worker.snapshot().status,
        "completed",
        "{:?}",
        worker.snapshot().error
    );
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
#[test]
fn outcome_timeout_cancel_and_malformed_preserve_previous_state() {
    let dir = tempfile::tempdir().unwrap();
    let w = Worker::new(dir.path());
    let ctx = causalmed_core::evaluation::EvaluationContext {
        version: 1,
        spec: causalmed_core::evaluation::OutcomeSpec {
            feature_id: "QA".into(),
            feature_column: "_index".into(),
        },
        inputs: vec![],
    };
    w.inner.lock().unwrap().snapshot.report = Some(serde_json::json!({"previous":"QC"}));
    w.inner.lock().unwrap().snapshot.outcome_report =
        Some(serde_json::json!({"previous":"outcome"}));
    w.reserve(1).unwrap();
    w.run_command_from(
        command("import time; time.sleep(60)"),
        RunOutput::Outcome {
            context: ctx.clone(),
        },
    );
    assert_eq!(w.snapshot().status, "timed_out");
    w.reserve(10).unwrap();
    std::thread::scope(|scope| {
        scope.spawn(||w.run_command_from(command("import json,time;print(json.dumps({'type':'progress','percent':10,'phase':'expression'}),flush=True);time.sleep(60)"),RunOutput::Outcome{context:ctx.clone()}));
        for _ in 0..100 {
            if w.snapshot().percent == 10 {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        w.cancel();
    });
    assert_eq!(w.snapshot().status, "cancelled");
    w.reserve(10).unwrap();
    w.run_command_from(
        command("import json;print(json.dumps({'type':'result','report':{}}))"),
        RunOutput::Outcome { context: ctx },
    );
    assert_eq!(w.snapshot().status, "failed");
    assert_eq!(
        w.snapshot().report,
        Some(serde_json::json!({"previous":"QC"}))
    );
    assert_eq!(
        w.snapshot().outcome_report,
        Some(serde_json::json!({"previous":"outcome"}))
    );
    assert!(w.inner.lock().unwrap().child.is_none());
}
#[test]
fn resource_budgets_reap_child_preserve_report_and_prevent_publication() {
    let dir = tempfile::tempdir().unwrap();
    let w = Worker::new(dir.path());
    w.inner.lock().unwrap().snapshot.report = Some(serde_json::json!({"previous":"QC"}));
    w.set_limits(
        resources::Limits {
            memory_mib: 128,
            cpu_seconds: 1,
            threads: 1,
        },
        0,
    )
    .unwrap();
    assert!(w.set_limits(resources::Limits::default(), 0).is_err());
    w.reserve(12).unwrap();
    assert!(w.set_limits(resources::Limits::default(), 1).is_err());
    w.run_command(command("while True: pass"));
    assert_eq!(w.snapshot().error.as_deref(), Some("worker_cpu_limit"));
    assert!(w.snapshot().resources.cpu_millis >= 1000);
    assert!(w.inner.lock().unwrap().child.is_none());
    let destination = dir.path().join("must-not-publish.zip");
    let staged = NamedTempFile::new_in(dir.path()).unwrap();
    let staged_path = staged.path().to_owned();
    w.reserve(12).unwrap();
    w.run_command_from(
        command("import time; x=bytearray(256*1024*1024); time.sleep(5)"),
        RunOutput::Export {
            destination: destination.clone(),
            staged,
            selection_id: "QA".into(),
            kind: "export_qc".into(),
        },
    );
    assert_eq!(w.snapshot().error.as_deref(), Some("worker_memory_limit"));
    assert!(!destination.exists());
    assert!(!staged_path.exists());
    assert!(w.inner.lock().unwrap().child.is_none());
    assert_eq!(
        w.snapshot().report,
        Some(serde_json::json!({"previous":"QC"}))
    );
    assert_eq!(
        Worker::new(dir.path())
            .snapshot()
            .resource_settings
            .limits
            .memory_mib,
        128
    );
    w.set_limits(resources::Limits::default(), 1).unwrap();
    w.reserve(12).unwrap();
    w.run_command(command(&success()));
    assert_eq!(w.snapshot().status, "completed");
}
#[test]
fn worker_thread_environment_is_applied_before_imports() {
    let dir = tempfile::tempdir().unwrap();
    let w = Worker::new(dir.path());
    w.set_limits(
        resources::Limits {
            memory_mib: 256,
            cpu_seconds: 30,
            threads: 1,
        },
        0,
    )
    .unwrap();
    w.reserve(12).unwrap();
    w.run_command(command(&format!("import os;assert all(os.environ[k]=='1' for k in ['OMP_NUM_THREADS','OPENBLAS_NUM_THREADS','MKL_NUM_THREADS','NUMEXPR_NUM_THREADS','VECLIB_MAXIMUM_THREADS','BLIS_NUM_THREADS']);{}",success())));
    assert_eq!(w.snapshot().status, "completed");
}
#[test]
#[ignore = "requires CAUSALMED_TEST_DONOR_H5AD and anndata"]
fn public_h5ad_resource_stop_retry_restore_and_isolation() {
    let root = tempfile::tempdir().unwrap();
    let a = crate::project::create(root.path(), "Budget A QA", 0).unwrap();
    let b = crate::project::create(root.path(), "Budget B QA", 0).unwrap();
    let source = PathBuf::from(std::env::var("CAUSALMED_TEST_DONOR_H5AD").unwrap());
    let original_sha = crate::project::digest(&source).unwrap();
    let w = Worker::new(a.path());
    w.set_limits(
        resources::Limits {
            memory_mib: 1024,
            cpu_seconds: 60,
            threads: 1,
        },
        0,
    )
    .unwrap();
    w.reserve(600).unwrap();
    w.run_analysis(
        &python_path(),
        worker_script(),
        &source,
        &serde_json::json!({"donorColumn":"replicate"}),
        "",
    );
    assert_eq!(w.snapshot().status, "completed", "{:?}", w.snapshot().error);
    let report = w.snapshot().report.unwrap();
    let selection = report["selection"]["id"].as_str().unwrap();
    assert_eq!(report["qc"]["retained"], 24562);
    assert!(w.snapshot().resources.samples > 0);
    assert!(w.snapshot().resources.peak_resident_bytes > 0);
    eprintln!(
        "Kang resource success: peak={} MiB cpu={} ms samples={}",
        w.snapshot().resources.peak_resident_bytes / 1048576,
        w.snapshot().resources.cpu_millis,
        w.snapshot().resources.samples
    );
    w.set_limits(
        resources::Limits {
            memory_mib: 64,
            cpu_seconds: 60,
            threads: 1,
        },
        1,
    )
    .unwrap();
    w.reserve(600).unwrap();
    w.run_analysis(
        &python_path(),
        worker_script(),
        &source,
        &serde_json::json!({"donorColumn":"replicate"}),
        &original_sha,
    );
    assert_eq!(w.snapshot().error.as_deref(), Some("worker_memory_limit"));
    assert_eq!(w.snapshot().report, Some(report.clone()));
    assert_eq!(
        std::fs::read_dir(a.path().join("qc-runs")).unwrap().count(),
        1
    );
    assert!(w.inner.lock().unwrap().child.is_none());
    let output = root.path().join("blocked.h5ad");
    let input = w.export_source(selection).unwrap();
    w.reserve(600).unwrap();
    w.run_export(
        &python_path(),
        worker_script(),
        input,
        &output,
        "export_h5ad",
    );
    assert_eq!(w.snapshot().error.as_deref(), Some("worker_memory_limit"));
    assert!(!output.exists());
    let restored = Worker::new(a.path());
    assert_eq!(restored.snapshot().resource_settings.limits.memory_mib, 64);
    assert_eq!(restored.snapshot().report, Some(report.clone()));
    assert_eq!(
        Worker::new(b.path())
            .snapshot()
            .resource_settings
            .limits
            .memory_mib,
        2048
    );
    restored
        .set_limits(
            resources::Limits {
                memory_mib: 1024,
                cpu_seconds: 60,
                threads: 1,
            },
            2,
        )
        .unwrap();
    let input = restored.export_source(selection).unwrap();
    restored.reserve(600).unwrap();
    restored.run_export(
        &python_path(),
        worker_script(),
        input,
        &output,
        "export_h5ad",
    );
    assert_eq!(
        restored.snapshot().status,
        "completed",
        "{:?}",
        restored.snapshot().error
    );
    assert!(output.is_file());
    let readback=Command::new(python_path()).args(["-I","-B","-c","import anndata as ad,sys; a=ad.read_h5ad(sys.argv[1],backed='r'); assert a.shape==(24562,15706); assert a.obs['replicate'].nunique()==8; a.file.close()"]).arg(&output).output().unwrap();
    assert!(
        readback.status.success(),
        "{}",
        String::from_utf8_lossy(&readback.stderr)
    );
    assert_eq!(restored.snapshot().report, Some(report.clone()));
    assert_eq!(crate::project::digest(&source).unwrap(), original_sha);
    // Local machine budgets are not portable research metadata.
    let doc = causalmed_core::ResearchStore::open(a.path().join("research.sqlite3"))
        .unwrap()
        .snapshot();
    let stage = crate::project::stage_save(a.path(), &doc, &restored).unwrap();
    let package = root.path().join("budgets.causalmed");
    crate::project::save(stage.path(), &package).unwrap();
    let un = crate::project::unpack(&package, root.path()).unwrap();
    crate::project::validate(un.path()).unwrap();
    assert_eq!(
        Worker::new(un.path())
            .snapshot()
            .resource_settings
            .limits
            .memory_mib,
        2048
    );
}
#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
#[ignore = "requires CAUSALMED_TEST_DONOR_H5AD and anndata; creates large HDF5 metadata"]
fn public_h5ad_native_memory_denial_preserves_committed_artifacts() {
    let root = tempfile::tempdir().unwrap();
    let digest = |file: &Path| {
        let output = Command::new(python_path())
                .args(["-I", "-B", "-c", "import hashlib,sys; print(hashlib.file_digest(open(sys.argv[1], 'rb'), 'sha256').hexdigest())"])
                .arg(file)
                .output()
                .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    let original = PathBuf::from(std::env::var("CAUSALMED_TEST_DONOR_H5AD").unwrap());
    let original_sha = digest(&original);
    for mode in ["attribute", "compatibility"] {
        let project = root.path().join(mode);
        std::fs::create_dir(&project).unwrap();
        let source = project.join("source.h5ad");
        std::fs::copy(&original, &source).unwrap();
        let fixture = Command::new(python_path())
            .args([
                "-I",
                "-B",
                "-c",
                r#"
import sys, h5py
with h5py.File(sys.argv[1], 'r+') as file:
    group = file['uns'].create_group('resource_probe')
    node = group.create_dataset('null', dtype='f8') if sys.argv[2] == 'compatibility' else group
    node.attrs.create('large', ['Z' * (16 << 20)] * 18, dtype=h5py.string_dtype())
"#,
            ])
            .arg(&source)
            .arg(mode)
            .output()
            .unwrap();
        assert!(fixture.status.success(), "{:?}", fixture.stderr);
        let sha = digest(&source);
        let worker = Worker::new(&project);
        let run_export = |worker: &Worker, id: &str, destination: &Path, kind: &str| {
            let input = worker.export_source(id).unwrap();
            worker.reserve(600).unwrap();
            worker.run_export(&python_path(), worker_script(), input, destination, kind);
        };
        worker.reserve(600).unwrap();
        worker.run_analysis(
            &python_path(),
            worker_script(),
            &source,
            &serde_json::json!({"donorColumn": "replicate"}),
            &sha,
        );
        let report = worker.snapshot().report.unwrap();
        assert_eq!(report["qc"]["retained"], 24562);
        let id = report["selection"]["id"].as_str().unwrap();
        let qc = project.join("qc-runs").join(id);
        let files = [
            qc.join("cells.npz"),
            qc.join("manifest.json"),
            project.join("saved.zip"),
        ];
        run_export(&worker, id, &files[2], "export_qc");
        assert_eq!(worker.snapshot().status, "completed");
        let digests = files
            .each_ref()
            .map(|file| crate::project::digest(file).unwrap());
        let receipt = worker.snapshot().last_export.unwrap();
        worker
            .set_limits(
                resources::Limits {
                    // The Linux ceiling includes library mappings at startup.
                    memory_mib: if cfg!(target_os = "linux") { 384 } else { 240 },
                    cpu_seconds: 60,
                    threads: 1,
                },
                0,
            )
            .unwrap();
        let blocked = project.join("blocked.h5ad");
        run_export(&worker, id, &blocked, "export_h5ad");
        let failed = worker.snapshot();
        assert_eq!(
            failed.error.as_deref(),
            Some("worker_memory_limit"),
            "{mode}"
        );
        assert!(failed.percent >= 10, "failed before HDF5 writing: {mode}");
        assert!(!blocked.exists());
        assert!(!std::fs::read_dir(&project).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".partial")
        }));
        assert_eq!(failed.last_export, Some(receipt));
        assert_eq!(
            files
                .each_ref()
                .map(|file| crate::project::digest(file).unwrap()),
            digests
        );
        assert_eq!(Worker::new(&project).snapshot().report, Some(report));
        assert_eq!(digest(&source), sha);
    }
    assert_eq!(digest(&original), original_sha);
}
#[test]
fn outcome_reservation_exposes_operation_and_cancels_before_spawn() {
    let dir = tempfile::tempdir().unwrap();
    let w = Worker::new(dir.path());
    w.reserve_outcome(10).unwrap();
    assert_eq!(w.snapshot().operation, "inspect_outcome");
    w.cancel();
    let ctx = causalmed_core::evaluation::EvaluationContext {
        version: 1,
        spec: causalmed_core::evaluation::OutcomeSpec {
            feature_id: "QA".into(),
            feature_column: "_index".into(),
        },
        inputs: vec![],
    };
    w.run_outcome(ctx, serde_json::json!([]));
    assert_eq!(w.snapshot().status, "cancelled");
    assert!(w.inner.lock().unwrap().child.is_none());
    assert!(w.snapshot().outcome_report.is_none());
}
