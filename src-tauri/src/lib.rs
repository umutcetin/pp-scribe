pub mod audio_toolkit;
pub mod commands;
pub mod knowledge;
pub mod meeting;
pub mod settings;
pub mod stt;

use std::sync::Arc;

use meeting::MeetingManager;
use stt::ModelManager;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .setup(|app| {
            if let Err(e) = meeting::storage::repair_interrupted(app.handle()) {
                eprintln!("[storage] repair failed: {}", e);
            }
            let models = Arc::new(ModelManager::new(app.handle())?);
            let manager = Arc::new(MeetingManager::new(app.handle(), models.clone()));
            manager.preload_live_model();
            app.manage(manager);
            match knowledge::KnowledgeManager::new(app.handle(), models) {
                Ok(kb) => {
                    app.manage(kb);
                }
                Err(e) => eprintln!("[kb] knowledge base unavailable: {e}"),
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::list_audio_devices,
            commands::list_models,
            commands::download_model,
            commands::start_meeting,
            commands::stop_meeting,
            commands::get_active_meeting,
            commands::list_meetings,
            commands::get_meeting,
            commands::rename_meeting,
            commands::delete_meeting,
            commands::refine_meeting,
            commands::is_refining,
            commands::export_meeting,
            commands::meetings_root,
            commands::copy_text,
            knowledge::commands::kb_status,
            knowledge::commands::kb_list_sources,
            knowledge::commands::kb_get_source,
            knowledge::commands::kb_add_files,
            knowledge::commands::kb_add_text,
            knowledge::commands::kb_add_url,
            knowledge::commands::kb_delete_source,
            knowledge::commands::kb_reindex_source,
            knowledge::commands::kb_backfill,
            knowledge::commands::kb_search,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
