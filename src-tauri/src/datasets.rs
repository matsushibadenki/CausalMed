//! Observation registration takes trusted worker metadata, never client-supplied dimensions.
use crate::h5ad::Worker;
use causalmed_core::{AnnotationColumn, DatasetObservation, Document, ResearchStore};
pub(crate) fn register(
    store: &mut ResearchStore,
    worker: &Worker,
    scope: &str,
    sha: &str,
) -> Result<Document, String> {
    let state = worker.snapshot();
    if state.project_key != scope {
        return Err("project_changed".into());
    }
    if ["selecting", "running", "cancelling"].contains(&state.status.as_str()) {
        return Err("worker_busy".into());
    }
    let report = state.report.ok_or("dataset_missing")?;
    if report["sha256"] != sha || !crate::h5ad::valid_report(&report) {
        return Err("dataset_changed".into());
    }
    store
        .register_dataset(observation(&report)?)
        .map_err(|e| e.to_string())
}
pub(crate) fn observation(report: &serde_json::Value) -> Result<DatasetObservation, String> {
    let columns = report["obsColumns"].as_array().ok_or("invalid_dataset")?;
    let schema = if let Some(schema) = report["obsSchema"].as_array() {
        serde_json::from_value(serde_json::Value::Array(schema.clone()))
            .map_err(|_| "invalid_dataset")?
    } else {
        columns
            .iter()
            .take(256)
            .map(|c| AnnotationColumn {
                name: c.as_str().unwrap_or("").into(),
                dtype: "unknown".into(),
                kind: "unknown".into(),
                nullable: None,
            })
            .collect()
    };
    let observation = DatasetObservation {
        sha256: report["sha256"].as_str().ok_or("invalid_dataset")?.into(),
        name: report["name"].as_str().ok_or("invalid_dataset")?.into(),
        cells: report["cells"].as_u64().ok_or("invalid_dataset")?,
        genes: report["genes"].as_u64().ok_or("invalid_dataset")?,
        annotation_columns_total: report["totalObsColumns"]
            .as_u64()
            .map(|v| v as usize)
            .unwrap_or(columns.len()),
        annotation_schema: schema,
        anndata_version: report["anndataVersion"]
            .as_str()
            .unwrap_or("unknown")
            .into(),
    };
    Ok(observation)
}
