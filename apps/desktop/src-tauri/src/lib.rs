use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

use er_optimizer_core::{GameData, load_game_data};
use tauri::Manager;

mod commands;
mod dto;
mod errors;

pub type CancelFlag = Arc<AtomicBool>;

pub struct AppState {
    pub data: Arc<GameData>,
    pub jobs: Arc<Mutex<HashMap<String, CancelFlag>>>,
    pub next_job: AtomicU64,
}

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = resolve_data_dir(app)?;
            let data = load_game_data(&data_dir).map_err(errors::AppError::from)?;
            app.manage(AppState {
                data: Arc::new(data),
                jobs: Arc::new(Mutex::new(HashMap::new())),
                next_job: AtomicU64::new(1),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::data::get_catalog,
            commands::data::get_weapon_profile,
            commands::data::affinities_for_weapon,
            commands::data::compatible_aow_names,
            commands::data::compatible_aow_names_for_affinity,
            commands::data::weapon_names_for_type,
            commands::data::weapon_scaling_for_upgrade,
            commands::optimize::estimate_search_space,
            commands::optimize::run_search,
            commands::optimize::solve_build,
            commands::optimize::build_upgrade_series,
            commands::optimize::start_search,
            commands::optimize::cancel_search,
            commands::paths::build_path_preview,
            commands::paths::start_path_preview,
            commands::paths::cancel_path_preview,
            commands::affinity_watch::build_affinity_watch,
            commands::affinity_watch::start_affinity_watch,
            commands::affinity_watch::cancel_affinity_watch,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri app");
}

fn resolve_data_dir(app: &tauri::App) -> Result<PathBuf, errors::AppError> {
    let dev_data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../data/phase1")
        .canonicalize()
        .ok();
    if let Some(path) = dev_data_dir.filter(|path| path.exists()) {
        return Ok(path);
    }

    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|err| errors::AppError::new(format!("failed to resolve resource dir: {err}")))?;
    let bundled = resource_dir.join("data").join("phase1");
    Ok(bundled)
}
