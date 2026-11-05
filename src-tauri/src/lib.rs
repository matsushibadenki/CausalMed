mod browse;
mod h5ad;
mod project;
mod search_index;
mod trash;
use causalmed_core::{Dock, Document, HypothesisStatus, Locale, NewHypothesis, ResearchStore};
use causalmed_renderer::Renderer;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

struct AppState {
    store: Mutex<ResearchStore>,
    h5ad: RwLock<Arc<h5ad::Worker>>,
    project_gate: RwLock<()>,
    root: PathBuf,
    renderer: Mutex<Option<Renderer>>,
}
impl AppState {
    fn worker(&self) -> Arc<h5ad::Worker> {
        self.h5ad.read().unwrap().clone()
    }
    fn switch_to(&self, id: &str) -> Result<u64, String> {
        let _gate = self.project_gate.try_write().map_err(|_| "project_busy")?;
        if ["selecting", "running", "cancelling"]
            .contains(&self.worker().snapshot().status.as_str())
        {
            return Err("worker_busy".into());
        }
        let folder = project::resolve(&self.root, id)?;
        let mut current = self.store.lock().map_err(|_| "state_unavailable")?;
        let (next, next_worker) =
            project::prepare_switch(&self.root, id, current.snapshot().revision)?;
        let revision = next.snapshot().revision;
        let mut worker = self.h5ad.write().map_err(|_| "state_unavailable")?;
        project::activate(&self.root, &folder)?;
        *current = next;
        *worker = Arc::new(next_worker);
        Ok(revision)
    }
}
fn publish_project(app: &tauri::AppHandle, revision: u64) {
    let _ = app.emit("document-changed", revision);
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        for name in ["inspector", "graph"] {
            if let Some(window) = handle.get_webview_window(name) {
                let _ = window.close();
            }
        }
        *handle.state::<AppState>().renderer.lock().unwrap() = None;
    });
}
#[tauri::command]
async fn query_projects(
    app: tauri::AppHandle,
    query: browse::Query,
) -> Result<browse::Page<project::ProjectSummary>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _gate = state.project_gate.try_read().map_err(|_| "project_busy")?;
        browse::projects(&state.root, &state.worker().snapshot().project_key, &query)
    })
    .await
    .map_err(|_| "storage_error")?
}
#[tauri::command]
async fn query_qc_history(
    app: tauri::AppHandle,
    project_key: String,
    query: browse::Query,
) -> Result<browse::Page<browse::HistoryRow>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _gate = state.project_gate.try_read().map_err(|_| "project_busy")?;
        browse::history(&state.worker(), &project_key, &query)
    })
    .await
    .map_err(|_| "storage_error")?
}
#[tauri::command]
async fn set_trash(
    app: tauri::AppHandle,
    project_key: String,
    id: String,
    kind: String,
    restore: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _gate = state.project_gate.try_write().map_err(|_| "project_busy")?;
        let worker = state.worker();
        let snapshot = worker.snapshot();
        if snapshot.project_key != project_key {
            return Err("project_changed".into());
        }
        if ["selecting", "running", "cancelling"].contains(&snapshot.status.as_str()) {
            return Err("worker_busy".into());
        }
        match kind.as_str() {
            "history" => trash::history(&worker, &project_key, &id, restore),
            "project" => trash::project(&state.root, &id, restore),
            _ => Err("trash_unavailable".into()),
        }
    })
    .await
    .map_err(|_| "storage_error")?
}
#[tauri::command]
async fn set_trash_batch(
    app: tauri::AppHandle,
    project_key: String,
    kind: String,
    ids: Vec<String>,
    restore: bool,
) -> Result<trash::BatchResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _gate = state.project_gate.try_write().map_err(|_| "project_busy")?;
        trash::batch(
            &state.root,
            &state.worker(),
            &project_key,
            &kind,
            &ids,
            restore,
        )
    })
    .await
    .map_err(|_| "storage_error")?
}
#[tauri::command]
async fn list_projects(app: tauri::AppHandle) -> Result<Vec<project::ProjectSummary>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _gate = state.project_gate.try_read().map_err(|_| "project_busy")?;
        project::catalog(&state.root)
    })
    .await
    .map_err(|_| "storage_error")?
}
#[tauri::command]
async fn switch_project(app: tauri::AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let revision = app.state::<AppState>().switch_to(&id)?;
        publish_project(&app, revision);
        Ok(())
    })
    .await
    .map_err(|_| "storage_error")?
}
#[tauri::command]
async fn create_project(app: tauri::AppHandle, name: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let gate = state.project_gate.try_write().map_err(|_| "project_busy")?;
        if ["selecting", "running", "cancelling"]
            .contains(&state.worker().snapshot().status.as_str())
        {
            return Err("worker_busy".into());
        }
        let mut current = state.store.lock().map_err(|_| "state_unavailable")?;
        let folder = project::create(&state.root, &name, current.snapshot().revision)?;
        let next = ResearchStore::open(folder.path().join("research.sqlite3"))
            .map_err(|_| "storage_error")?;
        let revision = next.snapshot().revision;
        let next_worker = Arc::new(h5ad::Worker::new(folder.path()));
        let mut worker = state.h5ad.write().map_err(|_| "state_unavailable")?;
        project::activate(&state.root, folder.path())?;
        let _kept = folder.keep();
        *current = next;
        *worker = next_worker;
        drop(worker);
        drop(current);
        drop(gate);
        publish_project(&app, revision);
        Ok(())
    })
    .await
    .map_err(|_| "storage_error")?
}
#[tauri::command]
fn rename_project(app: tauri::AppHandle, state: State<AppState>, name: String) -> Reply {
    mutate(&app, &state, |s| s.rename_project(&name))
}
type Reply = Result<Document, String>;

fn mutate(
    app: &tauri::AppHandle,
    state: &AppState,
    operation: impl FnOnce(&mut ResearchStore) -> causalmed_core::Result<Document>,
) -> Reply {
    let _gate = state.project_gate.try_read().map_err(|_| "project_busy")?;
    let document = operation(&mut *state.store.lock().map_err(|_| "state_unavailable")?)
        .map_err(|e| e.to_string())?;
    // Only a revision notification crosses the event bus; consumers request a snapshot.
    let _ = app.emit("document-changed", document.revision);
    Ok(document)
}
#[tauri::command]
fn get_document(state: State<AppState>) -> Reply {
    let _gate = state.project_gate.try_read().map_err(|_| "project_busy")?;
    Ok(state
        .store
        .lock()
        .map_err(|_| "state_unavailable")?
        .snapshot())
}
#[tauri::command]
fn import_evidence(app: tauri::AppHandle, state: State<AppState>, json: String) -> Reply {
    mutate(&app, &state, |s| s.import_json(&json))
}
#[tauri::command]
fn select_edge(app: tauri::AppHandle, state: State<AppState>, id: String) -> Reply {
    let doc = mutate(&app, &state, |s| s.select_edge(id))?;
    if let Some(edge) = doc
        .edges
        .iter()
        .find(|e| Some(&e.id) == doc.workspace.selected_edge_id.as_ref())
        .cloned()
    {
        let handle = app.clone();
        let title = graph_title(&edge, &doc);
        app.run_on_main_thread(move || {
            let state = handle.state::<AppState>();
            if let Ok(mut renderer) = state.renderer.lock() {
                if let Some(renderer) = renderer.as_mut() {
                    renderer.update(edge.clone());
                    let _ = renderer.render();
                }
            }
            if let Some(window) = handle.get_window("graph") {
                let _ = window.set_title(&title);
            }
        })
        .map_err(|e| e.to_string())?;
    }
    Ok(doc)
}
#[tauri::command]
fn set_locale(app: tauri::AppHandle, state: State<AppState>, locale: Locale) -> Reply {
    let doc = mutate(&app, &state, |s| s.set_locale(locale))?;
    if let (Some(window), Some(edge)) = (
        app.get_window("graph"),
        doc.edges
            .iter()
            .find(|e| Some(&e.id) == doc.workspace.selected_edge_id.as_ref()),
    ) {
        let _ = window.set_title(&graph_title(edge, &doc));
    }
    Ok(doc)
}
#[tauri::command]
fn create_hypothesis(app: tauri::AppHandle, state: State<AppState>, input: NewHypothesis) -> Reply {
    mutate(&app, &state, |s| s.create_hypothesis(input))
}
#[tauri::command]
fn set_hypothesis_status(
    app: tauri::AppHandle,
    state: State<AppState>,
    id: String,
    status: HypothesisStatus,
) -> Reply {
    mutate(&app, &state, |s| s.set_hypothesis_status(&id, status))
}
#[tauri::command]
fn run_benchmark(app: tauri::AppHandle, state: State<AppState>) -> Reply {
    mutate(&app, &state, |s| s.run_synthetic_benchmark())
}

#[tauri::command]
async fn save_project(app: tauri::AppHandle) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _gate = state.project_gate.try_write().map_err(|_| "project_busy")?;
        let worker = state.worker();
        if ["selecting", "running", "cancelling"].contains(&worker.snapshot().status.as_str()) {
            return Err("worker_busy".into());
        }
        let Some(file) = app
            .dialog()
            .file()
            .add_filter("CausalMed", &["causalmed"])
            .set_file_name("causalmed-project.causalmed")
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let path = file.into_path().map_err(|_| "invalid_project")?;
        let doc = state
            .store
            .lock()
            .map_err(|_| "state_unavailable")?
            .snapshot();
        let stage = project::stage_save(&state.root, &doc, &worker)?;
        project::save(stage.path(), &path)?;
        Ok(true)
    })
    .await
    .map_err(|_| "storage_error")?
}
#[tauri::command]
async fn restore_project(app: tauri::AppHandle) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let _gate = state.project_gate.try_write().map_err(|_| "project_busy")?;
        if ["selecting", "running", "cancelling"]
            .contains(&state.worker().snapshot().status.as_str())
        {
            return Err("worker_busy".into());
        }
        let Some(file) = app
            .dialog()
            .file()
            .add_filter("CausalMed", &["causalmed"])
            .blocking_pick_file()
        else {
            return Ok(false);
        };
        let path = file.into_path().map_err(|_| "invalid_project")?;
        let stage = project::unpack(&path, &state.root)?;
        let doc = project::validate(stage.path())?;
        let mut current = state.store.lock().map_err(|_| "state_unavailable")?;
        let mut next = ResearchStore::open(stage.path().join("research.sqlite3"))
            .map_err(|_| "storage_error")?;
        let restored = next
            .restore_document(doc, current.snapshot().revision)
            .map_err(|_| "invalid_project")?;
        std::fs::copy(
            stage.path().join("selected.json"),
            stage.path().join("h5ad-metadata.json"),
        )
        .map_err(|_| "storage_error")?;
        let new_worker = Arc::new(h5ad::Worker::new(stage.path()));
        let mut worker = state.h5ad.write().map_err(|_| "state_unavailable")?;
        project::activate(&state.root, stage.path())?;
        let _folder = stage.keep();
        *current = next;
        *worker = new_worker;
        drop(worker);
        drop(current);
        drop(_gate);
        publish_project(&app, restored.revision);
        Ok(true)
    })
    .await
    .map_err(|_| "storage_error")?
}
#[tauri::command]
async fn relink_h5ad(
    app: tauri::AppHandle,
    selection_id: String,
    timeout_seconds: u64,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let _gate = state.project_gate.try_read().map_err(|_| "project_busy")?;
    let worker = state.worker();
    let sha = worker.relink_sha(&selection_id)?;
    worker.reserve(timeout_seconds)?;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(file) = app
            .dialog()
            .file()
            .add_filter("H5AD", &["h5ad"])
            .blocking_pick_file()
        else {
            worker.finish("cancelled", None);
            return;
        };
        match file.into_path() {
            Ok(path) => worker.run_relink(&h5ad::python_path(), h5ad::worker_script(), &path, &sha),
            Err(_) => worker.finish("failed", Some("invalid_h5ad")),
        }
    });
    Ok(())
}

#[tauri::command]
async fn export_workspace(app: tauri::AppHandle) -> Result<bool, String> {
    let doc = app
        .state::<AppState>()
        .store
        .lock()
        .map_err(|_| "state_unavailable")?
        .snapshot();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(file) = app
            .dialog()
            .file()
            .add_filter("JSON", &["json"])
            .set_file_name("causalmed-workspace.json")
            .blocking_save_file()
        else {
            return Ok(false);
        };
        let path = file.into_path().map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(&doc).map_err(|e| e.to_string())?;
        std::fs::write(path, json).map_err(|e| e.to_string())?;
        Ok(true)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn set_inspector_dock(app: tauri::AppHandle, dock: Dock) -> Reply {
    if dock == Dock::Floating {
        if let Some(window) = app.get_webview_window("inspector") {
            window.set_focus().map_err(|e| e.to_string())?;
        } else {
            tauri::WebviewWindowBuilder::new(
                &app,
                "inspector",
                tauri::WebviewUrl::App("index.html?panel=inspector".into()),
            )
            .title("CausalMed · Inspector")
            .inner_size(420.0, 740.0)
            .min_inner_size(320.0, 400.0)
            .build()
            .map_err(|e| e.to_string())?;
        }
    } else {
        // Publish the destination before closing the invoking view. The destroy
        // handler must not race a bottom-dock request back to the default right.
        let document = mutate(&app, &app.state::<AppState>(), |s| s.set_dock(dock))?;
        if let Some(window) = app.get_webview_window("inspector") {
            window.close().map_err(|e| e.to_string())?;
        }
        if let Some(main) = app.get_webview_window("main") {
            main.show().map_err(|e| e.to_string())?;
            main.set_focus().map_err(|e| e.to_string())?;
        }
        return Ok(document);
    }
    mutate(&app, &app.state::<AppState>(), |s| s.set_dock(dock))
}

fn graph_title(edge: &causalmed_core::Edge, doc: &Document) -> String {
    use causalmed_core::{EvidenceKind, Relation};
    let symbol = match edge.relation {
        Relation::Activates => "→",
        Relation::Inhibits => "⊣",
        Relation::Associated => "~",
    };
    let labels = match doc.workspace.locale {
        Locale::En => [
            "Observation",
            "Intervention",
            "Hypothesis",
            "Synthetic data",
            "Unverified source",
        ],
        Locale::Ja => ["観測", "介入実験", "仮説", "合成データ", "出典未検証"],
        Locale::Zh => ["观察", "干预实验", "假设", "合成数据", "来源未验证"],
    };
    let evidence = labels[match edge.evidence {
        EvidenceKind::Observation => 0,
        EvidenceKind::Intervention => 1,
        EvidenceKind::Hypothesis => 2,
    }];
    let provenance = labels[if doc
        .sources
        .iter()
        .any(|s| s.id == edge.source_id && s.synthetic)
    {
        3
    } else {
        4
    }];
    format!(
        "{} {symbol} {} · {evidence} · {provenance} · CausalMed",
        edge.source, edge.target
    )
}

#[tauri::command]
async fn open_graph(app: tauri::AppHandle) -> Result<(), String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let result = (|| {
            if let Some(window) = handle.get_window("graph") {
                return window.set_focus().map_err(|e| e.to_string());
            }
            let state = handle.state::<AppState>();
            let doc = state
                .store
                .lock()
                .map_err(|_| "state_unavailable")?
                .snapshot();
            let edge = doc
                .edges
                .iter()
                .find(|e| Some(&e.id) == doc.workspace.selected_edge_id.as_ref())
                .cloned()
                .ok_or("edge_not_found")?;
            let window = tauri::window::WindowBuilder::new(&handle, "graph")
                .title(graph_title(&edge, &doc))
                .inner_size(720.0, 400.0)
                .build()
                .map_err(|e| e.to_string())?;
            let size = window.inner_size().map_err(|e| e.to_string())?;
            let renderer = pollster::block_on(Renderer::new(
                Arc::new(window.clone()),
                size.width,
                size.height,
                edge,
            ));
            match renderer {
                Ok(mut renderer) => {
                    renderer.render()?;
                    *state.renderer.lock().map_err(|_| "renderer_unavailable")? = Some(renderer);
                    Ok(())
                }
                Err(error) => {
                    let _ = window.close();
                    Err(error)
                }
            }
        })();
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}

#[tauri::command]
async fn import_h5ad(app: tauri::AppHandle, timeout_seconds: u64) -> Result<(), String> {
    let state = app.state::<AppState>();
    let _gate = state.project_gate.try_read().map_err(|_| "project_busy")?;
    state.worker().reserve(timeout_seconds)?;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let Some(file) = app
            .dialog()
            .file()
            .add_filter("H5AD", &["h5ad"])
            .blocking_pick_file()
        else {
            state.worker().finish("cancelled", None);
            return;
        };
        match file.into_path() {
            Ok(path) => state
                .worker()
                .run(&h5ad::python_path(), h5ad::worker_script(), &path),
            Err(_) => state.worker().finish("failed", Some("invalid_h5ad")),
        }
    });
    Ok(())
}
#[tauri::command]
async fn analyze_h5ad(
    app: tauri::AppHandle,
    options: serde_json::Value,
    sha256: String,
    timeout_seconds: u64,
) -> Result<(), String> {
    if options.to_string().len() > 4096 || !options.is_object() {
        return Err("invalid_qc_options".into());
    }
    let state = app.state::<AppState>();
    let _gate = state.project_gate.try_read().map_err(|_| "project_busy")?;
    let path = state.worker().analysis_source(&sha256)?;
    state.worker().reserve(timeout_seconds)?;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().worker().run_analysis(
            &h5ad::python_path(),
            h5ad::worker_script(),
            &path,
            &options,
            &sha256,
        );
    });
    Ok(())
}
#[tauri::command]
async fn export_h5ad_selection(
    app: tauri::AppHandle,
    selection_id: String,
    kind: String,
    timeout_seconds: u64,
) -> Result<(), String> {
    if !["export_h5ad", "export_qc"].contains(&kind.as_str()) {
        return Err("invalid_export".into());
    }
    let state = app.state::<AppState>();
    let _gate = state.project_gate.try_read().map_err(|_| "project_busy")?;
    let source = state.worker().export_source(&selection_id)?;
    state.worker().reserve(timeout_seconds)?;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let (extension, name) = if kind == "export_h5ad" {
            ("h5ad", "causalmed-retained.h5ad")
        } else {
            ("zip", "causalmed-qc-results.zip")
        };
        let Some(file) = app
            .dialog()
            .file()
            .add_filter(extension, &[extension])
            .set_file_name(name)
            .blocking_save_file()
        else {
            state.worker().finish("cancelled", None);
            return;
        };
        match file.into_path() {
            Ok(path) => state.worker().run_export(
                &h5ad::python_path(),
                h5ad::worker_script(),
                source,
                &path,
                &kind,
            ),
            Err(_) => state.worker().finish("failed", Some("invalid_export")),
        }
    });
    Ok(())
}
#[tauri::command]
fn get_qc_history(state: State<AppState>) -> Vec<serde_json::Value> {
    state.worker().history()
}
#[tauri::command]
fn select_qc_history(
    state: State<AppState>,
    id: String,
    project_key: String,
) -> Result<(), String> {
    let _gate = state.project_gate.try_read().map_err(|_| "project_busy")?;
    let worker = state.worker();
    if worker.snapshot().project_key != project_key {
        return Err("project_changed".into());
    }
    worker.select_history(&id)
}
#[tauri::command]
async fn compare_qc_history(
    app: tauri::AppHandle,
    left: String,
    right: String,
    project_key: String,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let _gate = state.project_gate.try_read().map_err(|_| "project_busy")?;
    let worker = state.worker();
    if worker.snapshot().project_key != project_key {
        return Err("project_changed".into());
    }
    let a = worker.export_source(&left)?;
    let b = state.worker().export_source(&right)?;
    state.worker().reserve(120)?;
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<AppState>().worker().compare_history(
            &h5ad::python_path(),
            h5ad::worker_script(),
            a,
            b,
        )
    });
    Ok(())
}
#[tauri::command]
fn get_h5ad_state(state: State<AppState>) -> h5ad::Snapshot {
    state.worker().snapshot()
}
#[tauri::command]
fn cancel_h5ad(state: State<AppState>) {
    state.worker().cancel();
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let folder = app.path().app_data_dir()?;
            std::fs::create_dir_all(&folder)?;
            let active = project::active_folder(&folder)?;
            let mut store = ResearchStore::open(active.join("research.sqlite3"))?;
            // A floating view never owns the document. Restore it docked on restart.
            if store.snapshot().workspace.inspector_dock == Dock::Floating {
                store.set_dock(Dock::Right)?;
            }
            app.manage(AppState {
                store: Mutex::new(store),
                renderer: Mutex::new(None),
                h5ad: RwLock::new(Arc::new(h5ad::Worker::new(&active))),
                project_gate: RwLock::new(()),
                root: folder,
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            let state = window.state::<AppState>();
            if window.label() == "inspector" && matches!(event, tauri::WindowEvent::Destroyed) {
                if let Ok(mut store) = state.store.lock() {
                    if store.snapshot().workspace.inspector_dock == Dock::Floating {
                        if let Ok(doc) = store.set_dock(Dock::Right) {
                            let _ = window.emit("document-changed", doc.revision);
                        }
                    }
                }
            }
            if window.label() == "graph" {
                if let Ok(mut renderer) = state.renderer.lock() {
                    match event {
                        tauri::WindowEvent::Destroyed => {
                            *renderer = None;
                        }
                        tauri::WindowEvent::Resized(size) => {
                            if let Some(r) = renderer.as_mut() {
                                r.resize(size.width, size.height);
                                let _ = r.render();
                            }
                        }
                        tauri::WindowEvent::Focused(true) => {
                            if let Some(r) = renderer.as_mut() {
                                let _ = r.render();
                            }
                        }
                        _ => {}
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_document,
            import_h5ad,
            get_h5ad_state,
            get_qc_history,
            select_qc_history,
            compare_qc_history,
            analyze_h5ad,
            export_h5ad_selection,
            cancel_h5ad,
            import_evidence,
            select_edge,
            set_locale,
            create_hypothesis,
            set_hypothesis_status,
            run_benchmark,
            set_inspector_dock,
            open_graph,
            export_workspace,
            set_trash,
            set_trash_batch,
            query_projects,
            query_qc_history,
            list_projects,
            switch_project,
            create_project,
            rename_project,
            save_project,
            restore_project,
            relink_h5ad
        ])
        .build(tauri::generate_context!())
        .expect("CausalMed failed to start")
        .run(|app, event| {
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                app.state::<AppState>().worker().shutdown();
            }
        });
}
