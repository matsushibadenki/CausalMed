use super::*;
use std::sync::{Arc, Mutex, RwLock};
fn state(root: &Path, folder: &Path) -> crate::AppState {
    crate::AppState {
        store: Mutex::new(ResearchStore::open(folder.join("research.sqlite3")).unwrap()),
        h5ad: RwLock::new(Arc::new(Worker::new(folder))),
        project_gate: RwLock::new(()),
        root: root.into(),
        renderer: Mutex::new(None),
    }
}
#[test]
fn poisoned_worker_handle_returns_state_error() {
    let root = tempfile::tempdir().unwrap();
    let app = state(root.path(), root.path());
    std::thread::scope(|scope| {
        let panicked = scope.spawn(|| {
            let _worker = app.h5ad.write().unwrap();
            panic!("simulate failed worker handle update");
        });
        assert!(panicked.join().is_err());
    });

    assert_eq!(app.worker().err().as_deref(), Some("state_unavailable"));
}

#[test]
fn catalog_switches_legacy_and_empty_projects_without_copying_state() {
    let root = tempfile::tempdir().unwrap();
    let original = ResearchStore::open(root.path().join("research.sqlite3"))
        .unwrap()
        .snapshot();
    let folder = create(root.path(), "Empty B", original.revision).unwrap();
    let id = folder
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let _path = folder.keep();
    let app = state(root.path(), root.path());
    app.worker().unwrap().reserve(120).unwrap();
    assert_eq!(app.switch_to(&id).unwrap_err(), "worker_busy");
    app.worker().unwrap().finish("cancelled", None);
    app.switch_to(&id).unwrap();
    let doc = app.store.lock().unwrap().snapshot();
    assert_eq!(doc.project_name, "Empty B");
    assert!(doc.edges.is_empty());
    assert!(app.worker().unwrap().snapshot().report.is_none());
    assert!(!app.worker().unwrap().snapshot().source_linked);
    let rows = catalog(root.path()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows.iter().filter(|r| r.active).count(), 1);
    let revision = doc.revision;
    app.switch_to("local").unwrap();
    let doc = app.store.lock().unwrap().snapshot();
    assert!(doc.revision > revision);
    assert_eq!(doc.edges.len(), original.edges.len());
    assert_eq!(active_folder(root.path()).unwrap(), root.path());
    assert!(app.switch_to("../outside").is_err());
    assert_eq!(active_folder(root.path()).unwrap(), root.path());
    app.store
        .lock()
        .unwrap()
        .rename_project("Original A")
        .unwrap();
    assert_eq!(
        catalog(root.path())
            .unwrap()
            .iter()
            .find(|r| r.id == "local")
            .unwrap()
            .name,
        "Original A"
    );
}
#[test]
#[ignore = "requires CAUSALMED_TEST_DONOR_H5AD and anndata"]
fn same_h5ad_sha_projects_keep_histories_options_donors_and_bindings_isolated() {
    let root = tempfile::tempdir().unwrap();
    let a = create(root.path(), "Kang A", 0).unwrap();
    let b = create(root.path(), "Kang B", 0).unwrap();
    let aid = a.path().file_name().unwrap().to_str().unwrap().to_owned();
    let bid = b.path().file_name().unwrap().to_str().unwrap().to_owned();
    let af = a.keep();
    let bf = b.keep();
    let ap = af.join("a.h5ad");
    let bp = bf.join("b.h5ad");
    let public = std::env::var("CAUSALMED_TEST_DONOR_H5AD").unwrap();
    fs::copy(&public, &ap).unwrap();
    fs::copy(&public, &bp).unwrap();
    let aw = Worker::new(&af);
    aw.reserve(120).unwrap();
    aw.run_analysis(
        &crate::h5ad::python_path(),
        crate::h5ad::worker_script(),
        &ap,
        &serde_json::json!({"donorColumn":"replicate"}),
        "",
    );
    let ar = aw.snapshot().report.unwrap();
    let asel = ar["selection"]["id"].as_str().unwrap();
    assert_eq!(ar["qc"]["retained"], 24562);
    let bw = Worker::new(&bf);
    bw.reserve(120).unwrap();
    bw.run_analysis(
        &crate::h5ad::python_path(),
        crate::h5ad::worker_script(),
        &bp,
        &serde_json::json!({"minGenes":300}),
        "",
    );
    let br = bw.snapshot().report.unwrap();
    let bsel = br["selection"]["id"].as_str().unwrap();
    assert_eq!(br["qc"]["retained"], 24297);
    assert_eq!(ar["sha256"], br["sha256"]);
    // Rebinding the same checksum in B must never affect A's independent source path.
    let moved = bf.join("relocated-b.h5ad");
    fs::rename(&bp, &moved).unwrap();
    bw.reserve(120).unwrap();
    bw.run_relink(
        &crate::h5ad::python_path(),
        crate::h5ad::worker_script(),
        &moved,
        br["sha256"].as_str().unwrap(),
    );
    activate(root.path(), &af).unwrap();
    let app = state(root.path(), &af);
    for (id, sel, expected_path, retained, donor, min_genes) in [
        (&bid, bsel, &moved, 24297, "", 300),
        (&aid, asel, &ap, 24562, "replicate", 200),
        (&bid, bsel, &moved, 24297, "", 300),
        (&aid, asel, &ap, 24562, "replicate", 200),
    ] {
        app.switch_to(id).unwrap();
        let worker = app.worker().unwrap();
        let snapshot = worker.snapshot();
        let report = snapshot.report.unwrap();
        assert_eq!(worker.history().len(), 1);
        assert_eq!(report["selection"]["id"], sel);
        assert_eq!(report["qc"]["retained"], retained);
        assert_eq!(report["qc"]["options"]["donorColumn"], donor);
        assert_eq!(report["qc"]["options"]["minGenes"], min_genes);
        assert_eq!(worker.export_source(sel).unwrap().source, *expected_path);
        assert!(worker
            .export_source(if sel == asel { bsel } else { asel })
            .is_err());
        let restarted = Worker::new(&active_folder(root.path()).unwrap());
        assert_eq!(restarted.snapshot().report.unwrap()["selection"]["id"], sel);
        assert_eq!(restarted.export_source(sel).unwrap().source, *expected_path);
    }
    let aq = crate::browse::history(
        &app.worker().unwrap(),
        &aid,
        &crate::browse::Query {
            search: "REPLICATE 200".into(),
            filter: "mapped".into(),
            page_size: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(aq.total, 1);
    assert_eq!(aq.items[0].id, asel);
    assert!(crate::browse::history(&app.worker().unwrap(), &bid, &Default::default()).is_err());
    app.switch_to(&bid).unwrap();
    let bq = crate::browse::history(
        &app.worker().unwrap(),
        &bid,
        &crate::browse::Query {
            search: "replicate".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(bq.total, 0);
    let bq = crate::browse::history(
        &app.worker().unwrap(),
        &bid,
        &crate::browse::Query {
            filter: "unmapped".into(),
            sort: "retained_desc".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(bq.total, 1);
    assert_eq!(bq.items[0].id, bsel);
    let pq = crate::browse::projects(
        root.path(),
        &bid,
        &crate::browse::Query {
            search: "Kang".into(),
            sort: "name".into(),
            page_size: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(pq.total, 2);
    assert_eq!(pq.pages, 2);
    assert_eq!(pq.items[0].id, aid);
    let pq = crate::browse::projects(
        root.path(),
        &bid,
        &crate::browse::Query {
            search: "Kang".into(),
            filter: "unmapped".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(pq.total, 1);
    assert_eq!(pq.items[0].id, bid);
    let rows = catalog(root.path()).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter()
            .find(|r| r.id == aid)
            .unwrap()
            .donor_column
            .as_deref(),
        Some("replicate")
    );
    assert_eq!(
        rows.iter()
            .find(|r| r.id == bid)
            .unwrap()
            .donor_column
            .as_deref(),
        Some("")
    );
    for (id, sel, expected) in [(&aid, asel, 24562), (&bid, bsel, 24297)] {
        app.switch_to(id).unwrap();
        let worker = app.worker().unwrap();
        let source = worker.export_source(sel).unwrap();
        worker.reserve(120).unwrap();
        let out = root.path().join(format!("{id}.h5ad"));
        worker.run_export(
            &crate::h5ad::python_path(),
            crate::h5ad::worker_script(),
            source,
            &out,
            "export_h5ad",
        );
        assert_eq!(worker.snapshot().status, "completed");
        assert_eq!(worker.snapshot().last_export.unwrap()["retained"], expected);
    }
}
