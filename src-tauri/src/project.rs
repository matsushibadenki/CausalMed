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
mod tests {
    use super::*;
    #[test]
    fn package_roundtrip_and_corruption_leave_active_project_untouched() {
        let root = tempfile::tempdir().unwrap();
        let worker = Worker::new(root.path());
        let mut store = ResearchStore::in_memory().unwrap();
        let doc = store.snapshot();
        let stage = stage_save(root.path(), &doc, &worker).unwrap();
        let package = root.path().join("saved.causalmed");
        save(stage.path(), &package).unwrap();
        assert!(save(stage.path(), &package).is_err());
        let extracted = unpack(&package, root.path()).unwrap();
        assert_eq!(
            validate(extracted.path()).unwrap().edges.len(),
            doc.edges.len()
        );
        let changed = root.path().join("corrupt.causalmed");
        let mut bytes = fs::read(&package).unwrap();
        let end = bytes.len() - 1;
        bytes[end] ^= 1;
        fs::write(&changed, &bytes).unwrap();
        assert!(unpack(&changed, root.path()).is_err());
        assert!(!root.path().join("active-project.json").exists());
        bytes = fs::read(&package).unwrap();
        bytes.push(0);
        fs::write(&changed, &bytes).unwrap();
        assert!(unpack(&changed, root.path()).is_err());
        let mut invalid = doc.clone();
        invalid.workspace.selected_edge_id = Some("missing".into());
        assert!(store.restore_document(invalid, 100).is_err());
        assert_eq!(store.snapshot().revision, doc.revision);
        let recovered = store.restore_document(doc, 100).unwrap();
        assert_eq!(recovered.revision, 101);
    }
    #[test]
    fn archive_names_sizes_duplicates_and_versions_are_bounded() {
        let entry = |name: &str| Entry {
            name: name.into(),
            bytes: 10,
            sha256: "a".repeat(64),
        };
        for names in [
            vec!["document.json", "../escape"],
            vec!["document.json", "document.json"],
            vec!["document.json", "/selected.json"],
            vec!["document.json", "qc-runs/../../cells.npz"],
        ] {
            assert!(check_index(&Index {
                schema_version: 1,
                entries: names.iter().map(|n| entry(n)).collect()
            })
            .is_err());
        }
        assert!(check_index(&Index {
            schema_version: 2,
            entries: vec![entry("document.json"), entry("selected.json")]
        })
        .is_err());
        let mut big = entry("selected.json");
        big.bytes = MAX_META + 1;
        assert!(check_index(&Index {
            schema_version: 1,
            entries: vec![entry("document.json"), big]
        })
        .is_err());
    }
    #[test]
    fn legacy_packages_remain_readable_and_state_references_are_validated() {
        let root = tempfile::tempdir().unwrap();
        let worker = Worker::new(root.path());
        let doc = ResearchStore::in_memory().unwrap().snapshot();
        let id = crate::search_index::tests::fixture(root.path(), 0);
        let stage = stage_save(root.path(), &doc, &worker).unwrap();
        let names = vec![
            "document.json".to_string(),
            "selected.json".into(),
            format!("qc-runs/{id}/report.json"),
            format!("qc-runs/{id}/manifest.json"),
            format!("qc-runs/{id}/cells.npz"),
        ];
        let index = Index {
            schema_version: 1,
            entries: names
                .iter()
                .map(|name| Entry {
                    name: name.clone(),
                    bytes: fs::metadata(stage.path().join(name)).unwrap().len(),
                    sha256: digest(&stage.path().join(name)).unwrap(),
                })
                .collect(),
        };
        let bytes = serde_json::to_vec(&index).unwrap();
        let path = root.path().join("legacy.causalmed");
        let mut out = File::create(&path).unwrap();
        out.write_all(MAGIC).unwrap();
        out.write_all(&(bytes.len() as u64).to_le_bytes()).unwrap();
        out.write_all(&bytes).unwrap();
        for name in names {
            out.write_all(&fs::read(stage.path().join(name)).unwrap())
                .unwrap();
        }
        drop(out);
        let old = unpack(&path, root.path()).unwrap();
        assert_eq!(Worker::new(old.path()).history().len(), 1);
        assert_eq!(validate(old.path()).unwrap().project_name, doc.project_name);
        let bad = serde_json::json!({"schemaVersion":1,"histories":[{"id":"00000000-0000-4000-8000-00000000ffff","trashedAt":1}]});
        fs::write(stage.path().join("history-state.json"), bad.to_string()).unwrap();
        assert!(validate(stage.path()).is_err());
        let bad_package = root.path().join("bad-state.causalmed");
        save(stage.path(), &bad_package).unwrap();
        assert!(unpack(&bad_package, root.path()).is_err());
        fs::write(
            stage.path().join("history-state.json"),
            serde_json::json!({"schemaVersion":9,"histories":[]}).to_string(),
        )
        .unwrap();
        assert!(validate(stage.path()).is_err());
        assert!(!root.path().join("active-project.json").exists());
    }
    #[test]
    fn ten_thousand_mixed_histories_package_scope_and_resave() {
        use crate::{
            browse::{self, Query},
            search_index, trash,
        };
        let root = tempfile::tempdir().unwrap();
        let a = create(root.path(), "Synthetic package load A", 0).unwrap();
        for i in 0..10_000 {
            search_index::tests::fixture(a.path(), i);
        }
        let aw = Worker::new(a.path());
        let scope = aw.snapshot().project_key;
        let selected = "00000000-0000-4000-8000-00000000270f";
        aw.select_history(selected).unwrap();
        for i in 0..100 {
            trash::history(
                &aw,
                &scope,
                &format!("00000000-0000-4000-8000-{i:012x}"),
                false,
            )
            .unwrap();
        }
        let timestamp =
            trash::history_timestamp(a.path(), "00000000-0000-4000-8000-000000000000", &scope)
                .unwrap();
        assert_eq!(
            browse::history(&aw, &scope, &Query::default())
                .unwrap()
                .total,
            9900
        );
        let doc = ResearchStore::inspect(a.path().join("research.sqlite3")).unwrap();
        let start = std::time::Instant::now();
        let stage = stage_save(a.path(), &doc, &aw).unwrap();
        let package = root.path().join("ten-thousand.causalmed");
        save(stage.path(), &package).unwrap();
        let restored = unpack(&package, root.path()).unwrap();
        assert!(!restored.path().join("history-search.sqlite3").exists());
        assert!(!restored.path().join("history-state.json").exists());
        fs::copy(
            restored.path().join("selected.json"),
            restored.path().join("h5ad-metadata.json"),
        )
        .unwrap();
        let rw = Worker::new(restored.path());
        let new_scope = rw.snapshot().project_key;
        assert_ne!(scope, new_scope);
        assert!(browse::history(&rw, &scope, &Query::default()).is_err());
        let q = Query {
            trash: true,
            search: "REPLICATE 200".into(),
            page_size: 25,
            page: 2,
            ..Default::default()
        };
        let trash_page = browse::history(&rw, &new_scope, &q).unwrap();
        assert_eq!(trash_page.total, 50);
        assert_eq!(trash_page.items.len(), 25);
        assert_eq!(
            browse::history(&rw, &new_scope, &Query::default())
                .unwrap()
                .total,
            9900
        );
        assert_eq!(rw.snapshot().report.unwrap()["selection"]["id"], selected);
        let id = "00000000-0000-4000-8000-000000000000";
        assert_eq!(
            trash::history_timestamp(restored.path(), id, &new_scope).unwrap(),
            timestamp
        );
        assert!(trash::history(&rw, &scope, id, true).is_err());
        trash::history(&rw, &new_scope, id, true).unwrap();
        assert_eq!(
            browse::history(
                &rw,
                &new_scope,
                &Query {
                    trash: true,
                    ..Default::default()
                }
            )
            .unwrap()
            .total,
            99
        );
        assert_eq!(
            browse::history(
                &aw,
                &scope,
                &Query {
                    trash: true,
                    ..Default::default()
                }
            )
            .unwrap()
            .total,
            100
        );
        rw.select_history(id).unwrap();
        let stage_again = stage_save(restored.path(), &doc, &rw).unwrap();
        let package_again = root.path().join("second-generation.causalmed");
        save(stage_again.path(), &package_again).unwrap();
        let again = unpack(&package_again, root.path()).unwrap();
        fs::copy(
            again.path().join("selected.json"),
            again.path().join("h5ad-metadata.json"),
        )
        .unwrap();
        let restarted = Worker::new(again.path());
        let k = restarted.snapshot().project_key;
        assert_eq!(
            browse::history(&restarted, &k, &Query::default())
                .unwrap()
                .total,
            9901
        );
        assert_eq!(
            browse::history(
                &restarted,
                &k,
                &Query {
                    trash: true,
                    ..Default::default()
                }
            )
            .unwrap()
            .total,
            99
        );
        assert_eq!(restarted.snapshot().report.unwrap()["selection"]["id"], id);
        // A selected trash reference and duplicated state entries must fail validation.
        let raw: HistoryStates = serde_json::from_value(
            read_json(&stage_again.path().join("history-state.json")).unwrap(),
        )
        .unwrap();
        let mut bad = raw.histories;
        bad.iter_mut().find(|s| s.id == id).unwrap().trashed_at = Some(1);
        fs::write(
            stage_again.path().join("history-state.json"),
            serde_json::to_vec(&HistoryStates {
                schema_version: 1,
                histories: bad.clone(),
            })
            .unwrap(),
        )
        .unwrap();
        assert!(validate(stage_again.path()).is_err());
        bad[0].trashed_at = None;
        bad.push(bad[0].clone());
        fs::write(
            stage_again.path().join("history-state.json"),
            serde_json::to_vec(&HistoryStates {
                schema_version: 1,
                histories: bad,
            })
            .unwrap(),
        )
        .unwrap();
        assert!(validate(stage_again.path()).is_err());
        eprintln!(
            "PORTABLE_TRASH_LOAD runs=10000 trash=100 two_generations_ms={} package_bytes={}",
            start.elapsed().as_millis(),
            fs::metadata(package).unwrap().len()
        );
    }
    #[test]
    #[ignore = "requires CAUSALMED_TEST_DONOR_H5AD and anndata"]
    fn real_project_restore_relink_and_historical_export() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("original.h5ad");
        fs::copy(
            std::env::var("CAUSALMED_TEST_DONOR_H5AD").unwrap(),
            &original,
        )
        .unwrap();
        let worker = Worker::new(root.path());
        worker.reserve(120).unwrap();
        worker.run_analysis(
            &crate::h5ad::python_path(),
            crate::h5ad::worker_script(),
            &original,
            &serde_json::json!({"donorColumn":"replicate"}),
            "",
        );
        let first = worker.snapshot().report.unwrap();
        let id = first["selection"]["id"].as_str().unwrap();
        let scope = worker.snapshot().project_key;
        ResearchStore::open(root.path().join("research.sqlite3")).unwrap();
        let project_rows =
            crate::browse::projects(root.path(), &scope, &Default::default()).unwrap();
        assert_eq!(project_rows.items[0].history_count, 1);
        let indexed = crate::browse::history(&worker, &scope, &Default::default()).unwrap();
        assert_eq!(indexed.total, 1);
        assert!(root.path().join("history-search.sqlite3").exists());
        worker.reserve(120).unwrap();
        worker.run_analysis(
            &crate::h5ad::python_path(),
            crate::h5ad::worker_script(),
            &original,
            &serde_json::json!({"donorColumn":"replicate","minGenes":300}),
            first["sha256"].as_str().unwrap(),
        );
        assert_eq!(worker.history().len(), 2);
        let indexed = crate::browse::history(
            &worker,
            &scope,
            &crate::browse::Query {
                search: "replicate 300".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(indexed.total, 1);
        assert_eq!(indexed.unfiltered_total, 2);
        assert_eq!(indexed.items[0].retained, 24297);
        let project_rows =
            crate::browse::projects(root.path(), &scope, &Default::default()).unwrap();
        assert_eq!(project_rows.items[0].history_count, 2);
        worker.select_history(id).unwrap();
        let selected = crate::browse::history(
            &worker,
            &scope,
            &crate::browse::Query {
                filter: "selected".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(selected.items[0].id, id);
        let mut store = ResearchStore::open(root.path().join("research.sqlite3")).unwrap();
        let registered = crate::datasets::register(
            &mut store,
            &worker,
            &scope,
            first["sha256"].as_str().unwrap(),
        )
        .unwrap();
        let record = &registered.datasets[0];
        assert!(record
            .observation
            .annotation_schema
            .iter()
            .any(|c| c.name == "replicate" && c.kind != "unknown"));
        let mut metadata = record.metadata.clone();
        metadata.donor_column = "replicate".into();
        metadata.notes = "Public Kang source; provenance/license not yet reviewed".into();
        let doc = store
            .update_dataset(
                first["sha256"].as_str().unwrap(),
                metadata.clone(),
                registered.revision,
            )
            .unwrap();
        let counts = crate::search_index::dataset_counts(
            &worker,
            &[first["sha256"].as_str().unwrap().into(), "0".repeat(64)],
        )
        .unwrap();
        assert_eq!(counts[first["sha256"].as_str().unwrap()], [2, 0]);
        assert_eq!(
            crate::browse::history(
                &worker,
                &scope,
                &crate::browse::Query {
                    source_sha256: "0".repeat(64),
                    ..Default::default()
                }
            )
            .unwrap()
            .total,
            0
        );

        let stage = stage_save(root.path(), &doc, &worker).unwrap();
        let package = root.path().join("real.causalmed");
        save(stage.path(), &package).unwrap();
        let restored = unpack(&package, root.path()).unwrap();
        let restored_doc = validate(restored.path()).unwrap();
        assert_eq!(restored_doc.datasets.len(), 1);
        assert_eq!(restored_doc.datasets[0].metadata, metadata);

        fs::copy(
            restored.path().join("selected.json"),
            restored.path().join("h5ad-metadata.json"),
        )
        .unwrap();
        assert!(!restored.path().join("history-search.sqlite3").exists());
        assert!(!restored.path().join("project-search.sqlite3").exists());
        let restored_worker = Worker::new(restored.path());
        let restored_scope = restored_worker.snapshot().project_key;
        assert!(crate::browse::history(&restored_worker, &scope, &Default::default()).is_err());
        let indexed = crate::browse::history(
            &restored_worker,
            &restored_scope,
            &crate::browse::Query {
                search: "REPLICATE 300".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(indexed.total, 1);
        assert_eq!(indexed.unfiltered_total, 2);
        assert_eq!(indexed.items[0].retained, 24297);
        assert_eq!(restored_worker.history().len(), 2);
        assert!(!restored_worker.snapshot().source_linked);
        assert_eq!(
            restored_worker.snapshot().report.unwrap()["selection"]["id"],
            id
        );
        let moved = root.path().join("moved-original.h5ad");
        fs::rename(&original, &moved).unwrap();
        assert!(!original.exists());
        let wrong = root.path().join("wrong.h5ad");
        fs::write(&wrong, b"not the original").unwrap();
        let sha = restored_worker.relink_sha(id).unwrap();
        restored_worker.reserve(120).unwrap();
        restored_worker.run_relink(
            &crate::h5ad::python_path(),
            crate::h5ad::worker_script(),
            &wrong,
            &sha,
        );
        assert_eq!(
            restored_worker.snapshot().error.as_deref(),
            Some("file_changed")
        );
        assert!(!restored_worker.snapshot().source_linked);
        restored_worker.reserve(120).unwrap();
        restored_worker.run_relink(
            &crate::h5ad::python_path(),
            crate::h5ad::worker_script(),
            &moved,
            &sha,
        );
        assert!(restored_worker.snapshot().source_linked);
        restored_worker.reserve(120).unwrap();
        restored_worker.run_relink(
            &crate::h5ad::python_path(),
            crate::h5ad::worker_script(),
            &wrong,
            &sha,
        );
        assert_eq!(
            restored_worker.snapshot().error.as_deref(),
            Some("file_changed")
        );
        assert!(restored_worker.snapshot().source_linked);
        assert_eq!(restored_worker.export_source(id).unwrap().source, moved);
        let restarted = Worker::new(restored.path());
        assert!(restarted.snapshot().source_linked);
        for run in restarted.history() {
            assert_eq!(
                restarted
                    .export_source(run["selection"]["id"].as_str().unwrap())
                    .unwrap()
                    .source,
                moved
            );
        }
        let output = root.path().join("past.h5ad");
        let source = restarted.export_source(id).unwrap();
        restarted.reserve(120).unwrap();
        restarted.run_export(
            &crate::h5ad::python_path(),
            crate::h5ad::worker_script(),
            source,
            &output,
            "export_h5ad",
        );
        assert_eq!(restarted.snapshot().status, "completed");
        assert_eq!(restarted.snapshot().last_export.unwrap()["retained"], 24562);
        assert_eq!(digest(&moved).unwrap(), sha);
        let verify=std::process::Command::new(crate::h5ad::python_path()).args(["-c","import anndata as a,numpy as n,json,sys; s=a.read_h5ad(sys.argv[1]); o=a.read_h5ad(sys.argv[2]); z=n.load(sys.argv[3],allow_pickle=False); m=z['retained']; assert o.shape==(24562,15706); assert (o.X!=s.X[m]).nnz==0; assert list(o.obs_names)==list(s.obs_names[m]); assert list(o.obs['replicate'])==list(s.obs['replicate'].iloc[n.flatnonzero(m)]); assert o.obs['replicate'].nunique()==8"]).arg(&moved).arg(&output).arg(restored.path().join("qc-runs").join(id).join("cells.npz")).output().unwrap();
        assert!(
            verify.status.success(),
            "{}",
            String::from_utf8_lossy(&verify.stderr)
        );
        let mut restored_store =
            ResearchStore::open(restored.path().join("research.sqlite3")).unwrap();
        restored_store.restore_document(doc, 0).unwrap();
        activate(root.path(), restored.path()).unwrap();
        assert_eq!(active_folder(root.path()).unwrap(), restored.path());
        let active_query = crate::browse::Query {
            filter: "active".into(),
            ..Default::default()
        };
        let rows = crate::browse::projects(root.path(), &restored_scope, &active_query).unwrap();
        assert_eq!(rows.unfiltered_total, 2);
        assert_eq!(rows.total, 1);
        assert_eq!(rows.items[0].id, restored_scope);
        assert_eq!(rows.items[0].history_count, 2);
        assert_eq!(rows.items[0].selected_id.as_deref(), Some(id));
    }
}

#[cfg(test)]
mod switching_tests {
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
        app.worker().reserve(120).unwrap();
        assert_eq!(app.switch_to(&id).unwrap_err(), "worker_busy");
        app.worker().finish("cancelled", None);
        app.switch_to(&id).unwrap();
        let doc = app.store.lock().unwrap().snapshot();
        assert_eq!(doc.project_name, "Empty B");
        assert!(doc.edges.is_empty());
        assert!(app.worker().snapshot().report.is_none());
        assert!(!app.worker().snapshot().source_linked);
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
            let worker = app.worker();
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
            &app.worker(),
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
        assert!(crate::browse::history(&app.worker(), &bid, &Default::default()).is_err());
        app.switch_to(&bid).unwrap();
        let bq = crate::browse::history(
            &app.worker(),
            &bid,
            &crate::browse::Query {
                search: "replicate".into(),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(bq.total, 0);
        let bq = crate::browse::history(
            &app.worker(),
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
            let worker = app.worker();
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
}
