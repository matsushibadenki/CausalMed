use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Observation,
    Intervention,
    Hypothesis,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Relation {
    Activates,
    Inhibits,
    Associated,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    pub id: String,
    pub title: String,
    pub locator: String,
    pub license: String,
    pub version: String,
    pub synthetic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub relation: Relation,
    pub evidence: EvidenceKind,
    pub source_id: String,
    pub cell_type: String,
    pub context: String,
    pub excerpt: String,
    pub contradictions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EvidenceBundle {
    pub schema_version: u32,
    pub sources: Vec<Source>,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRecord {
    pub id: String,
    pub sha256: String,
    pub imported_at: String,
    pub source_ids: Vec<String>,
    pub edge_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hypothesis {
    pub id: String,
    pub title: String,
    pub edge_id: String,
    pub intervention: String,
    pub falsification: String,
    pub status: HypothesisStatus,
    pub created_at: String,
    pub safety: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HypothesisStatus {
    Draft,
    Planned,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NewHypothesis {
    pub title: String,
    pub edge_id: String,
    pub intervention: String,
    pub falsification: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    pub at: String,
    pub kind: String,
    pub reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Locale {
    En,
    Ja,
    Zh,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Dock {
    Right,
    Bottom,
    Floating,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceState {
    pub locale: Locale,
    pub selected_edge_id: Option<String>,
    pub inspector_dock: Dock,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub schema_version: u32,
    pub revision: u64,
    pub project_name: String,
    pub sources: Vec<Source>,
    pub edges: Vec<Edge>,
    pub imports: Vec<ImportRecord>,
    pub hypotheses: Vec<Hypothesis>,
    pub benchmarks: Vec<crate::benchmark::BenchmarkReport>,
    pub activity: Vec<Activity>,
    #[serde(default)]
    pub datasets: Vec<crate::datasets::DatasetRecord>,
    #[serde(default)]
    pub cohorts: Vec<crate::cohorts::CohortReview>,
    #[serde(default)]
    pub cohort_definitions: Vec<crate::definitions::CohortDefinition>,
    #[serde(default)]
    pub evaluation_plan: crate::evaluation::EvaluationPlan,
    #[serde(default)]
    pub evaluations: Vec<crate::evaluation::EvaluationRecord>,
    pub workspace: WorkspaceState,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            schema_version: 1,
            revision: 0,
            project_name: "IPF".into(),
            sources: vec![],
            edges: vec![],
            imports: vec![],
            hypotheses: vec![],
            benchmarks: vec![],
            activity: vec![],
            datasets: vec![],
            cohorts: vec![],
            cohort_definitions: vec![],
            evaluation_plan: Default::default(),
            evaluations: vec![],
            workspace: WorkspaceState {
                locale: Locale::Ja,
                selected_edge_id: None,
                inspector_dock: Dock::Right,
            },
        }
    }
}
