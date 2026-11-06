//! Distribution builds never fall back to a developer or system Python.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
pub const UNAVAILABLE: &str = "/__causalmed_python_runtime_unavailable__";
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub kind: String,
    pub python_version: Option<String>,
    pub anndata_version: Option<String>,
    pub error: Option<String>,
}
#[derive(Clone)]
pub struct Runtime {
    pub python: PathBuf,
    pub info: Info,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    target: String,
    #[serde(rename = "minimumMacOS")]
    minimum_mac_os: String,
    python_version: String,
    anndata_version: String,
    archive_sha256: String,
    dependency_lock_sha256: String,
    python_sha256: String,
    inventory_sha256: String,
    build_recipe_sha256: String,
}
fn sha(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn unavailable(code: &str) -> Runtime {
    Runtime {
        python: UNAVAILABLE.into(),
        info: Info {
            kind: "unavailable".into(),
            python_version: None,
            anndata_version: None,
            error: Some(code.into()),
        },
    }
}
fn bundled(folder: &Path, kind: &str) -> Runtime {
    let python = folder.join("python/bin/python3.12");
    let manifest = folder.join("runtime.json");
    if !python.is_file() || !manifest.is_file() {
        return unavailable("bundled_runtime_missing");
    }
    let read = (|| -> Option<Manifest> {
        if std::fs::metadata(&manifest).ok()?.len() > 65536 {
            return None;
        }
        serde_json::from_slice(&std::fs::read(manifest).ok()?).ok()
    })();
    let Some(m) = read else {
        return unavailable("bundled_runtime_invalid");
    };
    let pin: serde_json::Value = serde_json::from_str(include_str!(
        "../../python/runtime/aarch64-apple-darwin.json"
    ))
    .expect("runtime pin");
    let valid = cfg!(all(target_os = "macos", target_arch = "aarch64"))
        && m.schema_version == 1
        && m.target == "aarch64-apple-darwin"
        && m.python_version == pin["pythonVersion"].as_str().unwrap_or("")
        && m.archive_sha256 == pin["sha256"].as_str().unwrap_or("")
        && m.minimum_mac_os == "14.0"
        && m.anndata_version == "0.12.19"
        && m.dependency_lock_sha256
            == sha(include_bytes!(
                "../../python/runtime/wheels-aarch64-apple-darwin.json"
            ))
        && m.build_recipe_sha256 == sha(include_bytes!("../../scripts/bundle_worker.py"))
        && m.inventory_sha256.len() == 64
        && std::fs::read(&python).is_ok_and(|bytes| sha(&bytes) == m.python_sha256);
    if !valid {
        return unavailable("bundled_runtime_invalid");
    }
    Runtime {
        python,
        info: Info {
            kind: kind.into(),
            python_version: Some(m.python_version),
            anndata_version: Some(m.anndata_version),
            error: None,
        },
    }
}
pub(crate) fn resolve(
    exe: &Path,
    development: bool,
    override_path: Option<PathBuf>,
    staged: &Path,
    venv: &Path,
) -> Runtime {
    // Even a debug .app is a distribution: missing resources must remain visible.
    if let Some(contents) = exe
        .parent()
        .filter(|p| p.file_name().is_some_and(|n| n == "MacOS"))
        .and_then(Path::parent)
        .filter(|p| p.file_name().is_some_and(|n| n == "Contents"))
    {
        return bundled(&contents.join("Resources/python-worker"), "bundled");
    }
    if development {
        if let Some(p) = override_path {
            return if p.is_file() {
                Runtime {
                    python: p,
                    info: Info {
                        kind: "development".into(),
                        python_version: None,
                        anndata_version: None,
                        error: None,
                    },
                }
            } else {
                unavailable("development_runtime_missing")
            };
        }
        if staged.exists() {
            return bundled(staged, "staged");
        }
        if venv.is_file() {
            return Runtime {
                python: venv.into(),
                info: Info {
                    kind: "development".into(),
                    python_version: None,
                    anndata_version: None,
                    error: None,
                },
            };
        }
    }
    unavailable("bundled_runtime_missing")
}
pub fn current() -> Runtime {
    static SELECTED: std::sync::OnceLock<Runtime> = std::sync::OnceLock::new();
    SELECTED
        .get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"));
            resolve(
                &std::env::current_exe().unwrap_or_default(),
                cfg!(debug_assertions),
                std::env::var_os("CAUSALMED_PYTHON").map(PathBuf::from),
                &root.join("python-worker"),
                &root.join("../.venv/bin/python"),
            )
        })
        .clone()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn app_and_release_never_fall_back_to_development_python() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("developer-python");
        std::fs::write(&p, b"not a python").unwrap();
        let app = t.path().join("Moved App.app/Contents/MacOS/causalmed");
        let r = resolve(&app, true, Some(p.clone()), t.path(), &p);
        assert_eq!(r.info.error.as_deref(), Some("bundled_runtime_missing"));
        assert_eq!(r.python, Path::new(UNAVAILABLE));
        let r = resolve(
            &t.path().join("binary"),
            false,
            Some(p.clone()),
            t.path(),
            &p,
        );
        assert_eq!(r.info.kind, "unavailable");
    }
    #[test]
    fn explicit_development_override_is_limited_to_unbundled_debug() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("python");
        std::fs::write(&p, b"python").unwrap();
        let r = resolve(
            &t.path().join("binary"),
            true,
            Some(p.clone()),
            t.path(),
            &p,
        );
        assert_eq!(r.python, p);
        assert_eq!(r.info.kind, "development");
    }
    #[test]
    fn invalid_manifest_is_a_visible_error() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(t.path().join("python/bin")).unwrap();
        std::fs::write(t.path().join("python/bin/python3.12"), b"broken").unwrap();
        std::fs::write(t.path().join("runtime.json"), b"{}").unwrap();
        assert_eq!(
            bundled(t.path(), "bundled").info.error.as_deref(),
            Some("bundled_runtime_invalid")
        );
    }
    #[test]
    fn relocated_bundle_is_preferred_and_executable_changes_fail_validation() {
        if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            return;
        }
        let t = tempfile::tempdir().unwrap();
        let contents = t.path().join("Moved App.app/Contents");
        let root = contents.join("Resources/python-worker");
        std::fs::create_dir_all(root.join("python/bin")).unwrap();
        let bin = root.join("python/bin/python3.12");
        std::fs::write(&bin, b"test executable").unwrap();
        let pin: serde_json::Value = serde_json::from_str(include_str!(
            "../../python/runtime/aarch64-apple-darwin.json"
        ))
        .unwrap();
        let m = serde_json::json!({"schemaVersion":1,"target":"aarch64-apple-darwin","minimumMacOS":"14.0","pythonVersion":pin["pythonVersion"],"anndataVersion":"0.12.19","archiveSha256":pin["sha256"],"dependencyLockSha256":sha(include_bytes!("../../python/runtime/wheels-aarch64-apple-darwin.json")),"pythonSha256":sha(b"test executable"),"inventorySha256":"f".repeat(64),"buildRecipeSha256":sha(include_bytes!("../../scripts/bundle_worker.py"))});
        std::fs::write(root.join("runtime.json"), m.to_string()).unwrap();
        let exe = contents.join("MacOS/causalmed");
        let r = resolve(
            &exe,
            true,
            Some("/missing/development".into()),
            Path::new("/missing"),
            Path::new("/missing"),
        );
        assert_eq!(r.python, bin);
        assert_eq!(r.info.kind, "bundled");
        std::fs::write(&bin, b"changed").unwrap();
        assert_eq!(
            resolve(&exe, true, None, t.path(), t.path())
                .info
                .error
                .as_deref(),
            Some("bundled_runtime_invalid")
        );
    }
}
