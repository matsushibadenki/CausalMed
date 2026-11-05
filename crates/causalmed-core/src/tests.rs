use super::*;

#[test]
fn additional_import_preserves_existing_records_and_tracks_original_bytes() {
    let mut store = ResearchStore::in_memory().unwrap();
    let before = store.snapshot();
    let mut bundle: EvidenceBundle = serde_json::from_str(FIXTURE).unwrap();
    bundle.sources[0].id = "SYN-002".into();
    for edge in &mut bundle.edges {
        edge.source_id = "SYN-002".into();
        edge.id = format!("second-{}", edge.id);
        edge.contradictions = edge
            .contradictions
            .iter()
            .map(|id| format!("second-{id}"))
            .collect();
    }
    let bytes = serde_json::to_string_pretty(&bundle).unwrap();
    let after = store.import_json(&bytes).unwrap();
    assert_eq!(after.revision, before.revision + 1);
    assert_eq!(after.edges.len(), 12);
    assert_eq!(after.sources.len(), 2);
    assert_eq!(after.edges[0].id, before.edges[0].id);
    let record = after.imports.last().unwrap();
    assert_eq!(record.sha256, checksum(bytes.as_bytes()));
    assert_eq!(record.source_ids, vec!["SYN-002"]);
    assert_eq!(record.edge_count, 6);
}

#[test]
fn persists_document_and_hypothesis_across_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("research.sqlite3");
    let mut store = ResearchStore::open(&path).unwrap();
    store
        .create_hypothesis(NewHypothesis {
            title: "Synthetic test".into(),
            edge_id: "E-001".into(),
            intervention: "Suppress GENE-A".into(),
            falsification: "No change in independent replicates".into(),
        })
        .unwrap();
    store.set_locale(Locale::En).unwrap();
    let expected = store.snapshot();
    drop(store);
    let actual = ResearchStore::open(path).unwrap().snapshot();
    assert_eq!(actual.revision, expected.revision);
    assert_eq!(actual.hypotheses.len(), 1);
    assert_eq!(actual.workspace.locale, Locale::En);
    assert_eq!(actual.hypotheses[0].safety, "unknown");
}

#[test]
fn invalid_import_is_atomic_and_does_not_promote_observation() {
    let mut store = ResearchStore::in_memory().unwrap();
    let revision = store.snapshot().revision;
    let mut bundle: EvidenceBundle = serde_json::from_str(FIXTURE).unwrap();
    bundle.sources[0].id = "new-source".into();
    for e in &mut bundle.edges {
        e.source_id = "new-source".into();
        e.id = format!("new-{}", e.id);
        e.contradictions.clear();
    }
    bundle.edges[1].relation = Relation::Activates;
    let error = store
        .import_json(&serde_json::to_string(&bundle).unwrap())
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("observation_cannot_assert_causality"));
    assert_eq!(store.snapshot().revision, revision);
    assert_eq!(store.snapshot().sources.len(), 1);
}

#[test]
fn duplicates_or_broken_provenance_are_rejected() {
    let mut store = ResearchStore::in_memory().unwrap();
    assert!(store
        .import_json(FIXTURE)
        .unwrap_err()
        .to_string()
        .contains("already_imported"));
    let mut bundle: EvidenceBundle = serde_json::from_str(FIXTURE).unwrap();
    bundle.edges[0].source_id = "missing".into();
    assert!(validate_bundle(&bundle, &Document::default())
        .unwrap_err()
        .to_string()
        .contains("missing_source"));
    bundle.edges[0].source_id = "SYN-001".into();
    bundle.edges[0].contradictions = vec!["E-002".into()];
    assert!(validate_bundle(&bundle, &Document::default()).is_err());
}

#[test]
fn benchmark_holds_out_dataset_perturbation_and_donors() {
    let samples: Vec<benchmark::Sample> = serde_json::from_str(BENCHMARK_FIXTURE).unwrap();
    let r = benchmark::evaluate(&samples, "held-out", "GENE-E", "digest".into()).unwrap();
    assert!(!r.abstained);
    assert!((r.control_mean - 1.2).abs() < 1e-10);
    assert!((r.donor_mae - 0.6).abs() < 1e-10);
    assert_eq!(r.train_donors, 2);
    assert_eq!(r.test_donors, 2);
    for id in &r.train_ids {
        let s = samples.iter().find(|s| &s.id == id).unwrap();
        assert_ne!(s.dataset, "held-out");
        assert_ne!(s.perturbation, "GENE-E");
        assert!(!["D3", "D4"].contains(&s.donor.as_str()));
    }
}

#[test]
fn cell_replication_does_not_change_donor_weight() {
    let mut samples: Vec<benchmark::Sample> = serde_json::from_str(BENCHMARK_FIXTURE).unwrap();
    let original = benchmark::evaluate(&samples, "held-out", "GENE-E", "digest".into()).unwrap();
    let extra: Vec<_> = samples
        .iter()
        .filter(|s| s.donor == "D1")
        .cloned()
        .collect();
    for i in 0..100 {
        for s in &extra {
            let mut s = s.clone();
            s.id = format!("extra-{i}-{}", s.id);
            samples.push(s);
        }
    }
    let repeated = benchmark::evaluate(&samples, "held-out", "GENE-E", "digest".into()).unwrap();
    assert!((original.control_mean - repeated.control_mean).abs() < 1e-10);
}

#[test]
fn insufficient_donors_abstains_and_nonfinite_data_fails() {
    let mut samples: Vec<benchmark::Sample> = serde_json::from_str(BENCHMARK_FIXTURE).unwrap();
    samples.retain(|s| s.donor != "D4");
    assert!(
        benchmark::evaluate(&samples, "held-out", "GENE-E", "digest".into())
            .unwrap()
            .abstained
    );
    samples[0].value = f64::NAN;
    assert!(benchmark::evaluate(&samples, "held-out", "GENE-E", "digest".into()).is_err());
}

#[test]
fn missing_falsification_and_arbitrary_selection_fail() {
    let mut store = ResearchStore::in_memory().unwrap();
    assert!(store.select_edge("missing".into()).is_err());
    assert!(store
        .create_hypothesis(NewHypothesis {
            title: "x".into(),
            edge_id: "E-001".into(),
            intervention: "x".into(),
            falsification: " ".into()
        })
        .is_err());
}
