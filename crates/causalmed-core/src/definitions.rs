//! Immutable mixed-source cohort definitions and receipts; original matrices remain external.
use crate::{
    cohorts::{self, CohortObservation, CohortReview, CohortSettings},
    invalid, DatasetMetadata, DatasetObservation, DatasetRecord, Document, ResearchStore, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DefinitionPayload {
    pub id: String,
    pub name: String,
    pub source_sha256: String,
    pub source_cells: u64,
    pub source_genes: u64,
    pub metadata: DatasetMetadata,
    pub alignment: CohortReview,
    pub condition_value: String,
    pub perturbation_value: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub condition_values: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub perturbation_values: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CohortExport {
    pub sha256: String,
    pub file_name: String,
    pub bytes: u64,
    pub retained: u64,
    pub matched: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CohortDefinition {
    pub payload: DefinitionPayload,
    pub sha256: String,
    pub exports: Vec<CohortExport>,
}
impl DefinitionPayload {
    pub fn conditions(&self) -> Vec<&str> {
        effective(&self.condition_values, &self.condition_value)
    }
    pub fn perturbations(&self) -> Vec<&str> {
        effective(&self.perturbation_values, &self.perturbation_value)
    }
}
fn effective<'a>(values: &'a [String], single: &'a str) -> Vec<&'a str> {
    if values.is_empty() {
        vec![single]
    } else {
        values.iter().map(String::as_str).collect()
    }
}
fn valid_values(values: &[String], single: &str) -> bool {
    values.is_empty()
        || (values.len() >= 2
            && values.len() <= 20
            && values.first().map(String::as_str) == Some(single)
            && values.iter().all(|v| text(v, 200))
            && values.windows(2).all(|v| v[0] < v[1]))
}
fn observed_matches(observed: Option<&str>, values: Vec<&str>) -> bool {
    match observed {
        Some(v) => values.contains(&v),
        None => values.len() > 1,
    }
}
pub fn payload_json(p: &DefinitionPayload) -> String {
    serde_json::to_string(p).expect("definition serialization")
}
pub fn payload_sha(p: &DefinitionPayload) -> String {
    crate::checksum(payload_json(p).as_bytes())
}
fn sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.contains('\0')
}
pub(crate) fn validate(doc: &Document) -> Result<()> {
    if doc.cohort_definitions.len() > 64
        || serde_json::to_vec(&doc.cohort_definitions)?.len() > 2 * 1024 * 1024
    {
        return Err(invalid("definition_limit"));
    }
    let mut ids = BTreeSet::new();
    for d in &doc.cohort_definitions {
        let p = &d.payload;
        let parent = doc
            .datasets
            .iter()
            .find(|r| r.observation.sha256 == p.source_sha256)
            .ok_or_else(|| invalid("dataset_missing"))?;
        if uuid::Uuid::parse_str(&p.id).is_err()
            || !ids.insert(&p.id)
            || !text(&p.name, 200)
            || !text(&p.condition_value, 200)
            || !text(&p.perturbation_value, 200)
            || !valid_values(&p.condition_values, &p.condition_value)
            || !valid_values(&p.perturbation_values, &p.perturbation_value)
            || p.metadata.condition_column.is_empty()
            || p.metadata.perturbation_column.is_empty()
            || p.metadata.condition_column == p.metadata.perturbation_column
            || p.alignment.observation.source_sha256 != p.source_sha256
            || p.alignment.observation.donor_column != p.metadata.donor_column
            || p.alignment.observation.condition_column != p.metadata.condition_column
            || p.alignment.observation.perturbation_column != p.metadata.perturbation_column
            || p.source_cells != parent.observation.cells
            || p.source_genes != parent.observation.genes
            || payload_sha(p) != d.sha256
            || payload_json(p).len() > 64 * 1024
            || d.exports.len() > 8
            || chrono::DateTime::parse_from_rfc3339(&p.created_at).is_err()
        {
            return Err(invalid("invalid_definition"));
        }
        let mut snapshot = doc.clone();
        snapshot.cohorts = vec![p.alignment.clone()];
        cohorts::validate_cohorts(&snapshot)?;
        let mut source = parent.clone();
        source.metadata = p.metadata.clone();
        source.reviewed_at = source.metadata.reviewed.then(|| p.created_at.clone());
        crate::datasets::validate_registry(&[source.clone()])?;
        if cohorts::metadata_sha(&source) != p.alignment.observation.metadata_sha256 {
            return Err(invalid("invalid_definition"));
        }
        for r in &d.exports {
            if !sha(&r.sha256)
                || !text(&r.file_name, 2000)
                || r.file_name.contains('/')
                || r.file_name.contains('\\')
                || r.bytes == 0
                || r.retained == 0
                || r.retained > r.matched
                || r.matched > p.source_cells
            {
                return Err(invalid("invalid_definition"));
            }
        }
    }
    Ok(())
}
impl ResearchStore {
    pub fn create_cohort_definition(
        &mut self,
        payload: DefinitionPayload,
        revision: u64,
    ) -> Result<Document> {
        if self.document.revision != revision {
            return Err(invalid("dataset_changed"));
        }
        let mut doc = self.snapshot();
        let sha256 = payload_sha(&payload);
        doc.cohort_definitions.push(CohortDefinition {
            payload,
            sha256,
            exports: vec![],
        });
        validate(&doc)?;
        self.commit(doc)
    }
    pub fn rename_cohort_definition(
        &mut self,
        id: &str,
        name: String,
        revision: u64,
    ) -> Result<Document> {
        if self.document.revision != revision {
            return Err(invalid("dataset_changed"));
        }
        let mut doc = self.snapshot();
        let row = doc
            .cohort_definitions
            .iter_mut()
            .find(|d| d.payload.id == id)
            .ok_or_else(|| invalid("definition_missing"))?;
        if !row.exports.is_empty() {
            return Err(invalid("definition_locked"));
        }
        row.payload.name = name;
        row.sha256 = payload_sha(&row.payload);
        validate(&doc)?;
        self.commit(doc)
    }
    pub fn record_cohort_export(&mut self, id: &str, receipt: CohortExport) -> Result<Document> {
        let mut doc = self.snapshot();
        let row = doc
            .cohort_definitions
            .iter_mut()
            .find(|r| r.payload.id == id)
            .ok_or_else(|| invalid("definition_missing"))?;
        if row.exports.contains(&receipt) {
            return Ok(doc);
        }
        if row.exports.len() == 8 {
            row.exports.remove(0);
        }
        row.exports.push(receipt);
        validate(&doc)?;
        self.commit(doc)
    }
    /// A derived input is explicitly adopted only after a project-local export SHA matches.
    pub fn adopt_cohort(
        &mut self,
        definition_id: &str,
        observation: DatasetObservation,
        mut census: CohortObservation,
        revision: u64,
    ) -> Result<Document> {
        if self.document.revision != revision {
            return Err(invalid("dataset_changed"));
        }
        let mut doc = self.snapshot();
        let def = doc
            .cohort_definitions
            .iter()
            .find(|d| d.payload.id == definition_id)
            .ok_or_else(|| invalid("definition_missing"))?
            .clone();
        if doc
            .datasets
            .iter()
            .any(|d| d.observation.sha256 == observation.sha256)
        {
            return Err(invalid("cohort_already_adopted"));
        }
        if !def
            .exports
            .iter()
            .any(|r| r.sha256 == observation.sha256 && r.retained == observation.cells)
            || observation.genes != def.payload.source_genes
            || census.source_sha256 != observation.sha256
            || census.donor_column != def.payload.metadata.donor_column
            || !observed_matches(census.condition_value.as_deref(), def.payload.conditions())
            || !observed_matches(
                census.perturbation_value.as_deref(),
                def.payload.perturbations(),
            )
        {
            return Err(invalid("cohort_origin_mismatch"));
        }
        let now = chrono::Utc::now().to_rfc3339();
        let mut metadata = def.payload.metadata.clone();
        metadata.name = def.payload.name.clone();
        metadata.reviewed = false;
        let dataset = DatasetRecord {
            observation,
            metadata,
            created_at: now.clone(),
            updated_at: now.clone(),
            reviewed_at: None,
        };
        census.metadata_sha256 = cohorts::metadata_sha(&dataset);
        let mut settings = CohortSettings {
            study_id: def.payload.alignment.settings.study_id.clone(),
            context_id: def.payload.alignment.settings.context_id.clone(),
            perturbation_id: def.payload.alignment.settings.perturbation_id.clone(),
            partition: "unassigned".into(),
            ..Default::default()
        };
        for donor in &census.donors {
            let mapping = def
                .payload
                .alignment
                .settings
                .mappings
                .iter()
                .find(|m| m.label == donor.label)
                .ok_or_else(|| invalid("cohort_origin_mismatch"))?;
            settings.mappings.push(mapping.clone());
        }
        doc.datasets.push(dataset);
        doc.cohorts.push(CohortReview {
            observation: census,
            settings,
            updated_at: now,
        });
        crate::datasets::validate_registry(&doc.datasets)?;
        cohorts::validate_cohorts(&doc)?;
        validate(&doc)?;
        self.commit(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn defined() -> ResearchStore {
        let mut s = crate::cohorts::tests::store();
        let doc = s.snapshot();
        let p = DefinitionPayload {
            id: "00000000-0000-4000-8000-000000000010".into(),
            name: "Synthetic condition A control".into(),
            source_sha256: "a".repeat(64),
            source_cells: 10,
            source_genes: 5,
            metadata: doc.datasets[0].metadata.clone(),
            alignment: doc.cohorts[0].clone(),
            condition_value: "A".into(),
            perturbation_value: "ctrl".into(),
            created_at: chrono::Utc::now().to_rfc3339(),
            condition_values: vec![],
            perturbation_values: vec![],
        };
        s.create_cohort_definition(p, doc.revision).unwrap();
        s
    }
    fn child(doc: &Document) -> (DatasetObservation, CohortObservation) {
        let mut dataset = doc.datasets[0].observation.clone();
        dataset.sha256 = "c".repeat(64);
        dataset.cells = 4;
        let mut o = doc.cohorts[0].observation.clone();
        o.source_sha256 = "c".repeat(64);
        o.qc_id = "00000000-0000-4000-8000-000000000020".into();
        o.retained = 4;
        o.donors.iter_mut().for_each(|d| d.retained = 2);
        o.condition_value = Some("A".into());
        o.perturbation_value = Some("ctrl".into());
        (dataset, o)
    }
    #[test]
    fn frozen_definition_receipt_and_atomic_adoption_preserve_donor_reviews() {
        let mut s = defined();
        let doc = s.snapshot();
        let def = doc.cohort_definitions[0].clone();
        let mut edited = doc.cohorts[0].settings.clone();
        edited.mappings[0].canonical_id = "Edited after definition".into();
        s.update_cohort(&"a".repeat(64), edited, doc.revision)
            .unwrap();
        assert_eq!(s.snapshot().cohort_definitions[0].sha256, def.sha256);
        let (dataset, census) = child(&doc);
        assert!(s
            .adopt_cohort(
                &def.payload.id,
                dataset.clone(),
                census.clone(),
                s.snapshot().revision
            )
            .is_err());
        assert_eq!(s.snapshot().datasets.len(), 2);
        s.record_cohort_export(
            &def.payload.id,
            CohortExport {
                sha256: dataset.sha256.clone(),
                file_name: "synthetic-child.h5ad".into(),
                bytes: 100,
                retained: 4,
                matched: 5,
            },
        )
        .unwrap();
        let rev = s.snapshot().revision;
        let adopted = s
            .adopt_cohort(&def.payload.id, dataset.clone(), census.clone(), rev)
            .unwrap();
        let child = adopted.cohorts.last().unwrap();
        assert_eq!(
            child.settings.mappings,
            def.payload.alignment.settings.mappings
        );
        assert_eq!(child.settings.partition, "unassigned");
        assert!(!child.settings.reviewed);
        assert!(!adopted.datasets.last().unwrap().metadata.reviewed);
        assert!(s
            .adopt_cohort(&def.payload.id, dataset, census, adopted.revision)
            .is_err());
        assert_eq!(s.snapshot().revision, adopted.revision);
    }
    #[test]
    fn name_correction_is_cas_guarded_and_locked_after_export() {
        let mut s = defined();
        let d = s.snapshot();
        let id = d.cohort_definitions[0].payload.id.clone();
        let corrected = s
            .rename_cohort_definition(&id, "Corrected synthetic name".into(), d.revision)
            .unwrap();
        assert_ne!(
            corrected.cohort_definitions[0].sha256,
            d.cohort_definitions[0].sha256
        );
        assert!(s
            .rename_cohort_definition(&id, "Stale edit".into(), d.revision)
            .is_err());
        s.record_cohort_export(
            &id,
            CohortExport {
                sha256: "c".repeat(64),
                file_name: "qa.h5ad".into(),
                bytes: 100,
                retained: 4,
                matched: 5,
            },
        )
        .unwrap();
        let revision = s.snapshot().revision;
        assert!(s
            .rename_cohort_definition(&id, "Must stay fixed".into(), revision)
            .is_err());
        assert_eq!(s.snapshot().revision, revision);
    }
    #[test]
    fn portable_validation_legacy_and_project_isolation() {
        let s = defined();
        let d = s.snapshot();
        let dir = tempfile::tempdir().unwrap();
        let mut restored = ResearchStore::open(dir.path().join("research.sqlite3")).unwrap();
        restored.restore_document(d.clone(), 0).unwrap();
        drop(restored);
        let restored = ResearchStore::open(dir.path().join("research.sqlite3")).unwrap();
        assert_eq!(
            restored.snapshot().cohort_definitions[0].sha256,
            d.cohort_definitions[0].sha256
        );
        let mut b = crate::cohorts::tests::store();
        let (dataset, census) = child(&d);
        assert!(b
            .adopt_cohort(
                &d.cohort_definitions[0].payload.id,
                dataset,
                census,
                b.snapshot().revision
            )
            .is_err());
        assert!(b.snapshot().cohort_definitions.is_empty());
        let mut tampered = d.clone();
        tampered.cohort_definitions[0].payload.condition_value = "forged".into();
        assert!(ResearchStore::in_memory()
            .unwrap()
            .restore_document(tampered, 0)
            .is_err());
        let mut json = serde_json::to_value(d).unwrap();
        json.as_object_mut().unwrap().remove("cohortDefinitions");
        let legacy: Document = serde_json::from_value(json).unwrap();
        assert!(legacy.cohort_definitions.is_empty());
    }
    #[test]
    fn multi_values_are_canonical_portable_and_legacy_bytes_stay_unchanged() {
        let legacy = defined().snapshot().cohort_definitions[0].clone();
        let raw = payload_json(&legacy.payload);
        assert!(!raw.contains("conditionValues"));
        assert!(!raw.contains("perturbationValues"));
        let old: DefinitionPayload = serde_json::from_str(&raw).unwrap();
        assert_eq!(payload_json(&old), raw);
        assert_eq!(payload_sha(&old), legacy.sha256);
        let mut p = old;
        p.id = "00000000-0000-4000-8000-000000000030".into();
        p.condition_values = vec!["A".into(), "B".into()];
        p.perturbation_values = vec!["ctrl".into(), "stim".into()];
        let mut store = defined();
        let d = store
            .create_cohort_definition(p.clone(), store.snapshot().revision)
            .unwrap();
        let mut restored = ResearchStore::in_memory().unwrap();
        restored.restore_document(d.clone(), 0).unwrap();
        assert_eq!(
            restored.snapshot().cohort_definitions[1].sha256,
            payload_sha(&p)
        );
        for invalid in [
            vec!["A".into()],
            vec!["A".into(), "A".into()],
            vec!["B".into(), "A".into()],
            vec!["A".into(), "".into()],
        ] {
            let mut bad = d.clone();
            let def = &mut bad.cohort_definitions[1];
            def.payload.condition_values = invalid;
            def.sha256 = payload_sha(&def.payload);
            assert!(ResearchStore::in_memory()
                .unwrap()
                .restore_document(bad, 0)
                .is_err());
        }
        assert!(observed_matches(None, p.conditions()));
        assert!(!observed_matches(Some("outside"), p.conditions()));
        assert!(!observed_matches(None, legacy.payload.conditions()));
    }
}
