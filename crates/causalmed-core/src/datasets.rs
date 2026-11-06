//! Project-owned dataset identity and user-reviewed provenance, never matrix data.
use crate::{invalid, Document, ResearchStore, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnnotationColumn {
    pub name: String,
    pub dtype: String,
    pub kind: String,
    pub nullable: Option<bool>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DatasetObservation {
    pub sha256: String,
    pub name: String,
    pub cells: u64,
    pub genes: u64,
    pub annotation_columns_total: usize,
    pub annotation_schema: Vec<AnnotationColumn>,
    pub anndata_version: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DatasetMetadata {
    pub name: String,
    pub source_url: String,
    pub source_version: String,
    pub license: String,
    pub license_url: String,
    pub citation: String,
    pub notes: String,
    pub donor_column: String,
    pub condition_column: String,
    pub perturbation_column: String,
    pub cell_type_column: String,
    pub reviewed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DatasetRecord {
    pub observation: DatasetObservation,
    pub metadata: DatasetMetadata,
    pub created_at: String,
    pub updated_at: String,
    pub reviewed_at: Option<String>,
}
fn text(s: &str, max: usize) -> bool {
    s.len() <= max && !s.contains('\0')
}
fn sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn url(s: &str) -> bool {
    s.is_empty()
        || url::Url::parse(s).is_ok_and(|u| {
            u.scheme() == "https"
                && u.host_str().is_some()
                && u.username().is_empty()
                && u.password().is_none()
        })
}
fn validate(record: &DatasetRecord) -> Result<()> {
    let o = &record.observation;
    let m = &record.metadata;
    if !sha(&o.sha256)
        || o.name.is_empty()
        || !text(&o.name, 2000)
        || o.cells == 0
        || o.genes == 0
        || o.annotation_schema.len() > 256
        || o.annotation_schema.len() > o.annotation_columns_total
        || !text(&o.anndata_version, 80)
    {
        return Err(invalid("invalid_dataset"));
    }
    let mut names = HashSet::new();
    for c in &o.annotation_schema {
        if !text(&c.name, 500)
            || !names.insert(&c.name)
            || !text(&c.dtype, 100)
            || c.dtype.is_empty()
            || !["numeric", "categorical", "string", "boolean", "unknown"]
                .contains(&c.kind.as_str())
        {
            return Err(invalid("invalid_dataset"));
        }
    }
    if m.name.trim().is_empty()
        || !text(&m.name, 200)
        || !text(&m.source_url, 2048)
        || !text(&m.license_url, 2048)
        || !url(&m.source_url)
        || !url(&m.license_url)
        || !text(&m.source_version, 200)
        || !text(&m.license, 500)
        || !text(&m.citation, 2000)
        || !text(&m.notes, 4000)
    {
        return Err(invalid("invalid_dataset_metadata"));
    }
    for column in [
        &m.donor_column,
        &m.condition_column,
        &m.perturbation_column,
        &m.cell_type_column,
    ] {
        if !column.is_empty() && !names.contains(column) {
            return Err(invalid("invalid_dataset_column"));
        }
    }
    if m.reviewed
        && (m.source_url.is_empty()
            || m.source_version.trim().is_empty()
            || m.license.trim().is_empty()
            || m.citation.trim().is_empty())
    {
        return Err(invalid("dataset_review_incomplete"));
    }
    if m.reviewed != record.reviewed_at.is_some()
        || chrono::DateTime::parse_from_rfc3339(&record.created_at).is_err()
        || chrono::DateTime::parse_from_rfc3339(&record.updated_at).is_err()
        || record
            .reviewed_at
            .as_ref()
            .is_some_and(|s| chrono::DateTime::parse_from_rfc3339(s).is_err())
    {
        return Err(invalid("invalid_dataset"));
    }
    Ok(())
}
pub(crate) fn validate_registry(records: &[DatasetRecord]) -> Result<()> {
    if records.len() > 128 || serde_json::to_vec(records)?.len() > 4 * 1024 * 1024 {
        return Err(invalid("dataset_registry_full"));
    }
    let mut seen = HashSet::new();
    for r in records {
        validate(r)?;
        if !seen.insert(&r.observation.sha256) {
            return Err(invalid("invalid_dataset"));
        }
    }
    Ok(())
}
impl ResearchStore {
    pub fn register_dataset(&mut self, observation: DatasetObservation) -> Result<Document> {
        let now = chrono::Utc::now().to_rfc3339();
        validate(&DatasetRecord {
            observation: observation.clone(),
            metadata: DatasetMetadata {
                name: observation.name.chars().take(48).collect(),
                ..Default::default()
            },
            created_at: now.clone(),
            updated_at: now,
            reviewed_at: None,
        })?;
        let mut doc = self.snapshot();
        if let Some(existing) = doc
            .datasets
            .iter_mut()
            .find(|r| r.observation.sha256 == observation.sha256)
        {
            let old = &existing.observation;
            if old.cells != observation.cells
                || old.genes != observation.genes
                || old.annotation_columns_total != observation.annotation_columns_total
            {
                return Err(invalid("dataset_observation_conflict"));
            }
            // Older stored reports can supply names only. Fill unknown types once,
            // but never silently replace an already-observed contradictory schema.
            let upgrade = old.annotation_schema.iter().all(|c| c.dtype == "unknown")
                && observation
                    .annotation_schema
                    .iter()
                    .any(|c| c.dtype != "unknown")
                && old.annotation_schema.iter().all(|c| {
                    observation
                        .annotation_schema
                        .iter()
                        .any(|n| n.name == c.name)
                });
            if !upgrade {
                if old.annotation_schema != observation.annotation_schema {
                    return Err(invalid("dataset_observation_conflict"));
                }
                return Ok(doc);
            }
            existing.observation = observation;
            existing.updated_at = chrono::Utc::now().to_rfc3339();
        } else {
            let name = observation.name.chars().take(48).collect();
            let now = chrono::Utc::now().to_rfc3339();
            doc.datasets.push(DatasetRecord {
                observation,
                metadata: DatasetMetadata {
                    name,
                    ..Default::default()
                },
                created_at: now.clone(),
                updated_at: now,
                reviewed_at: None,
            });
        }
        validate_registry(&doc.datasets)?;
        self.commit(doc)
    }
    pub fn update_dataset(
        &mut self,
        sha256: &str,
        metadata: DatasetMetadata,
        expected_revision: u64,
    ) -> Result<Document> {
        if self.document.revision != expected_revision {
            return Err(invalid("dataset_changed"));
        }
        let mut doc = self.snapshot();
        let row = doc
            .datasets
            .iter_mut()
            .find(|r| r.observation.sha256 == sha256)
            .ok_or_else(|| invalid("dataset_missing"))?;
        if row.metadata == metadata {
            return Ok(doc);
        }
        let now = chrono::Utc::now().to_rfc3339();
        row.reviewed_at = metadata.reviewed.then_some(now.clone());
        row.metadata = metadata;
        row.updated_at = now;
        validate_registry(&doc.datasets)?;
        self.commit(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn observation() -> DatasetObservation {
        DatasetObservation {
            sha256: "a".repeat(64),
            name: "synthetic QA.h5ad".into(),
            cells: 10,
            genes: 5,
            annotation_columns_total: 1,
            annotation_schema: vec![AnnotationColumn {
                name: "donor".into(),
                dtype: "category".into(),
                kind: "categorical".into(),
                nullable: Some(false),
            }],
            anndata_version: "test".into(),
        }
    }
    #[test]
    fn durable_identity_review_cas_and_project_isolation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("research.sqlite3");
        let mut a = ResearchStore::open(&path).unwrap();
        let mut b = ResearchStore::in_memory().unwrap();
        let first = a.register_dataset(observation()).unwrap();
        let mut alias = observation();
        alias.name = "moved.h5ad".into();
        assert_eq!(a.register_dataset(alias).unwrap().revision, first.revision);
        let mut m = first.datasets[0].metadata.clone();
        m.donor_column = "donor".into();
        m.reviewed = true;
        assert!(a
            .update_dataset(&"a".repeat(64), m.clone(), first.revision)
            .unwrap_err()
            .to_string()
            .contains("review_incomplete"));
        m.source_url = "https://example.org/qa".into();
        m.source_version = "synthetic v1".into();
        m.license = "QA only".into();
        m.citation = "Synthetic test; no biomedical findings".into();
        let edited = a
            .update_dataset(&"a".repeat(64), m.clone(), first.revision)
            .unwrap();
        assert!(edited.datasets[0].reviewed_at.is_some());
        assert!(a
            .update_dataset(&"a".repeat(64), m.clone(), first.revision)
            .unwrap_err()
            .to_string()
            .contains("dataset_changed"));
        b.register_dataset(observation()).unwrap();
        assert!(!b.snapshot().datasets[0].metadata.reviewed);
        drop(a);
        let a = ResearchStore::open(&path).unwrap();
        assert_eq!(a.snapshot().datasets[0].metadata, m);
        let mut restored = ResearchStore::in_memory().unwrap();
        restored.restore_document(a.snapshot(), 0).unwrap();
        assert_eq!(restored.snapshot().datasets[0].metadata, m);
    }
    #[test]
    fn rejects_invalid_roles_urls_and_conflicting_observations() {
        let mut s = ResearchStore::in_memory().unwrap();
        let d = s.register_dataset(observation()).unwrap();
        let base = d.datasets[0].metadata.clone();
        for url in [
            "http://example.org",
            "https://user:pass@example.org",
            "file:///tmp/a",
        ] {
            let mut m = base.clone();
            m.source_url = url.into();
            assert!(s.update_dataset(&"a".repeat(64), m, d.revision).is_err());
        }
        let mut m = base.clone();
        m.donor_column = "missing".into();
        assert!(s.update_dataset(&"a".repeat(64), m, d.revision).is_err());
        let mut o = observation();
        o.cells += 1;
        assert!(s.register_dataset(o).is_err());
        assert_eq!(s.snapshot().revision, d.revision);
        let mut bad = d.clone();
        bad.datasets.push(bad.datasets[0].clone());
        assert!(s.restore_document(bad, 0).is_err());
        let mut legacy = serde_json::to_value(d).unwrap();
        legacy.as_object_mut().unwrap().remove("datasets");
        let legacy: Document = serde_json::from_value(legacy).unwrap();
        assert!(legacy.datasets.is_empty());
    }
    #[test]
    fn unknown_schema_upgrade_preserves_user_metadata() {
        let mut s = ResearchStore::in_memory().unwrap();
        let mut o = observation();
        o.annotation_schema[0].dtype = "unknown".into();
        o.annotation_schema[0].kind = "unknown".into();
        o.annotation_schema[0].nullable = None;
        let d = s.register_dataset(o).unwrap();
        let mut m = d.datasets[0].metadata.clone();
        m.notes = "Keep provenance".into();
        s.update_dataset(&"a".repeat(64), m.clone(), d.revision)
            .unwrap();
        let upgraded = s.register_dataset(observation()).unwrap();
        assert_eq!(upgraded.datasets[0].metadata, m);
        assert_eq!(
            upgraded.datasets[0].observation.annotation_schema[0].dtype,
            "category"
        );
    }
}
