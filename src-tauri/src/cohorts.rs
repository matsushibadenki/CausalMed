use crate::h5ad::Worker;
use causalmed_core::{
    cohorts::{self, CohortObservation, Eligibility, ObservedDonor},
    Document, ResearchStore,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::Path,
    time::{Duration, Instant},
};
fn scope(worker: &Worker, key: &str) -> Result<(), String> {
    let state = worker.snapshot();
    if state.project_key != key {
        return Err("project_changed".into());
    }
    if ["selecting", "running", "cancelling"].contains(&state.status.as_str()) {
        return Err("worker_busy".into());
    }
    Ok(())
}
pub fn capture(
    store: &mut ResearchStore,
    worker: &Worker,
    key: &str,
    sha: &str,
    revision: u64,
) -> Result<Document, String> {
    scope(worker, key)?;
    let current = worker.snapshot().report.ok_or("dataset_missing")?;
    if current["sha256"] != sha {
        return Err("dataset_changed".into());
    }
    let id = current["selection"]["id"]
        .as_str()
        .ok_or("qc_unavailable")?;
    let stored = worker.history_record(id)?;
    let r = &stored["report"];
    let doc = store.snapshot();
    let dataset = doc
        .datasets
        .iter()
        .find(|d| d.observation.sha256 == sha)
        .ok_or("dataset_missing")?;
    store
        .capture_cohort(observe(r, dataset)?, revision)
        .map_err(|e| e.to_string())
}
pub(crate) fn observe(
    r: &serde_json::Value,
    dataset: &causalmed_core::DatasetRecord,
) -> Result<CohortObservation, String> {
    let qc = &r["qc"];
    let donor = &qc["donor"];
    if donor["inventoryComplete"] != true {
        return Err("donor_inventory_incomplete".into());
    }
    let column = donor["column"].as_str().ok_or("invalid_cohort")?;
    if column.is_empty() || column != dataset.metadata.donor_column {
        return Err("donor_role_mismatch".into());
    }
    let observation = CohortObservation {
        source_sha256: r["sha256"].as_str().ok_or("invalid_cohort")?.into(),
        qc_id: r["selection"]["id"]
            .as_str()
            .ok_or("invalid_cohort")?
            .into(),
        manifest_sha256: r["selection"]["manifestSha256"]
            .as_str()
            .ok_or("invalid_cohort")?
            .into(),
        metadata_sha256: cohorts::metadata_sha(dataset),
        donor_column: column.into(),
        condition_column: dataset.metadata.condition_column.clone(),
        perturbation_column: dataset.metadata.perturbation_column.clone(),
        retained: qc["retained"].as_u64().ok_or("invalid_cohort")?,
        missing_retained: donor["missingRetained"].as_u64().ok_or("invalid_cohort")?,
        qc_status: qc["status"].as_str().ok_or("invalid_cohort")?.into(),
        donors: donor["groups"]
            .as_array()
            .ok_or("invalid_cohort")?
            .iter()
            .map(|d| {
                Ok(ObservedDonor {
                    label: d["label"].as_str().ok_or("invalid_cohort")?.into(),
                    retained: d["retained"].as_u64().ok_or("invalid_cohort")?,
                })
            })
            .collect::<Result<Vec<_>, &str>>()?,
        condition_value: single(qc, &dataset.metadata.condition_column),
        perturbation_value: single(qc, &dataset.metadata.perturbation_column),
    };
    Ok(observation)
}
fn single(qc: &serde_json::Value, column: &str) -> Option<String> {
    let g = &qc["annotationContext"][column];
    if !column.is_empty()
        && g["complete"] == true
        && g["distinct"] == 1
        && g["missing"] == 0
        && g["other"] == 0
    {
        g["counts"][0]["label"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .map(str::to_owned)
    } else {
        None
    }
}
fn matches(o: &CohortObservation, r: &serde_json::Value) -> bool {
    let qc = &r["qc"];
    let donors: Option<Vec<ObservedDonor>> = qc["donor"]["groups"].as_array().and_then(|groups| {
        groups
            .iter()
            .map(|d| {
                Some(ObservedDonor {
                    label: d["label"].as_str()?.into(),
                    retained: d["retained"].as_u64()?,
                })
            })
            .collect()
    });
    r["sha256"] == o.source_sha256
        && r["selection"]["manifestSha256"] == o.manifest_sha256
        && qc["donor"]["inventoryComplete"] == true
        && qc["donor"]["column"] == o.donor_column
        && qc["retained"] == o.retained
        && qc["donor"]["missingRetained"] == o.missing_retained
        && qc["status"] == o.qc_status
        && donors.as_ref() == Some(&o.donors)
        && single(qc, &o.condition_column) == o.condition_value
        && single(qc, &o.perturbation_column) == o.perturbation_value
}
pub fn validate_archives(doc: &Document, worker: &Worker) -> Result<(), String> {
    for c in doc
        .cohorts
        .iter()
        .chain(doc.cohort_definitions.iter().map(|d| &d.payload.alignment))
        .chain(
            doc.evaluations
                .iter()
                .flat_map(|r| r.context.inputs.iter().map(|i| &i.cohort)),
        )
    {
        let record = worker.archived_record(&c.observation.qc_id)?;
        if !matches(&c.observation, &record["report"]) {
            return Err("invalid_project".into());
        }
    }
    Ok(())
}
fn hash_matches(path: &Path, expected: &str, deadline: Instant) -> bool {
    let Ok(mut f) = std::fs::File::open(path) else {
        return false;
    };
    let Ok(before) = f.metadata() else {
        return false;
    };
    if !before.is_file() {
        return false;
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        if Instant::now() > deadline {
            return false;
        }
        match f.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => hash.update(&buffer[..n]),
            Err(_) => return false,
        }
    }
    let Ok(after) = std::fs::metadata(path) else {
        return false;
    };
    before.len() == after.len()
        && before.modified().ok() == after.modified().ok()
        && format!("{:x}", hash.finalize()) == expected
}
pub fn assess(doc: &Document, worker: &Worker, key: &str) -> Result<Eligibility, String> {
    scope(worker, key)?;
    let mut availability = BTreeMap::new();
    let deadline = Instant::now() + Duration::from_secs(15);
    for c in &doc.cohorts {
        if c.settings.partition == "exclude" {
            continue;
        }
        let o = &c.observation;
        let live = worker.history_record(&o.qc_id).is_ok_and(|r| {
            matches(o, &r["report"])
                && hash_matches(
                    &worker
                        .folder()
                        .join("qc-runs")
                        .join(&o.qc_id)
                        .join("cells.npz"),
                    r["report"]["selection"]["sha256"].as_str().unwrap_or(""),
                    deadline,
                )
        });
        let linked = live
            && worker
                .export_source(&o.qc_id)
                .is_ok_and(|s| hash_matches(&s.source, &o.source_sha256, deadline));
        availability.insert(o.source_sha256.clone(), (live, linked));
    }
    Ok(cohorts::assess(doc, &availability))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_integrity_is_bounded_and_rechecks_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.h5ad");
        std::fs::write(&path, b"synthetic file integrity QA").unwrap();
        let sha = causalmed_core::checksum(b"synthetic file integrity QA");
        assert!(hash_matches(
            &path,
            &sha,
            Instant::now() + Duration::from_secs(1)
        ));
        assert!(!hash_matches(
            &path,
            &sha,
            Instant::now() - Duration::from_secs(1)
        ));
        std::fs::write(&path, b"changed file integrity QA").unwrap();
        assert!(!hash_matches(
            &path,
            &sha,
            Instant::now() + Duration::from_secs(1)
        ));
    }
    #[test]
    #[ignore = "requires CAUSALMED_TEST_DONOR_H5AD and anndata"]
    fn real_donor_alignment_package_restart_isolation_and_abstention() {
        use crate::{datasets, project, trash};
        let root = tempfile::tempdir().unwrap();
        let a = project::create(root.path(), "Donor A QA", 0).unwrap();
        let b = project::create(root.path(), "Donor B QA", 0).unwrap();
        let original = root.path().join("original.h5ad");
        std::fs::copy(
            std::env::var("CAUSALMED_TEST_DONOR_H5AD").unwrap(),
            &original,
        )
        .unwrap();
        let mut stores = vec![];
        let mut workers = vec![];
        for folder in [a.path(), b.path()] {
            let worker = Worker::new(folder);
            worker.reserve(120).unwrap();
            worker.run_analysis(
                &crate::h5ad::python_path(),
                crate::h5ad::worker_script(),
                &original,
                &serde_json::json!({"donorColumn":"replicate"}),
                "",
            );
            assert_eq!(worker.snapshot().status, "completed");
            let r = worker.snapshot().report.unwrap();
            let scope = worker.snapshot().project_key;
            let mut store = ResearchStore::open(folder.join("research.sqlite3")).unwrap();
            let doc =
                datasets::register(&mut store, &worker, &scope, r["sha256"].as_str().unwrap())
                    .unwrap();
            let mut metadata = doc.datasets[0].metadata.clone();
            metadata.donor_column = "replicate".into();
            metadata.condition_column = "cell_type".into();
            metadata.perturbation_column = "label".into();
            metadata.notes =
                "Public software QA; license and scientific independence not verified".into();
            let doc = store
                .update_dataset(r["sha256"].as_str().unwrap(), metadata, doc.revision)
                .unwrap();
            let doc = capture(
                &mut store,
                &worker,
                &scope,
                r["sha256"].as_str().unwrap(),
                doc.revision,
            )
            .unwrap();
            assert_eq!(doc.cohorts[0].observation.donors.len(), 8);
            assert_eq!(doc.cohorts[0].observation.retained, 24562);
            assert!(doc.cohorts[0].observation.condition_value.is_none());
            assert!(doc.cohorts[0].observation.perturbation_value.is_none());
            assert!(capture(
                &mut store,
                &worker,
                "wrong-project",
                r["sha256"].as_str().unwrap(),
                doc.revision
            )
            .is_err());
            let mut settings = doc.cohorts[0].settings.clone();
            settings.study_id = "Kang2018".into();
            settings.partition = "train".into();
            settings.is_control = true;
            settings.mappings[0].canonical_id = "QA-known-source-donor".into();
            settings.mappings[0].reviewed = true;
            settings.mappings[0].evidence="Software QA: same source and original donor label only; no independent study claim".into();
            store
                .update_cohort(r["sha256"].as_str().unwrap(), settings, doc.revision)
                .unwrap();
            workers.push(worker);
            stores.push(store);
        }
        let before = stores[0].snapshot();
        let mut changed = stores[1].snapshot().cohorts[0].settings.clone();
        changed.partition = "test".into();
        changed.evidence = "B-only QA note".into();
        let rev = stores[1].snapshot().revision;
        stores[1]
            .update_cohort(&before.cohorts[0].observation.source_sha256, changed, rev)
            .unwrap();
        assert_eq!(
            stores[0].snapshot().cohorts[0].settings,
            before.cohorts[0].settings
        );
        let scope = workers[0].snapshot().project_key;
        let report = assess(&before, &workers[0], &scope).unwrap();
        assert!(!report.ready);
        assert!(!report.cohorts[0]
            .issues
            .contains(&"source_unverified".into()));
        assert!(report.cohorts[0]
            .issues
            .contains(&"condition_unresolved".into()));
        assert!(report.cohorts[0].issues.contains(&"qc_partial".into()));
        let stage = project::stage_save(a.path(), &before, &workers[0]).unwrap();
        let package = root.path().join("donors.causalmed");
        project::save(stage.path(), &package).unwrap();
        let unpacked = project::unpack(&package, root.path()).unwrap();
        let restored = project::validate(unpacked.path()).unwrap();
        let mut tampered = restored.clone();
        tampered.cohorts[0].observation.donors[0].label = "QA forged label".into();
        tampered.cohorts[0].settings.mappings[0].label = "QA forged label".into();
        std::fs::write(
            unpacked.path().join("document.json"),
            serde_json::to_vec(&tampered).unwrap(),
        )
        .unwrap();
        assert!(project::validate(unpacked.path()).is_err());
        std::fs::write(
            unpacked.path().join("document.json"),
            serde_json::to_vec(&restored).unwrap(),
        )
        .unwrap();

        assert_eq!(restored.cohorts[0].settings, before.cohorts[0].settings);
        assert_eq!(
            restored.cohorts[0].observation.donors,
            before.cohorts[0].observation.donors
        );
        std::fs::copy(
            unpacked.path().join("selected.json"),
            unpacked.path().join("h5ad-metadata.json"),
        )
        .unwrap();
        let worker = Worker::new(unpacked.path());
        let scope = worker.snapshot().project_key;
        assert!(assess(&restored, &worker, &scope).unwrap().cohorts[0]
            .issues
            .contains(&"source_unverified".into()));
        let relocated = root.path().join("relocated.h5ad");
        std::fs::rename(&original, &relocated).unwrap();
        let o = &restored.cohorts[0].observation;
        worker.reserve(120).unwrap();
        worker.run_relink(
            &crate::h5ad::python_path(),
            crate::h5ad::worker_script(),
            &relocated,
            &o.source_sha256,
        );
        assert!(worker.snapshot().source_linked);
        let restarted = Worker::new(unpacked.path());
        assert!(!assess(&restored, &restarted, &scope).unwrap().cohorts[0]
            .issues
            .contains(&"source_unverified".into()));
        trash::history(&restarted, &scope, &o.qc_id, false).unwrap();
        assert!(assess(&restored, &restarted, &scope).unwrap().cohorts[0]
            .issues
            .contains(&"qc_unavailable".into()));
        trash::history(&restarted, &scope, &o.qc_id, true).unwrap();
        let source = restarted.export_source(&o.qc_id).unwrap();
        restarted.reserve(120).unwrap();
        let output = root.path().join("retained.h5ad");
        restarted.run_export(
            &crate::h5ad::python_path(),
            crate::h5ad::worker_script(),
            source,
            &output,
            "export_h5ad",
        );
        assert_eq!(restarted.snapshot().status, "completed");
        assert!(output.is_file());
        // Alter only our disposable copy. A bound path alone must never pass source integrity.
        use std::io::Write;
        std::fs::OpenOptions::new()
            .append(true)
            .open(&relocated)
            .unwrap()
            .write_all(b"QA changed copy")
            .unwrap();
        assert!(assess(&restored, &restarted, &scope).unwrap().cohorts[0]
            .issues
            .contains(&"source_unverified".into()));
    }
}
