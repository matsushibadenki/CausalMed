mod h5ad;
use causalmed_core::{Dock, Document, HypothesisStatus, Locale, NewHypothesis, ResearchStore};
use causalmed_renderer::Renderer;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

struct AppState {
    store: Mutex<ResearchStore>,
    h5ad: h5ad::Worker,
    renderer: Mutex<Option<Renderer>>,
}
type Reply = Result<Document, String>;

fn mutate(
    app: &tauri::AppHandle,
    state: &AppState,
    operation: impl FnOnce(&mut ResearchStore) -> causalmed_core::Result<Document>,
) -> Reply {
    let document = operation(&mut *state.store.lock().map_err(|_| "state_unavailable")?)
        .map_err(|e| e.to_string())?;
    // Only a revision notification crosses the event bus; consumers request a snapshot.
    let _ = app.emit("document-changed", document.revision);
    Ok(document)
}
#[tauri::command]
fn get_document(state: State<AppState>) -> Reply {
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
    app.state::<AppState>().h5ad.reserve(timeout_seconds)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let Some(file) = app
            .dialog()
            .file()
            .add_filter("H5AD", &["h5ad"])
            .blocking_pick_file()
        else {
            state.h5ad.finish("cancelled", None);
            return;
        };
        match file.into_path() {
            Ok(path) => state.h5ad.run(
                &h5ad::python_path(),
                include_str!("../../python/causalmed_worker/h5ad.py"),
                &path,
            ),
            Err(_) => state.h5ad.finish("failed", Some("invalid_h5ad")),
        }
    });
    Ok(())
}
#[tauri::command]
fn get_h5ad_state(state: State<AppState>) -> h5ad::Snapshot {
    state.h5ad.snapshot()
}
#[tauri::command]
fn cancel_h5ad(state: State<AppState>) {
    state.h5ad.cancel();
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let folder = app.path().app_data_dir()?;
            std::fs::create_dir_all(&folder)?;
            let mut store = ResearchStore::open(folder.join("research.sqlite3"))?;
            // A floating view never owns the document. Restore it docked on restart.
            if store.snapshot().workspace.inspector_dock == Dock::Floating {
                store.set_dock(Dock::Right)?;
            }
            app.manage(AppState {
                store: Mutex::new(store),
                renderer: Mutex::new(None),
                h5ad: h5ad::Worker::new(&folder),
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
            cancel_h5ad,
            import_evidence,
            select_edge,
            set_locale,
            create_hypothesis,
            set_hypothesis_status,
            run_benchmark,
            set_inspector_dock,
            open_graph,
            export_workspace
        ])
        .build(tauri::generate_context!())
        .expect("CausalMed failed to start")
        .run(|app, event| {
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                app.state::<AppState>().h5ad.shutdown();
            }
        });
}
