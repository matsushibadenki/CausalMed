//! Explicit project-local donor identities and a conservative cross-study preflight.
use crate::{invalid, Document, ResearchStore, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ObservedDonor {
    pub label: String,
    pub retained: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CohortObservation {
    pub source_sha256: String,
    pub qc_id: String,
    pub manifest_sha256: String,
    pub metadata_sha256: String,
    pub donor_column: String,
    pub condition_column: String,
    pub perturbation_column: String,
    pub retained: u64,
    pub missing_retained: u64,
    pub qc_status: String,
    pub donors: Vec<ObservedDonor>,
    pub condition_value: Option<String>,
    pub perturbation_value: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DonorMapping {
    pub label: String,
    pub canonical_id: String,
    pub reviewed: bool,
    pub evidence: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CohortSettings {
    pub study_id: String,
    pub context_id: String,
    pub perturbation_id: String,
    pub partition: String,
    pub is_control: bool,
    pub reviewed: bool,
    pub evidence: String,
    pub mappings: Vec<DonorMapping>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CohortReview {
    pub observation: CohortObservation,
    pub settings: CohortSettings,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CohortCheck {
    pub source_sha256: String,
    pub issues: Vec<String>,
    pub mapped: usize,
    pub total: usize,
    pub participating: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Eligibility {
    pub ready: bool,
    pub document_revision: u64,
    pub checked_at: String,
    pub issues: Vec<String>,
    pub cohorts: Vec<CohortCheck>,
    pub train_donors: usize,
    pub test_donors: usize,
    pub overlaps: Vec<String>,
}
fn bounded(s: &str, n: usize) -> bool {
    s.len() <= n && !s.contains('\0')
}
fn id(s: &str) -> bool {
    !s.is_empty() && s.trim() == s && bounded(s, 100)
}
fn sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
pub fn metadata_sha(dataset: &crate::DatasetRecord) -> String {
    crate::checksum(&serde_json::to_vec(&dataset.metadata).expect("metadata serialization"))
}
pub(crate) fn validate_cohorts(doc: &Document) -> Result<()> {
    if doc.cohorts.len() > 32 || serde_json::to_vec(&doc.cohorts)?.len() > 1024 * 1024 {
        return Err(invalid("cohort_limit"));
    }
    let mut sources = BTreeSet::new();
    for c in &doc.cohorts {
        let o = &c.observation;
        let s = &c.settings;
        let dataset = doc
            .datasets
            .iter()
            .find(|d| d.observation.sha256 == o.source_sha256)
            .ok_or_else(|| invalid("dataset_missing"))?;
        if !sources.insert(&o.source_sha256)
            || !sha(&o.source_sha256)
            || !sha(&o.manifest_sha256)
            || !sha(&o.metadata_sha256)
            || o.qc_id.len() != 36
            || !o.qc_id.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-')
            || o.donors.len() > 30
            || !bounded(&o.condition_column, 500)
            || !bounded(&o.perturbation_column, 500)
            || (o.condition_column.is_empty() && o.condition_value.is_some())
            || (o.perturbation_column.is_empty() && o.perturbation_value.is_some())
            || !bounded(&o.donor_column, 500)
            || o.donor_column.is_empty()
            || o.retained > dataset.observation.cells
            || o.missing_retained > o.retained
            || !["complete", "partial"].contains(&o.qc_status.as_str())
            || chrono::DateTime::parse_from_rfc3339(&c.updated_at).is_err()
        {
            return Err(invalid("invalid_cohort"));
        }
        let mut labels = BTreeSet::new();
        let mut total = o.missing_retained;
        for d in &o.donors {
            if d.label.trim().is_empty() || !bounded(&d.label, 200) || !labels.insert(&d.label) {
                return Err(invalid("invalid_cohort"));
            }
            total = total
                .checked_add(d.retained)
                .ok_or_else(|| invalid("invalid_cohort"))?;
        }
        if total != o.retained
            || [&o.condition_value, &o.perturbation_value].iter().any(|v| {
                v.as_ref()
                    .is_some_and(|s| s.trim().is_empty() || !bounded(s, 200))
            })
        {
            return Err(invalid("invalid_cohort"));
        }
        if !["unassigned", "train", "test", "exclude"].contains(&s.partition.as_str())
            || [&s.study_id, &s.context_id, &s.perturbation_id]
                .iter()
                .any(|v| !v.is_empty() && !id(v))
            || !bounded(&s.evidence, 2000)
            || s.mappings.len() != o.donors.len()
        {
            return Err(invalid("invalid_cohort"));
        }
        let mut mapped = BTreeSet::new();
        let mut canonical = BTreeSet::new();
        for m in &s.mappings {
            if !labels.contains(&m.label)
                || !mapped.insert(&m.label)
                || (!m.canonical_id.is_empty()
                    && (!id(&m.canonical_id) || !canonical.insert(&m.canonical_id)))
                || !bounded(&m.evidence, 1000)
                || (m.reviewed && (m.canonical_id.is_empty() || m.evidence.trim().is_empty()))
            {
                return Err(invalid("invalid_donor_mapping"));
            }
        }
        if s.reviewed
            && (!id(&s.study_id)
                || !id(&s.context_id)
                || !id(&s.perturbation_id)
                || s.evidence.trim().is_empty()
                || o.condition_value.is_none()
                || o.perturbation_value.is_none())
        {
            return Err(invalid("cohort_review_incomplete"));
        }
    }
    Ok(())
}
impl ResearchStore {
    pub fn capture_cohort(
        &mut self,
        observation: CohortObservation,
        expected_revision: u64,
    ) -> Result<Document> {
        if self.document.revision != expected_revision {
            return Err(invalid("dataset_changed"));
        }
        let mut doc = self.snapshot();
        let settings = CohortSettings {
            partition: "unassigned".into(),
            mappings: observation
                .donors
                .iter()
                .map(|d| DonorMapping {
                    label: d.label.clone(),
                    canonical_id: String::new(),
                    reviewed: false,
                    evidence: String::new(),
                })
                .collect(),
            ..Default::default()
        };
        let row = CohortReview {
            observation,
            settings,
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        if let Some(existing) = doc
            .cohorts
            .iter_mut()
            .find(|c| c.observation.source_sha256 == row.observation.source_sha256)
        {
            *existing = row;
        } else {
            doc.cohorts.push(row);
        }
        validate_cohorts(&doc)?;
        self.commit(doc)
    }
    pub fn update_cohort(
        &mut self,
        source: &str,
        settings: CohortSettings,
        expected_revision: u64,
    ) -> Result<Document> {
        if self.document.revision != expected_revision {
            return Err(invalid("dataset_changed"));
        }
        let mut doc = self.snapshot();
        let row = doc
            .cohorts
            .iter_mut()
            .find(|c| c.observation.source_sha256 == source)
            .ok_or_else(|| invalid("cohort_missing"))?;
        row.settings = settings;
        row.updated_at = chrono::Utc::now().to_rfc3339();
        validate_cohorts(&doc)?;
        self.commit(doc)
    }
}
/// Availability is freshly derived from this project's immutable QC archive and source bindings.
/// No readiness flag is persisted and no cell count is interpreted as replication.
pub fn assess(doc: &Document, availability: &BTreeMap<String, (bool, bool)>) -> Eligibility {
    let mut result = Eligibility {
        ready: false,
        document_revision: doc.revision,
        checked_at: chrono::Utc::now().to_rfc3339(),
        issues: vec![],
        cohorts: vec![],
        train_donors: 0,
        test_donors: 0,
        overlaps: vec![],
    };
    let mut train = BTreeSet::new();
    let mut test = BTreeSet::new();
    let mut train_studies = BTreeSet::new();
    let mut test_studies = BTreeSet::new();
    let mut contexts = BTreeSet::new();
    let mut train_perturbations = BTreeSet::new();
    let mut test_perturbations = BTreeSet::new();
    let (mut ntrain, mut ntest) = (0, 0);
    for c in &doc.cohorts {
        let o = &c.observation;
        let s = &c.settings;
        let mut issues = vec![];
        let mut issue = |code: &str| issues.push(code.to_owned());
        let d = doc
            .datasets
            .iter()
            .find(|d| d.observation.sha256 == o.source_sha256);
        if d.is_none_or(|d| !d.metadata.reviewed) {
            issue("provenance_unreviewed");
        }
        if d.is_none_or(|d| {
            metadata_sha(d) != o.metadata_sha256
                || d.metadata.donor_column != o.donor_column
                || d.metadata.condition_column != o.condition_column
                || d.metadata.perturbation_column != o.perturbation_column
        }) {
            issue("cohort_stale");
        }
        let (live, linked) = availability
            .get(&o.source_sha256)
            .copied()
            .unwrap_or_default();
        if !live {
            issue("qc_unavailable");
        }
        if !linked {
            issue("source_unverified");
        }
        if o.qc_status != "complete" {
            issue("qc_partial");
        }
        if o.retained == 0 {
            issue("empty_cohort");
        }
        if o.missing_retained > 0 {
            issue("missing_donors");
        }
        if o.condition_value.is_none() {
            issue("condition_unresolved");
        }
        if o.perturbation_value.is_none() {
            issue("perturbation_unresolved");
        }
        if !s.reviewed {
            issue("semantics_unreviewed");
        }
        if s.partition == "unassigned" {
            issue("partition_unassigned");
        }
        let active: Vec<_> = s
            .mappings
            .iter()
            .filter(|m| {
                o.donors
                    .iter()
                    .any(|d| d.label == m.label && d.retained > 0)
            })
            .collect();
        let mapped = active
            .iter()
            .filter(|m| m.reviewed && !m.canonical_id.is_empty())
            .count();
        if mapped != active.len() {
            issue("donors_unreviewed");
        }
        if s.partition == "train" {
            ntrain += 1;
            train_studies.insert(&s.study_id);
            contexts.insert(&s.context_id);
            train_perturbations.insert(&s.perturbation_id);
            if s.is_control {
                for m in &active {
                    if m.reviewed && !m.canonical_id.is_empty() {
                        train.insert(m.canonical_id.clone());
                    }
                }
            }
        }
        if s.partition == "test" {
            ntest += 1;
            test_studies.insert(&s.study_id);
            contexts.insert(&s.context_id);
            test_perturbations.insert(&s.perturbation_id);
            if s.is_control {
                issue("test_is_control");
            }
            for m in &active {
                if m.reviewed && !m.canonical_id.is_empty() {
                    test.insert(m.canonical_id.clone());
                }
            }
        }
        result.cohorts.push(CohortCheck {
            source_sha256: o.source_sha256.clone(),
            issues,
            mapped,
            total: active.len(),
            participating: s.partition != "exclude",
        });
    }
    let mut issue = |code: &str| result.issues.push(code.to_owned());
    if ntrain == 0 || ntest == 0 {
        issue("split_missing");
    }
    if train_studies.intersection(&test_studies).next().is_some() {
        issue("study_overlap");
    }
    if train_perturbations
        .intersection(&test_perturbations)
        .next()
        .is_some()
    {
        issue("perturbation_overlap");
    }
    if test_perturbations.len() != 1 || contexts.len() != 1 || contexts.contains(&String::new()) {
        issue("context_mismatch");
    }
    // Every training donor, including non-controls, must be disjoint from test donors.
    let all_train: BTreeSet<_> = doc
        .cohorts
        .iter()
        .filter(|c| c.settings.partition == "train")
        .flat_map(|c| {
            c.settings
                .mappings
                .iter()
                .filter(|m| {
                    m.reviewed
                        && c.observation
                            .donors
                            .iter()
                            .any(|d| d.label == m.label && d.retained > 0)
                })
                .map(|m| m.canonical_id.clone())
        })
        .collect();
    result.overlaps = all_train.intersection(&test).cloned().collect();
    if !result.overlaps.is_empty() {
        issue("donor_overlap");
    }
    result.train_donors = train.len();
    result.test_donors = test.len();
    if train.len() < 2 || test.len() < 2 {
        issue("insufficient_donors");
    }
    result.ready = result.issues.is_empty()
        && result
            .cohorts
            .iter()
            .all(|c| !c.participating || c.issues.is_empty());
    result
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn store() -> ResearchStore {
        let mut s = ResearchStore::in_memory().unwrap();
        for (i, sha) in ["a".repeat(64), "b".repeat(64)].iter().enumerate() {
            s.register_dataset(crate::DatasetObservation {
                sha256: sha.clone(),
                name: format!("synthetic QA {i}"),
                cells: 10,
                genes: 5,
                annotation_columns_total: 3,
                annotation_schema: ["donor", "condition", "perturbation"]
                    .iter()
                    .map(|name| crate::AnnotationColumn {
                        name: (*name).into(),
                        dtype: "category".into(),
                        kind: "categorical".into(),
                        nullable: Some(false),
                    })
                    .collect(),
                anndata_version: "test".into(),
            })
            .unwrap();
            let mut m = s.snapshot().datasets[i].metadata.clone();
            m.donor_column = "donor".into();
            m.condition_column = "condition".into();
            m.perturbation_column = "perturbation".into();
            m.source_url = "https://example.org/synthetic".into();
            m.source_version = "test".into();
            m.license = "Synthetic QA only".into();
            m.citation = "No biomedical findings".into();
            m.reviewed = true;
            let d = s.update_dataset(sha, m, s.snapshot().revision).unwrap();
            let donors = vec![
                ObservedDonor {
                    label: "D1".into(),
                    retained: 5,
                },
                ObservedDonor {
                    label: "D2".into(),
                    retained: 5,
                },
            ];
            let o = CohortObservation {
                source_sha256: sha.clone(),
                qc_id: format!("00000000-0000-4000-8000-{i:012x}"),
                manifest_sha256: "f".repeat(64),
                metadata_sha256: metadata_sha(&d.datasets[i]),
                donor_column: "donor".into(),
                condition_column: "condition".into(),
                perturbation_column: "perturbation".into(),
                retained: 10,
                missing_retained: 0,
                qc_status: "complete".into(),
                donors,
                condition_value: Some("synthetic context".into()),
                perturbation_value: Some(format!("synthetic {i}")),
            };
            let d = s.capture_cohort(o, d.revision).unwrap();
            let mut c = d.cohorts[i].settings.clone();
            c.study_id = format!("QA study {i}");
            c.context_id = "QA same context".into();
            c.perturbation_id = format!("QA perturbation {i}");
            c.partition = if i == 0 { "train" } else { "test" }.into();
            c.is_control = i == 0;
            c.reviewed = true;
            c.evidence = "Synthetic fixture attestation only".into();
            for (j, m) in c.mappings.iter_mut().enumerate() {
                m.canonical_id = format!("QA-{i}-{j}");
                m.evidence = "Synthetic mapping".into();
                m.reviewed = true;
            }
            s.update_cohort(sha, c, d.revision).unwrap();
        }
        s
    }
    fn available() -> BTreeMap<String, (bool, bool)> {
        [
            ("a".repeat(64), (true, true)),
            ("b".repeat(64), (true, true)),
        ]
        .into()
    }
    #[test]
    fn explicit_identities_hold_out_studies_perturbations_and_donors() {
        let s = store();
        let doc = s.snapshot();
        let ok = assess(&doc, &available());
        assert!(ok.ready);
        assert_eq!((ok.train_donors, ok.test_donors), (2, 2));
        // Equal original labels in different sources do not implicitly identify a person.
        assert!(ok.overlaps.is_empty());
        let mut overlap = doc.clone();
        overlap.cohorts[1].settings.mappings[0].canonical_id = "QA-0-0".into();
        let report = assess(&overlap, &available());
        assert!(!report.ready);
        assert_eq!(report.overlaps, vec!["QA-0-0"]);
        assert!(report.issues.contains(&"donor_overlap".into()));
        for (code, mutant) in [
            ("study_overlap", 0),
            ("perturbation_overlap", 1),
            ("context_mismatch", 2),
            ("insufficient_donors", 3),
        ] {
            let mut d = doc.clone();
            match mutant {
                0 => d.cohorts[1].settings.study_id = d.cohorts[0].settings.study_id.clone(),
                1 => {
                    d.cohorts[1].settings.perturbation_id =
                        d.cohorts[0].settings.perturbation_id.clone()
                }
                2 => d.cohorts[1].settings.context_id = "different".into(),
                _ => d.cohorts[0].settings.is_control = false,
            };
            let result = assess(&d, &available());
            assert!(!result.ready);
            assert!(result.issues.contains(&code.into()));
        }
    }
    #[test]
    fn stale_unreviewed_missing_partial_and_unavailable_sources_abstain() {
        let base = store().snapshot();
        for code in [
            "provenance_unreviewed",
            "cohort_stale",
            "qc_partial",
            "missing_donors",
            "condition_unresolved",
            "perturbation_unresolved",
            "semantics_unreviewed",
            "donors_unreviewed",
        ] {
            let mut d = base.clone();
            match code {
                "provenance_unreviewed" => d.datasets[0].metadata.reviewed = false,
                "cohort_stale" => d.datasets[0].metadata.notes = "changed".into(),
                "qc_partial" => d.cohorts[0].observation.qc_status = "partial".into(),
                "missing_donors" => d.cohorts[0].observation.missing_retained = 1,
                "condition_unresolved" => d.cohorts[0].observation.condition_value = None,
                "perturbation_unresolved" => d.cohorts[0].observation.perturbation_value = None,
                "semantics_unreviewed" => d.cohorts[0].settings.reviewed = false,
                _ => d.cohorts[0].settings.mappings[0].reviewed = false,
            };
            let result = assess(&d, &available());
            assert!(!result.ready);
            assert!(result.cohorts[0].issues.contains(&code.into()));
        }
        let report = assess(&base, &BTreeMap::new());
        assert!(!report.ready);
        assert!(report.cohorts[0].issues.contains(&"qc_unavailable".into()));
        assert!(report.cohorts[0]
            .issues
            .contains(&"source_unverified".into()));
    }
    #[test]
    fn mapping_validation_cas_and_capture_reset_are_atomic() {
        let mut s = store();
        let d = s.snapshot();
        let mut settings = d.cohorts[0].settings.clone();
        settings.mappings[1].canonical_id = settings.mappings[0].canonical_id.clone();
        assert!(s
            .update_cohort(&"a".repeat(64), settings, d.revision)
            .is_err());
        assert_eq!(s.snapshot().revision, d.revision);
        let mut settings = d.cohorts[0].settings.clone();
        settings.mappings[0].evidence.clear();
        assert!(s
            .update_cohort(&"a".repeat(64), settings, d.revision)
            .is_err());
        assert!(s
            .update_cohort(
                &"a".repeat(64),
                d.cohorts[0].settings.clone(),
                d.revision - 1
            )
            .is_err());
        let fresh = s
            .capture_cohort(d.cohorts[0].observation.clone(), d.revision)
            .unwrap();
        assert!(!fresh.cohorts[0].settings.reviewed);
        assert!(fresh.cohorts[0]
            .settings
            .mappings
            .iter()
            .all(|m| !m.reviewed && m.canonical_id.is_empty()));
        assert_eq!(fresh.cohorts[1].settings, d.cohorts[1].settings);
    }
    #[test]
    fn persistence_restore_legacy_and_project_isolation() {
        let original = store().snapshot();
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("research.sqlite3");
        let mut a = ResearchStore::open(&db).unwrap();
        a.restore_document(original.clone(), 0).unwrap();
        drop(a);
        let a = ResearchStore::open(&db).unwrap();
        assert_eq!(
            a.snapshot().cohorts[0].settings,
            original.cohorts[0].settings
        );
        let b = ResearchStore::in_memory().unwrap();
        assert!(b.snapshot().cohorts.is_empty());
        let mut bad = original.clone();
        bad.cohorts[0].observation.donors[0].retained += 1;
        let mut restored = ResearchStore::in_memory().unwrap();
        assert!(restored.restore_document(bad, 0).is_err());
        let mut json = serde_json::to_value(original).unwrap();
        json.as_object_mut().unwrap().remove("cohorts");
        let legacy: Document = serde_json::from_value(json).unwrap();
        assert!(legacy.cohorts.is_empty());
    }
}
