// Listing the commands makes Tauri check every IPC call against capabilities/, so a
// command registered in lib.rs stays unreachable until a capability allows it.
const COMMANDS: &[&str] = &[
    "get_catalog",
    "get_profiles",
    "get_data_manifest",
    "get_weapon_profile",
    "compatible_aow_names_for_affinity",
    "start_solve_build",
    "start_upgrade_series",
    "start_ar_bleed_frontier",
    "cancel_analysis",
    "get_analysis_status",
    "start_search",
    "cancel_search",
    "get_search_status",
    "start_path_preview",
    "cancel_path_preview",
    "get_path_preview_status",
    "start_affinity_watch",
    "cancel_affinity_watch",
    "get_affinity_watch_status",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
