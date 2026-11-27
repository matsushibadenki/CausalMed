use super::*;
#[test]
fn package_roundtrip_and_corruption_leave_active_project_untouched() {
    let root = tempfile::tempdir().unwrap();
    let worker = Worker::new(root.path());
    let mut store = ResearchStore::in_memory().unwrap();
    let doc = store.snapshot();
    let stage = stage_save(root.path(), &doc, &worker).unwrap();
    let package = root.path().join("saved.causalmed");
    save(stage.path(), &package).unwrap();
    assert!(save(stage.path(), &package).is_err());
    let extracted = unpack(&package, root.path()).unwrap();
    assert_eq!(
        validate(extracted.path()).unwrap().edges.len(),
        doc.edges.len()
    );
    let changed = root.path().join("corrupt.causalmed");
    let mut bytes = fs::read(&package).unwrap();
    let end = bytes.len() - 1;
    bytes[end] ^= 1;
    fs::write(&changed, &bytes).unwrap();
    assert!(unpack(&changed, root.path()).is_err());
    assert!(!root.path().join("active-project.json").exists());
    bytes = fs::read(&package).unwrap();
    bytes.push(0);
    fs::write(&changed, &bytes).unwrap();
    assert!(unpack(&changed, root.path()).is_err());
    let mut invalid = doc.clone();
    invalid.workspace.selected_edge_id = Some("missing".into());
    assert!(store.restore_document(invalid, 100).is_err());
    assert_eq!(store.snapshot().revision, doc.revision);
    let recovered = store.restore_document(doc, 100).unwrap();
    assert_eq!(recovered.revision, 101);
}
#[test]
fn archive_names_sizes_duplicates_and_versions_are_bounded() {
    let entry = |name: &str| Entry {
        name: name.into(),
        bytes: 10,
        sha256: "a".repeat(64),
    };
    for names in [
        vec!["document.json", "../escape"],
        vec!["document.json", "document.json"],
        vec!["document.json", "/selected.json"],
        vec!["document.json", "qc-runs/../../cells.npz"],
    ] {
        assert!(check_index(&Index {
            schema_version: 1,
            entries: names.iter().map(|n| entry(n)).collect()
        })
        .is_err());
    }
    assert!(check_index(&Index {
        schema_version: 2,
        entries: vec![entry("document.json"), entry("selected.json")]
    })
    .is_err());
    let mut big = entry("selected.json");
    big.bytes = MAX_META + 1;
    assert!(check_index(&Index {
        schema_version: 1,
        entries: vec![entry("document.json"), big]
    })
    .is_err());
}
#[test]
fn legacy_packages_remain_readable_and_state_references_are_validated() {
    let root = tempfile::tempdir().unwrap();
    let worker = Worker::new(root.path());
    let doc = ResearchStore::in_memory().unwrap().snapshot();
    let id = crate::search_index::tests::fixture(root.path(), 0);
    let stage = stage_save(root.path(), &doc, &worker).unwrap();
    let names = vec![
        "document.json".to_string(),
        "selected.json".into(),
        format!("qc-runs/{id}/report.json"),
        format!("qc-runs/{id}/manifest.json"),
        format!("qc-runs/{id}/cells.npz"),
    ];
    let index = Index {
        schema_version: 1,
        entries: names
            .iter()
            .map(|name| Entry {
                name: name.clone(),
                bytes: fs::metadata(stage.path().join(name)).unwrap().len(),
                sha256: digest(&stage.path().join(name)).unwrap(),
            })
            .collect(),
    };
    let bytes = serde_json::to_vec(&index).unwrap();
    let path = root.path().join("legacy.causalmed");
    let mut out = File::create(&path).unwrap();
    out.write_all(MAGIC).unwrap();
    out.write_all(&(bytes.len() as u64).to_le_bytes()).unwrap();
    out.write_all(&bytes).unwrap();
    for name in names {
        out.write_all(&fs::read(stage.path().join(name)).unwrap())
            .unwrap();
    }
    drop(out);
    let old = unpack(&path, root.path()).unwrap();
    assert_eq!(Worker::new(old.path()).history().len(), 1);
    assert_eq!(validate(old.path()).unwrap().project_name, doc.project_name);
    let bad = serde_json::json!({"schemaVersion":1,"histories":[{"id":"00000000-0000-4000-8000-00000000ffff","trashedAt":1}]});
    fs::write(stage.path().join("history-state.json"), bad.to_string()).unwrap();
    assert!(validate(stage.path()).is_err());
    let bad_package = root.path().join("bad-state.causalmed");
    save(stage.path(), &bad_package).unwrap();
    assert!(unpack(&bad_package, root.path()).is_err());
    fs::write(
        stage.path().join("history-state.json"),
        serde_json::json!({"schemaVersion":9,"histories":[]}).to_string(),
    )
    .unwrap();
    assert!(validate(stage.path()).is_err());
    assert!(!root.path().join("active-project.json").exists());
}
#[test]
fn ten_thousand_mixed_histories_package_scope_and_resave() {
    use crate::{
        browse::{self, Query},
        search_index, trash,
    };
    let root = tempfile::tempdir().unwrap();
    let a = create(root.path(), "Synthetic package load A", 0).unwrap();
    for i in 0..10_000 {
        search_index::tests::fixture(a.path(), i);
    }
    let aw = Worker::new(a.path());
    let scope = aw.snapshot().project_key;
    let selected = "00000000-0000-4000-8000-00000000270f";
    aw.select_history(selected).unwrap();
    for i in 0..100 {
        trash::history(
            &aw,
            &scope,
            &format!("00000000-0000-4000-8000-{i:012x}"),
            false,
        )
        .unwrap();
    }
    let timestamp =
        trash::history_timestamp(a.path(), "00000000-0000-4000-8000-000000000000", &scope).unwrap();
    assert_eq!(
        browse::history(&aw, &scope, &Query::default())
            .unwrap()
            .total,
        9900
    );
    let doc = ResearchStore::inspect(a.path().join("research.sqlite3")).unwrap();
    let start = std::time::Instant::now();
    let stage = stage_save(a.path(), &doc, &aw).unwrap();
    let package = root.path().join("ten-thousand.causalmed");
    save(stage.path(), &package).unwrap();
    let restored = unpack(&package, root.path()).unwrap();
    assert!(!restored.path().join("history-search.sqlite3").exists());
    assert!(!restored.path().join("history-state.json").exists());
    fs::copy(
        restored.path().join("selected.json"),
        restored.path().join("h5ad-metadata.json"),
    )
    .unwrap();
    let rw = Worker::new(restored.path());
    let new_scope = rw.snapshot().project_key;
    assert_ne!(scope, new_scope);
    assert!(browse::history(&rw, &scope, &Query::default()).is_err());
    let q = Query {
        trash: true,
        search: "REPLICATE 200".into(),
        page_size: 25,
        page: 2,
        ..Default::default()
    };
    let trash_page = browse::history(&rw, &new_scope, &q).unwrap();
    assert_eq!(trash_page.total, 50);
    assert_eq!(trash_page.items.len(), 25);
    assert_eq!(
        browse::history(&rw, &new_scope, &Query::default())
            .unwrap()
            .total,
        9900
    );
    assert_eq!(rw.snapshot().report.unwrap()["selection"]["id"], selected);
    let id = "00000000-0000-4000-8000-000000000000";
    assert_eq!(
        trash::history_timestamp(restored.path(), id, &new_scope).unwrap(),
        timestamp
    );
    assert!(trash::history(&rw, &scope, id, true).is_err());
    trash::history(&rw, &new_scope, id, true).unwrap();
    assert_eq!(
        browse::history(
            &rw,
            &new_scope,
            &Query {
                trash: true,
                ..Default::default()
            }
        )
        .unwrap()
        .total,
        99
    );
    assert_eq!(
        browse::history(
            &aw,
            &scope,
            &Query {
                trash: true,
                ..Default::default()
            }
        )
        .unwrap()
        .total,
        100
    );
    rw.select_history(id).unwrap();
    let stage_again = stage_save(restored.path(), &doc, &rw).unwrap();
    let package_again = root.path().join("second-generation.causalmed");
    save(stage_again.path(), &package_again).unwrap();
    let again = unpack(&package_again, root.path()).unwrap();
    fs::copy(
        again.path().join("selected.json"),
        again.path().join("h5ad-metadata.json"),
    )
    .unwrap();
    let restarted = Worker::new(again.path());
    let k = restarted.snapshot().project_key;
    assert_eq!(
        browse::history(&restarted, &k, &Query::default())
            .unwrap()
            .total,
        9901
    );
    assert_eq!(
        browse::history(
            &restarted,
            &k,
            &Query {
                trash: true,
                ..Default::default()
            }
        )
        .unwrap()
        .total,
        99
    );
    assert_eq!(restarted.snapshot().report.unwrap()["selection"]["id"], id);
    // A selected trash reference and duplicated state entries must fail validation.
    let raw: HistoryStates =
        serde_json::from_value(read_json(&stage_again.path().join("history-state.json")).unwrap())
            .unwrap();
    let mut bad = raw.histories;
    bad.iter_mut().find(|s| s.id == id).unwrap().trashed_at = Some(1);
    fs::write(
        stage_again.path().join("history-state.json"),
        serde_json::to_vec(&HistoryStates {
            schema_version: 1,
            histories: bad.clone(),
        })
        .unwrap(),
    )
    .unwrap();
    assert!(validate(stage_again.path()).is_err());
    bad[0].trashed_at = None;
    bad.push(bad[0].clone());
    fs::write(
        stage_again.path().join("history-state.json"),
        serde_json::to_vec(&HistoryStates {
            schema_version: 1,
            histories: bad,
        })
        .unwrap(),
    )
    .unwrap();
    assert!(validate(stage_again.path()).is_err());
    eprintln!(
        "PORTABLE_TRASH_LOAD runs=10000 trash=100 two_generations_ms={} package_bytes={}",
        start.elapsed().as_millis(),
        fs::metadata(package).unwrap().len()
    );
}
#[test]
#[ignore = "requires CAUSALMED_TEST_DONOR_H5AD and anndata"]
fn real_project_restore_relink_and_historical_export() {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original.h5ad");
    fs::copy(
        std::env::var("CAUSALMED_TEST_DONOR_H5AD").unwrap(),
        &original,
    )
    .unwrap();
    let worker = Worker::new(root.path());
    worker.reserve(600).unwrap();
    worker.run_analysis(
        &crate::h5ad::python_path(),
        crate::h5ad::worker_script(),
        &original,
        &serde_json::json!({"donorColumn":"replicate"}),
        "",
    );
    let first = worker.snapshot().report.unwrap();
    let id = first["selection"]["id"].as_str().unwrap();
    let scope = worker.snapshot().project_key;
    ResearchStore::open(root.path().join("research.sqlite3")).unwrap();
    let project_rows = crate::browse::projects(root.path(), &scope, &Default::default()).unwrap();
    assert_eq!(project_rows.items[0].history_count, 1);
    let indexed = crate::browse::history(&worker, &scope, &Default::default()).unwrap();
    assert_eq!(indexed.total, 1);
    assert!(root.path().join("history-search.sqlite3").exists());
    worker.reserve(600).unwrap();
    worker.run_analysis(
        &crate::h5ad::python_path(),
        crate::h5ad::worker_script(),
        &original,
        &serde_json::json!({"donorColumn":"replicate","minGenes":300}),
        first["sha256"].as_str().unwrap(),
    );
    assert_eq!(worker.history().len(), 2);
    let indexed = crate::browse::history(
        &worker,
        &scope,
        &crate::browse::Query {
            search: "replicate 300".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(indexed.total, 1);
    assert_eq!(indexed.unfiltered_total, 2);
    assert_eq!(indexed.items[0].retained, 24297);
    let project_rows = crate::browse::projects(root.path(), &scope, &Default::default()).unwrap();
    assert_eq!(project_rows.items[0].history_count, 2);
    worker.select_history(id).unwrap();
    let selected = crate::browse::history(
        &worker,
        &scope,
        &crate::browse::Query {
            filter: "selected".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(selected.items[0].id, id);
    let mut store = ResearchStore::open(root.path().join("research.sqlite3")).unwrap();
    let registered = crate::datasets::register(
        &mut store,
        &worker,
        &scope,
        first["sha256"].as_str().unwrap(),
    )
    .unwrap();
    let record = &registered.datasets[0];
    assert!(record
        .observation
        .annotation_schema
        .iter()
        .any(|c| c.name == "replicate" && c.kind != "unknown"));
    let mut metadata = record.metadata.clone();
    metadata.donor_column = "replicate".into();
    metadata.notes = "Public Kang source; provenance/license not yet reviewed".into();
    let doc = store
        .update_dataset(
            first["sha256"].as_str().unwrap(),
            metadata.clone(),
            registered.revision,
        )
        .unwrap();
    let counts = crate::search_index::dataset_counts(
        &worker,
        &[first["sha256"].as_str().unwrap().into(), "0".repeat(64)],
    )
    .unwrap();
    assert_eq!(counts[first["sha256"].as_str().unwrap()], [2, 0]);
    assert_eq!(
        crate::browse::history(
            &worker,
            &scope,
            &crate::browse::Query {
                source_sha256: "0".repeat(64),
                ..Default::default()
            }
        )
        .unwrap()
        .total,
        0
    );

    let initial_output = root.path().join("before-restore.h5ad");
    let initial_source = worker.export_source(id).unwrap();
    worker.reserve(600).unwrap();
    worker.run_export(
        &crate::h5ad::python_path(),
        crate::h5ad::worker_script(),
        initial_source,
        &initial_output,
        "export_h5ad",
    );
    assert_eq!(
        worker.snapshot().status,
        "completed",
        "{:?}",
        worker.snapshot().error
    );
    assert_eq!(worker.snapshot().last_export.unwrap()["retained"], 24562);

    let stage = stage_save(root.path(), &doc, &worker).unwrap();
    let package = root.path().join("real.causalmed");
    save(stage.path(), &package).unwrap();
    let restored = unpack(&package, root.path()).unwrap();
    let restored_doc = validate(restored.path()).unwrap();
    assert_eq!(restored_doc.datasets.len(), 1);
    assert_eq!(restored_doc.datasets[0].metadata, metadata);

    fs::copy(
        restored.path().join("selected.json"),
        restored.path().join("h5ad-metadata.json"),
    )
    .unwrap();
    assert!(!restored.path().join("history-search.sqlite3").exists());
    assert!(!restored.path().join("project-search.sqlite3").exists());
    let restored_worker = Worker::new(restored.path());
    let restored_scope = restored_worker.snapshot().project_key;
    assert!(crate::browse::history(&restored_worker, &scope, &Default::default()).is_err());
    let indexed = crate::browse::history(
        &restored_worker,
        &restored_scope,
        &crate::browse::Query {
            search: "REPLICATE 300".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(indexed.total, 1);
    assert_eq!(indexed.unfiltered_total, 2);
    assert_eq!(indexed.items[0].retained, 24297);
    assert_eq!(restored_worker.history().len(), 2);
    assert!(!restored_worker.snapshot().source_linked);
    assert_eq!(
        restored_worker.snapshot().report.unwrap()["selection"]["id"],
        id
    );
    let moved = root.path().join("moved-original.h5ad");
    fs::rename(&original, &moved).unwrap();
    assert!(!original.exists());
    let wrong = root.path().join("wrong.h5ad");
    fs::write(&wrong, b"not the original").unwrap();
    let sha = restored_worker.relink_sha(id).unwrap();
    restored_worker.reserve(600).unwrap();
    restored_worker.run_relink(
        &crate::h5ad::python_path(),
        crate::h5ad::worker_script(),
        &wrong,
        &sha,
    );
    assert_eq!(
        restored_worker.snapshot().error.as_deref(),
        Some("file_changed")
    );
    assert!(!restored_worker.snapshot().source_linked);
    restored_worker.reserve(600).unwrap();
    restored_worker.run_relink(
        &crate::h5ad::python_path(),
        crate::h5ad::worker_script(),
        &moved,
        &sha,
    );
    assert!(restored_worker.snapshot().source_linked);
    restored_worker.reserve(600).unwrap();
    restored_worker.run_relink(
        &crate::h5ad::python_path(),
        crate::h5ad::worker_script(),
        &wrong,
        &sha,
    );
    assert_eq!(
        restored_worker.snapshot().error.as_deref(),
        Some("file_changed")
    );
    assert!(restored_worker.snapshot().source_linked);
    assert_eq!(restored_worker.export_source(id).unwrap().source, moved);
    let restarted = Worker::new(restored.path());
    assert!(restarted.snapshot().source_linked);
    for run in restarted.history() {
        assert_eq!(
            restarted
                .export_source(run["selection"]["id"].as_str().unwrap())
                .unwrap()
                .source,
            moved
        );
    }
    let output = root.path().join("past.h5ad");
    let source = restarted.export_source(id).unwrap();
    restarted.reserve(600).unwrap();
    restarted.run_export(
        &crate::h5ad::python_path(),
        crate::h5ad::worker_script(),
        source,
        &output,
        "export_h5ad",
    );
    assert_eq!(restarted.snapshot().status, "completed");
    assert_eq!(restarted.snapshot().last_export.unwrap()["retained"], 24562);
    assert_eq!(digest(&moved).unwrap(), sha);
    let verify = std::process::Command::new(crate::h5ad::python_path())
            .args(["-c", r#"
import sys, anndata as ad, numpy as np, pandas as pd
source, first, restored = (ad.read_h5ad(path) for path in sys.argv[1:4])
with np.load(sys.argv[4], allow_pickle=False) as original_qc, np.load(sys.argv[5], allow_pickle=False) as restored_qc:
    assert set(original_qc.files) == set(restored_qc.files)
    for key in original_qc.files:
        np.testing.assert_array_equal(original_qc[key], restored_qc[key])
    retained = restored_qc['retained']
    assert restored.shape == (24562, 15706)
    assert (restored.X != source.X[retained]).nnz == 0
    assert (restored.X != first.X).nnz == 0
    pd.testing.assert_frame_equal(restored.obs, first.obs)
    pd.testing.assert_frame_equal(restored.var, first.var)
    assert restored.obs_names.equals(source.obs_names[retained])
    assert list(restored.obs['replicate']) == list(source.obs['replicate'].iloc[np.flatnonzero(retained)])
    assert restored.obs['replicate'].nunique() == 8
"#])
            .arg(&moved)
            .arg(&initial_output)
            .arg(&output)
            .arg(root.path().join("qc-runs").join(id).join("cells.npz"))
            .arg(restored.path().join("qc-runs").join(id).join("cells.npz"))
            .output().unwrap();
    assert!(
        verify.status.success(),
        "{}",
        String::from_utf8_lossy(&verify.stderr)
    );
    let mut restored_store = ResearchStore::open(restored.path().join("research.sqlite3")).unwrap();
    restored_store.restore_document(doc, 0).unwrap();
    activate(root.path(), restored.path()).unwrap();
    assert_eq!(active_folder(root.path()).unwrap(), restored.path());
    let active_query = crate::browse::Query {
        filter: "active".into(),
        ..Default::default()
    };
    let rows = crate::browse::projects(root.path(), &restored_scope, &active_query).unwrap();
    assert_eq!(rows.unfiltered_total, 2);
    assert_eq!(rows.total, 1);
    assert_eq!(rows.items[0].id, restored_scope);
    assert_eq!(rows.items[0].history_count, 2);
    assert_eq!(rows.items[0].selected_id.as_deref(), Some(id));
}
