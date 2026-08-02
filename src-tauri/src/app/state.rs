use crate::diagnostics::Diagnostics;
use crate::settings::{CatalogEntry, Settings, discover_catalog, discover_knowledge_bases};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex};

pub(crate) struct AppState {
    pub(crate) diagnostics: Diagnostics,
    pub(crate) catalog: Mutex<CatalogSnapshot>,
    pub(crate) pending: Mutex<Option<PendingSession>>,
    pub(crate) reference_text: Mutex<Option<String>>,
    pub(crate) registered_shortcuts: Mutex<Vec<String>>,
    pub(crate) settings: Mutex<Settings>,
    pub(crate) toast_generation: Arc<AtomicU64>,
}

#[derive(Clone)]
pub(crate) struct PendingSession {
    pub(crate) id: String,
    pub(crate) target: isize,
    pub(crate) original: String,
    pub(crate) reference_text: Option<String>,
}

#[derive(Clone)]
pub(crate) struct CatalogSnapshot {
    pub(crate) agents: Vec<CatalogEntry>,
    pub(crate) knowledge_bases: Vec<CatalogEntry>,
}

impl CatalogSnapshot {
    pub(crate) fn discover(settings: &Settings) -> Self {
        Self {
            agents: discover_catalog(&settings.agents_root, "AGENT.md"),
            knowledge_bases: discover_knowledge_bases(settings),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        let settings = crate::settings::SettingsRepository::load().unwrap_or_default();
        let catalog = CatalogSnapshot::discover(&settings);
        Self {
            diagnostics: Diagnostics::new(),
            catalog: Mutex::new(catalog),
            pending: Mutex::new(None),
            reference_text: Mutex::new(None),
            registered_shortcuts: Mutex::new(Vec::new()),
            settings: Mutex::new(settings),
            toast_generation: Arc::new(AtomicU64::new(0)),
        }
    }
}

pub(crate) fn cached_catalog(state: &AppState) -> Result<CatalogSnapshot, String> {
    state
        .catalog
        .lock()
        .map_err(|_| "Catalog lock failed.".to_string())
        .map(|catalog| catalog.clone())
}

pub(crate) fn refresh_catalog(
    state: &AppState,
    settings: &Settings,
) -> Result<CatalogSnapshot, String> {
    let catalog = CatalogSnapshot::discover(settings);
    *state
        .catalog
        .lock()
        .map_err(|_| "Catalog lock failed.".to_string())? = catalog.clone();
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::CatalogSnapshot;
    use crate::settings::Settings;

    #[test]
    fn catalog_snapshot_refreshes_after_library_changes() {
        let root = std::env::temp_dir().join(format!(
            "codex-input-enhancer-catalog-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("current time should be after UNIX epoch")
                .as_nanos()
        ));
        let agents_root = root.join("agents");
        let knowledge_bases_root = root.join("knowledge-bases");
        std::fs::create_dir_all(&agents_root).unwrap();
        std::fs::create_dir_all(&knowledge_bases_root).unwrap();
        let settings = Settings {
            agents_root: agents_root.to_string_lossy().into_owned(),
            knowledge_bases_root: knowledge_bases_root.to_string_lossy().into_owned(),
            ..Settings::default()
        };
        std::fs::create_dir_all(agents_root.join("writer")).unwrap();
        std::fs::write(agents_root.join("writer/AGENT.md"), "# Writer").unwrap();
        std::fs::create_dir_all(knowledge_bases_root.join("product")).unwrap();
        std::fs::write(knowledge_bases_root.join("product/INDEX.md"), "# Product").unwrap();
        let catalog = CatalogSnapshot::discover(&settings);
        assert_eq!(catalog.agents.len(), 1);
        assert_eq!(catalog.knowledge_bases.len(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
