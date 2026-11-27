//! Portable, versioned, streaming project packages. Source H5ADs are external references.
use crate::h5ad::Worker;
use causalmed_core::{Document, ResearchStore};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAGIC: &[u8] = b"CAUSALMED-PROJECT\n";
const MAX_META: u64 = 8 * 1024 * 1024;
const MAX_TOTAL: u64 = 16 * 1024 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    name: String,
    bytes: u64,
    sha256: String,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Index {
    schema_version: u32,
    entries: Vec<Entry>,
}
const MAX_RUNS: usize = 10_000;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HistoryState {
    id: String,
    trashed_at: Option<u128>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HistoryStates {
    schema_version: u32,
    histories: Vec<HistoryState>,
}
fn history_states(folder: &Path, ids: &HashSet<String>) -> Result<Vec<HistoryState>, String> {
    if !folder.join("history-state.json").exists() {
        let scope = folder
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("invalid_project")?;
        return ids
            .iter()
            .map(|id| {
                Ok(HistoryState {
                    id: id.clone(),
                    trashed_at: crate::trash::history_timestamp(folder, id, scope)?,
                })
            })
            .collect();
    }
    let states: HistoryStates =
        serde_json::from_value(read_json(&folder.join("history-state.json"))?).map_err(err)?;
    let mut seen = HashSet::new();
    if states.schema_version != 1 || states.histories.len() > MAX_RUNS {
        return Err("invalid_project".into());
    }
    for state in &states.histories {
        if !super::h5ad::safe_id(&state.id)
            || !seen.insert(state.id.clone())
            || !ids.contains(&state.id)
        {
            return Err("invalid_project".into());
        }
    }
    if &seen != ids {
        return Err("invalid_project".into());
    }
    Ok(states.histories)
}
fn err(_: impl std::fmt::Display) -> String {
    "invalid_project".into()
}
pub fn digest(path: &Path) -> Result<String, String> {
    let mut input = File::open(path).map_err(err)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = input.read(&mut buffer).map_err(err)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn allowed(name: &str) -> bool {
    if ["document.json", "selected.json", "history-state.json"].contains(&name) {
        return true;
    }
    let p: Vec<_> = name.split('/').collect();
    p.len() == 3
        && p[0] == "qc-runs"
        && super::h5ad::safe_id(p[1])
        && ["report.json", "manifest.json", "cells.npz"].contains(&p[2])
}
fn read_json(path: &Path) -> Result<Value, String> {
    if fs::metadata(path).map_err(err)?.len() > MAX_META {
        return Err("invalid_project".into());
    }
    serde_json::from_slice(&fs::read(path).map_err(err)?).map_err(err)
}
/// Copy the authoritative metadata and immutable artifacts into a private staging directory.
pub fn stage_save(
    folder: &Path,
    doc: &Document,
    worker: &Worker,
) -> Result<tempfile::TempDir, String> {
    let stage = tempfile::tempdir_in(folder).map_err(err)?;
    fs::write(
        stage.path().join("document.json"),
        serde_json::to_vec(doc).map_err(err)?,
    )
    .map_err(err)?;
    let report = worker.snapshot().report;
    fs::write(
        stage.path().join("selected.json"),
        serde_json::json!({"report":report,"sourcePath":null}).to_string(),
    )
    .map_err(err)?;
    worker.visit_archives(|_| {}); // Recover legacy reports without collecting full history payloads.
    let mut states = Vec::new();
    let scope = worker.snapshot().project_key;
    let runs = worker.folder().join("qc-runs");
    let entries = match fs::read_dir(runs) {
        Ok(entries) => Some(entries),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(err(e)),
    };
    if let Some(entries) = entries {
        for entry in entries {
            let entry = entry.map_err(err)?;
            let id = entry.file_name().to_string_lossy().to_string();
            if !super::h5ad::safe_id(&id) {
                continue;
            }
            states.push(HistoryState {
                id: id.clone(),
                trashed_at: crate::trash::history_timestamp(worker.folder(), &id, &scope)?,
            });
            if states.len() > MAX_RUNS {
                return Err("invalid_project".into());
            }
            let stored = worker.archived_record(&id)?;
            let target = stage.path().join("qc-runs").join(&id);
            fs::create_dir_all(&target).map_err(err)?;
            fs::write(
                target.join("report.json"),
                serde_json::json!({"report":stored["report"],"sourcePath":null}).to_string(),
            )
            .map_err(err)?;
            for name in ["manifest.json", "cells.npz"] {
                fs::copy(entry.path().join(name), target.join(name)).map_err(err)?;
            }
        }
    }
    states.sort_by(|a, b| a.id.cmp(&b.id));
    fs::write(
        stage.path().join("history-state.json"),
        serde_json::to_vec(&HistoryStates {
            schema_version: 1,
            histories: states,
        })
        .map_err(err)?,
    )
    .map_err(err)?;
    validate(stage.path())?;
    Ok(stage)
}
/// Publish an exclusive destination after all bytes and source digests have been checked.
pub fn save(stage: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        return Err("destination_exists".into());
    }
    let mut names = vec![
        "document.json".to_string(),
        "selected.json".into(),
        "history-state.json".into(),
    ];
    if let Ok(entries) = fs::read_dir(stage.join("qc-runs")) {
        for entry in entries {
            let id = entry
                .map_err(err)?
                .file_name()
                .to_string_lossy()
                .to_string();
            for name in ["report.json", "manifest.json", "cells.npz"] {
                names.push(format!("qc-runs/{id}/{name}"));
            }
        }
    }
    names.sort();
    let entries: Result<Vec<_>, String> = names
        .into_iter()
        .map(|name| {
            let path = stage.join(&name);
            Ok(Entry {
                name,
                bytes: fs::metadata(&path).map_err(err)?.len(),
                sha256: digest(&path)?,
            })
        })
        .collect();
    let index = Index {
        schema_version: 2,
        entries: entries?,
    };
    check_index(&index)?;
    let json = serde_json::to_vec(&index).map_err(err)?;
    if json.len() as u64 > MAX_META {
        return Err("invalid_project".into());
    }
    let mut output =
        tempfile::NamedTempFile::new_in(destination.parent().unwrap_or(Path::new(".")))
            .map_err(err)?;
    output
        .write_all(MAGIC)
        .and_then(|_| output.write_all(&(json.len() as u64).to_le_bytes()))
        .and_then(|_| output.write_all(&json))
        .map_err(err)?;
    for entry in &index.entries {
        let path = stage.join(&entry.name);
        std::io::copy(&mut File::open(&path).map_err(err)?, &mut output).map_err(err)?;
        if digest(&path)? != entry.sha256 {
            return Err("invalid_project".into());
        }
    }
    output.as_file().sync_all().map_err(err)?;
    output.persist_noclobber(destination).map_err(|e| {
        if e.error.kind() == std::io::ErrorKind::AlreadyExists {
            "destination_exists".to_string()
        } else {
            "storage_error".to_string()
        }
    })?;
    Ok(())
}
fn check_index(index: &Index) -> Result<(), String> {
    let mut names = HashSet::new();
    let mut total = 0u64;
    if ![1, 2].contains(&index.schema_version)
        || index.entries.len()
            > if index.schema_version == 1 {
                3002
            } else {
                3 * MAX_RUNS + 3
            }
        || index.entries.len() < 2
    {
        return Err("invalid_project".into());
    }
    for e in &index.entries {
        total = total.checked_add(e.bytes).ok_or("invalid_project")?;
        if !allowed(&e.name)
            || !names.insert(&e.name)
            || e.bytes == 0
            || e.bytes > 4 * 1024 * 1024 * 1024
            || (!e.name.ends_with(".npz") && e.bytes > MAX_META)
            || e.sha256.len() != 64
            || !e.sha256.bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err("invalid_project".into());
        }
    }
    if total > MAX_TOTAL
        || !names.contains(&"document.json".to_string())
        || !names.contains(&"selected.json".to_string())
    {
        return Err("invalid_project".into());
    }
    if (index.schema_version == 2) != names.contains(&"history-state.json".to_string()) {
        return Err("invalid_project".into());
    }
    let ids: HashSet<_> = index
        .entries
        .iter()
        .filter_map(|e| {
            e.name
                .strip_prefix("qc-runs/")
                .and_then(|s| s.split('/').next())
        })
        .collect();
    for id in ids {
        for file in ["report.json", "manifest.json", "cells.npz"] {
            if !names.contains(&format!("qc-runs/{id}/{file}")) {
                return Err("invalid_project".into());
            }
        }
    }
    Ok(())
}
/// Extract only whitelisted names into a fresh directory; never follow package paths or bindings.
pub fn unpack(package: &Path, parent: &Path) -> Result<tempfile::TempDir, String> {
    let stage = tempfile::Builder::new()
        .prefix("project-")
        .tempdir_in(parent)
        .map_err(err)?;
    let mut input = File::open(package).map_err(err)?;
    let mut magic = vec![0; MAGIC.len()];
    input.read_exact(&mut magic).map_err(err)?;
    if magic != MAGIC {
        return Err("invalid_project".into());
    }
    let mut size = [0; 8];
    input.read_exact(&mut size).map_err(err)?;
    let size = u64::from_le_bytes(size);
    if size > MAX_META {
        return Err("invalid_project".into());
    }
    let mut json = vec![0; size as usize];
    input.read_exact(&mut json).map_err(err)?;
    let index: Index = serde_json::from_slice(&json).map_err(err)?;
    check_index(&index)?;
    for entry in index.entries {
        let path = stage.path().join(&entry.name);
        fs::create_dir_all(path.parent().unwrap()).map_err(err)?;
        let mut output = File::create(&path).map_err(err)?;
        let copied =
            std::io::copy(&mut (&mut input).take(entry.bytes), &mut output).map_err(err)?;
        if copied != entry.bytes || digest(&path)? != entry.sha256 {
            return Err("invalid_project".into());
        }
    }
    if input.read(&mut [0; 1]).map_err(err)? != 0 {
        return Err("invalid_project".into());
    }
    validate(stage.path())?;
    let ids = archive_ids(stage.path())?;
    for state in history_states(stage.path(), &ids)? {
        if let Some(time) = state.trashed_at {
            crate::trash::restore_portable_marker(stage.path(), &state.id, time)?;
        }
    }
    // Portable state is materialized into local owner-scoped markers. Do not leave
    // a second, eventually stale classification store in an active project.
    if stage.path().join("history-state.json").exists() {
        fs::remove_file(stage.path().join("history-state.json")).map_err(err)?;
    }
    Ok(stage)
}
fn archive_ids(folder: &Path) -> Result<HashSet<String>, String> {
    match fs::read_dir(folder.join("qc-runs")) {
        Ok(entries) => entries
            .map(|entry| {
                let entry = entry.map_err(err)?;
                let id = entry.file_name().to_string_lossy().to_string();
                if !super::h5ad::safe_id(&id) {
                    return Err("invalid_project".into());
                }
                Ok(id)
            })
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashSet::new()),
        Err(e) => Err(err(e)),
    }
}
pub fn validate(folder: &Path) -> Result<Document, String> {
    let doc: Document =
        serde_json::from_value(read_json(&folder.join("document.json"))?).map_err(err)?;
    let mut store = ResearchStore::in_memory().map_err(err)?;
    store.restore_document(doc.clone(), 0).map_err(err)?;
    let selected = read_json(&folder.join("selected.json"))?;
    if !selected["sourcePath"].is_null() {
        return Err("invalid_project".into());
    }
    let worker = Worker::new(folder);
    let ids = archive_ids(folder)?;
    crate::cohorts::validate_archives(&doc, &worker)?;
    let states = history_states(folder, &ids)?;
    if let Ok(entries) = fs::read_dir(folder.join("qc-runs")) {
        for entry in entries {
            let id = entry
                .map_err(err)?
                .file_name()
                .to_string_lossy()
                .to_string();
            let record = worker.archived_record(&id)?;
            if !record["sourcePath"].is_null() {
                return Err("invalid_project".into());
            }
            let m = read_json(&folder.join("qc-runs").join(&id).join("manifest.json"))?;
            if digest(&folder.join("qc-runs").join(&id).join("cells.npz"))?
                != m["selectionSha256"].as_str().ok_or("invalid_project")?
            {
                return Err("invalid_project".into());
            }
        }
    }
    if selected["report"]["selection"].is_object() {
        let id = selected["report"]["selection"]["id"]
            .as_str()
            .ok_or("invalid_project")?;
        if states.iter().any(|s| s.id == id && s.trashed_at.is_some())
            || worker.archived_record(id)?["report"] != selected["report"]
        {
            return Err("invalid_project".into());
        }
    } else if !selected["report"].is_null() && !super::h5ad::valid_report(&selected["report"]) {
        return Err("invalid_project".into());
    }
    Ok(doc)
}
pub fn active_folder(root: &Path) -> Result<PathBuf, String> {
    let pointer = root.join("active-project.json");
    if !pointer.exists() {
        return Ok(root.to_path_buf());
    }
    let value = read_json(&pointer)?;
    let name = value["folder"].as_str().ok_or("invalid_project")?;
    if name == "." {
        return Ok(root.to_path_buf());
    }
    if !name.starts_with("project-")
        || name.len() > 100
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("invalid_project".into());
    }
    let folder = root.join(name);
    if !folder.is_dir() {
        return Err("invalid_project".into());
    }
    Ok(folder)
}
pub fn activate(root: &Path, folder: &Path) -> Result<(), String> {
    let name = if folder == root {
        "."
    } else {
        folder
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or("invalid_project")?
    };
    let mut tmp = tempfile::NamedTempFile::new_in(root).map_err(err)?;
    tmp.write_all(serde_json::json!({"folder":name}).to_string().as_bytes())
        .map_err(err)?;
    tmp.as_file().sync_all().map_err(err)?;
    tmp.persist(root.join("active-project.json")).map_err(err)?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) active: bool,
    pub(crate) created_at: Option<u64>,
    pub(crate) history_count: usize,
    pub(crate) selected_id: Option<String>,
    pub(crate) retained: Option<u64>,
    pub(crate) excluded: Option<u64>,
    pub(crate) donor_column: Option<String>,
    pub(crate) source_name: Option<String>,
    pub(crate) source_linked: bool,
    pub(crate) source_sha256: Option<String>,
    #[serde(default)]
    pub(crate) trashed: bool,
}
pub fn resolve(root: &Path, id: &str) -> Result<PathBuf, String> {
    let path = resolve_any(root, id)?;
    if crate::trash::project_trashed(&path) {
        return Err("project_trashed".into());
    }
    Ok(path)
}
pub(crate) fn resolve_any(root: &Path, id: &str) -> Result<PathBuf, String> {
    let path = if id == "local" {
        root.to_path_buf()
    } else {
        if !id.starts_with("project-")
            || id.len() > 100
            || !id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("invalid_project".into());
        }
        root.join(id)
    };
    if !path.join("research.sqlite3").is_file()
        || fs::symlink_metadata(&path)
            .map_err(err)?
            .file_type()
            .is_symlink()
    {
        return Err("invalid_project".into());
    }
    Ok(path)
}
pub fn catalog(root: &Path) -> Result<Vec<ProjectSummary>, String> {
    Ok(catalog_all(root)?
        .into_iter()
        .filter(|r| !r.trashed)
        .collect())
}
pub(crate) fn catalog_all(root: &Path) -> Result<Vec<ProjectSummary>, String> {
    let active = active_folder(root)?;
    let mut ids = vec!["local".to_string()];
    for entry in fs::read_dir(root).map_err(err)? {
        let name = entry
            .map_err(err)?
            .file_name()
            .to_string_lossy()
            .to_string();
        if name.starts_with("project-") {
            ids.push(name);
        }
    }
    let mut rows = Vec::new();
    for id in ids {
        let Ok(folder) = resolve_any(root, &id) else {
            continue;
        };
        let Ok(doc) = ResearchStore::inspect(folder.join("research.sqlite3")) else {
            continue;
        };
        let worker = Worker::new(&folder);
        let snapshot = worker.snapshot();
        let report = snapshot.report.unwrap_or(Value::Null);
        let created_at = fs::metadata(folder.join("research.sqlite3"))
            .ok()
            .and_then(|m| m.created().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64);
        rows.push(ProjectSummary {
            trashed: crate::trash::project_trashed(&folder),
            id,
            name: doc.project_name,
            active: folder == active,
            created_at,
            history_count: crate::search_index::history_count(&worker)?,
            selected_id: report["selection"]["id"].as_str().map(str::to_owned),
            retained: report["qc"]["retained"].as_u64(),
            excluded: report["qc"]["excluded"].as_u64(),
            donor_column: report["qc"]["options"]["donorColumn"]
                .as_str()
                .map(str::to_owned),
            source_name: snapshot.source_name,
            source_linked: snapshot.source_linked,
            source_sha256: report["sha256"].as_str().map(str::to_owned),
        });
    }
    rows.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then_with(|| b.created_at.cmp(&a.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(rows)
}
/// Prepare a project without sharing stores, reports or source bindings with another folder.
pub fn prepare_switch(
    root: &Path,
    id: &str,
    minimum_revision: u64,
) -> Result<(ResearchStore, Worker), String> {
    let folder = resolve(root, id)?;
    let doc = ResearchStore::inspect(folder.join("research.sqlite3")).map_err(err)?;
    let mut store = ResearchStore::open(folder.join("research.sqlite3")).map_err(err)?;
    store.restore_document(doc, minimum_revision).map_err(err)?;
    Ok((store, Worker::new(&folder)))
}
pub fn create(root: &Path, name: &str, minimum_revision: u64) -> Result<tempfile::TempDir, String> {
    if name.trim().is_empty() || name.len() > 120 || name.contains('\0') {
        return Err("invalid_project_name".into());
    }
    let folder = tempfile::Builder::new()
        .prefix("project-")
        .tempdir_in(root)
        .map_err(err)?;
    let mut store = ResearchStore::open(folder.path().join("research.sqlite3")).map_err(err)?;
    let doc = Document {
        project_name: name.trim().into(),
        ..Document::default()
    };
    store.restore_document(doc, minimum_revision).map_err(err)?;
    Ok(folder)
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod switching_tests;
