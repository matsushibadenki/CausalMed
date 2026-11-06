pub mod benchmark;
pub mod cohorts;
pub mod datasets;
pub mod definitions;
pub mod evaluation;
pub use datasets::*;
pub mod model;
pub use model::*;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::Path};

pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Validation(String),
    #[error("storage_error: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("invalid_json: {0}")]
    Json(#[from] serde_json::Error),
}

pub const MAX_IMPORT_BYTES: usize = 2 * 1024 * 1024;
pub const FIXTURE: &str = include_str!("../../../fixtures/evidence.synthetic.json");
pub const BENCHMARK_FIXTURE: &str = include_str!("../../../fixtures/benchmark.synthetic.json");
pub fn checksum(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn invalid(code: &str) -> Error {
    Error::Validation(code.into())
}
fn bounded(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.contains('\0')
}

pub fn validate_bundle(bundle: &EvidenceBundle, document: &Document) -> Result<()> {
    if bundle.edges.len() > 5000 {
        return Err(invalid("invalid_bundle_size"));
    }
    validate_full_bundle(bundle, document)
}
fn validate_full_bundle(bundle: &EvidenceBundle, document: &Document) -> Result<()> {
    if bundle.schema_version != 1 {
        return Err(invalid("unsupported_schema"));
    }
    if bundle.sources.is_empty()
        || bundle.edges.is_empty()
        || document.edges.len() + bundle.edges.len() > 10000
    {
        return Err(invalid("invalid_bundle_size"));
    }
    let mut source_ids: HashSet<&str> = document.sources.iter().map(|s| s.id.as_str()).collect();
    for s in &bundle.sources {
        if !bounded(&s.id, 128)
            || !bounded(&s.title, 500)
            || !bounded(&s.version, 128)
            || !bounded(&s.license, 256)
            || !bounded(&s.locator, 2048)
        {
            return Err(invalid("invalid_source"));
        }
        if !s.locator.starts_with("https://")
            && !(s.synthetic && s.locator.starts_with("fixture://"))
        {
            return Err(invalid("invalid_source_locator"));
        }
        if !source_ids.insert(&s.id) {
            return Err(invalid("duplicate_source"));
        }
    }
    let mut edge_ids: HashSet<&str> = document.edges.iter().map(|e| e.id.as_str()).collect();
    for e in &bundle.edges {
        if !bounded(&e.id, 128)
            || !bounded(&e.source, 128)
            || !bounded(&e.target, 128)
            || !bounded(&e.cell_type, 256)
            || !bounded(&e.context, 2000)
            || !bounded(&e.excerpt, 8000)
        {
            return Err(invalid("invalid_edge"));
        }
        if !edge_ids.insert(&e.id) {
            return Err(invalid("duplicate_edge"));
        }
        if !source_ids.contains(e.source_id.as_str()) {
            return Err(invalid("missing_source"));
        }
        if e.evidence == EvidenceKind::Observation && e.relation != Relation::Associated {
            return Err(invalid("observation_cannot_assert_causality"));
        }
    }
    for edge in &bundle.edges {
        for id in &edge.contradictions {
            let other = bundle
                .edges
                .iter()
                .chain(&document.edges)
                .find(|e| e.id == *id);
            if id == &edge.id
                || other.is_none_or(|e| e.source != edge.source || e.target != edge.target)
            {
                return Err(invalid("invalid_contradiction"));
            }
        }
    }
    Ok(())
}

/// One owner for Document and WorkspaceState. Windows receive read-only projections.
pub struct ResearchStore {
    connection: Connection,
    document: Document,
}
impl ResearchStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_connection(Connection::open(path)?)
    }
    pub fn inspect(path: impl AsRef<Path>) -> Result<Document> {
        let connection =
            Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let json: String = connection.query_row(
            "SELECT payload FROM research_document WHERE id=1",
            [],
            |r| r.get(0),
        )?;
        let document: Document = serde_json::from_str(&json)?;
        if document.schema_version != 1 {
            return Err(invalid("unsupported_schema"));
        }
        datasets::validate_registry(&document.datasets)?;
        cohorts::validate_cohorts(&document)?;
        definitions::validate(&document)?;
        evaluation::validate(&document)?;
        Ok(document)
    }
    pub fn rename_project(&mut self, name: &str) -> Result<Document> {
        if !bounded(name, 120) {
            return Err(invalid("invalid_project_name"));
        }
        let mut document = self.snapshot();
        document.project_name = name.trim().into();
        self.commit(document)
    }
    pub fn in_memory() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory()?)
    }
    fn from_connection(connection: Connection) -> Result<Self> {
        connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS research_document (id INTEGER PRIMARY KEY CHECK (id=1), schema_version INTEGER NOT NULL, payload TEXT NOT NULL);")?;
        let existing = connection.query_row(
            "SELECT payload FROM research_document WHERE id=1",
            [],
            |r| r.get::<_, String>(0),
        );
        let document = match existing {
            Ok(json) => serde_json::from_str::<Document>(&json)?,
            Err(rusqlite::Error::QueryReturnedNoRows) => Document::default(),
            Err(e) => return Err(e.into()),
        };
        if document.schema_version != 1 {
            return Err(invalid("unsupported_schema"));
        }
        datasets::validate_registry(&document.datasets)?;
        cohorts::validate_cohorts(&document)?;
        definitions::validate(&document)?;
        evaluation::validate(&document)?;
        let mut store = Self {
            connection,
            document,
        };
        if store.document.revision == 0 {
            store.import_json(FIXTURE)?;
        }
        Ok(store)
    }
    pub fn snapshot(&self) -> Document {
        self.document.clone()
    }
    /// Restore a complete, validated research document into a staged project store.
    pub fn restore_document(
        &mut self,
        mut document: Document,
        minimum_revision: u64,
    ) -> Result<Document> {
        if document.schema_version != 1
            || !bounded(&document.project_name, 500)
            || document.edges.len() > 10000
            || document.hypotheses.len() > 10000
            || document.imports.len() > 10000
            || document.activity.len() > 100
        {
            return Err(invalid("invalid_project"));
        }
        datasets::validate_registry(&document.datasets)?;
        cohorts::validate_cohorts(&document)?;
        definitions::validate(&document)?;
        evaluation::validate(&document)?;
        let mut empty = Document::default();
        empty.sources.clear();
        empty.edges.clear();
        if !document.edges.is_empty() || !document.sources.is_empty() {
            // The import batch limit is 5,000; a restored accumulated document may contain 10,000.
            let bundle = EvidenceBundle {
                schema_version: 1,
                sources: document.sources.clone(),
                edges: document.edges.clone(),
            };
            validate_full_bundle(&bundle, &empty)?;
        }
        let edges: HashSet<_> = document.edges.iter().map(|e| e.id.as_str()).collect();
        let sources: HashSet<_> = document.sources.iter().map(|s| s.id.as_str()).collect();
        let mut ids = HashSet::new();
        for h in &document.hypotheses {
            if !ids.insert(&h.id)
                || !bounded(&h.id, 128)
                || !edges.contains(h.edge_id.as_str())
                || !bounded(&h.title, 500)
                || !bounded(&h.intervention, 4000)
                || !bounded(&h.falsification, 4000)
                || h.safety != "unknown"
            {
                return Err(invalid("invalid_project"));
            }
        }
        if document
            .workspace
            .selected_edge_id
            .as_deref()
            .is_some_and(|id| !edges.contains(id))
            || document
                .imports
                .iter()
                .any(|r| r.source_ids.iter().any(|id| !sources.contains(id.as_str())))
        {
            return Err(invalid("invalid_project"));
        }
        // Revision ordering remains monotonic across a project switch and all WebViews.
        document.revision = document.revision.max(minimum_revision);
        if document.revision == u64::MAX {
            return Err(invalid("invalid_project"));
        }
        if document.workspace.inspector_dock == Dock::Floating {
            document.workspace.inspector_dock = Dock::Right;
        }
        self.commit(document)
    }
    fn commit(&mut self, mut next: Document) -> Result<Document> {
        next.revision += 1;
        let json = serde_json::to_string(&next)?;
        let tx = self.connection.transaction()?;
        tx.execute("INSERT INTO research_document(id,schema_version,payload) VALUES(1,1,?1) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload", params![json])?;
        tx.commit()?;
        self.document = next;
        Ok(self.snapshot())
    }
    fn activity(doc: &mut Document, kind: &str, reference: &str) {
        doc.activity.insert(
            0,
            Activity {
                at: chrono::Utc::now().to_rfc3339(),
                kind: kind.into(),
                reference: reference.into(),
            },
        );
        doc.activity.truncate(100);
    }
    pub fn import_json(&mut self, json: &str) -> Result<Document> {
        if json.len() > MAX_IMPORT_BYTES {
            return Err(invalid("import_too_large"));
        }
        let digest = checksum(json.as_bytes());
        if self.document.imports.iter().any(|r| r.sha256 == digest) {
            return Err(invalid("already_imported"));
        }
        let bundle: EvidenceBundle = serde_json::from_str(json)?;
        validate_bundle(&bundle, &self.document)?;
        let mut next = self.snapshot();
        let import = ImportRecord {
            id: uuid::Uuid::new_v4().to_string(),
            sha256: digest,
            imported_at: chrono::Utc::now().to_rfc3339(),
            source_ids: bundle.sources.iter().map(|s| s.id.clone()).collect(),
            edge_count: bundle.edges.len(),
        };
        Self::activity(&mut next, "import", &import.id);
        next.sources.extend(bundle.sources);
        next.edges.extend(bundle.edges);
        next.imports.push(import);
        if next.workspace.selected_edge_id.is_none() {
            next.workspace.selected_edge_id = next.edges.first().map(|e| e.id.clone());
        }
        self.commit(next)
    }
    pub fn select_edge(&mut self, id: String) -> Result<Document> {
        if !self.document.edges.iter().any(|e| e.id == id) {
            return Err(invalid("edge_not_found"));
        }
        let mut next = self.snapshot();
        next.workspace.selected_edge_id = Some(id);
        self.commit(next)
    }
    pub fn set_locale(&mut self, locale: Locale) -> Result<Document> {
        let mut next = self.snapshot();
        next.workspace.locale = locale;
        self.commit(next)
    }
    pub fn set_dock(&mut self, dock: Dock) -> Result<Document> {
        let mut next = self.snapshot();
        next.workspace.inspector_dock = dock;
        self.commit(next)
    }
    pub fn create_hypothesis(&mut self, input: NewHypothesis) -> Result<Document> {
        if !bounded(&input.title, 200)
            || !bounded(&input.intervention, 2000)
            || !bounded(&input.falsification, 4000)
        {
            return Err(invalid("hypothesis_fields_required"));
        }
        if !self.document.edges.iter().any(|e| e.id == input.edge_id) {
            return Err(invalid("edge_not_found"));
        }
        let mut next = self.snapshot();
        let hypothesis = Hypothesis {
            id: uuid::Uuid::new_v4().to_string(),
            title: input.title.trim().into(),
            edge_id: input.edge_id,
            intervention: input.intervention.trim().into(),
            falsification: input.falsification.trim().into(),
            status: HypothesisStatus::Draft,
            created_at: chrono::Utc::now().to_rfc3339(),
            safety: "unknown".into(),
        };
        Self::activity(&mut next, "hypothesis", &hypothesis.id);
        next.hypotheses.push(hypothesis);
        self.commit(next)
    }
    pub fn set_hypothesis_status(
        &mut self,
        id: &str,
        status: HypothesisStatus,
    ) -> Result<Document> {
        let mut next = self.snapshot();
        let h = next
            .hypotheses
            .iter_mut()
            .find(|h| h.id == id)
            .ok_or_else(|| invalid("hypothesis_not_found"))?;
        h.status = status;
        Self::activity(&mut next, "status", id);
        self.commit(next)
    }
    pub fn run_synthetic_benchmark(&mut self) -> Result<Document> {
        let samples: Vec<benchmark::Sample> = serde_json::from_str(BENCHMARK_FIXTURE)?;
        let report = benchmark::evaluate(
            &samples,
            "held-out",
            "GENE-E",
            checksum(BENCHMARK_FIXTURE.as_bytes()),
        )?;
        let mut next = self.snapshot();
        Self::activity(&mut next, "benchmark", &report.id);
        next.benchmarks.insert(0, report);
        next.benchmarks.truncate(50);
        self.commit(next)
    }
}

#[cfg(test)]
mod tests;
