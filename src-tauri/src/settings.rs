use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

/// Where the "them" (remote participants) channel comes from.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemSource {
    /// System output via CoreAudio process tap — no virtual driver needed.
    #[default]
    System,
    /// A loopback input device such as BlackHole.
    Device,
    Off,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppSettings {
    /// `None` = system default input.
    #[serde(default)]
    pub mic_device: Option<String>,
    #[serde(default)]
    pub them_source: ThemSource,
    #[serde(default)]
    pub them_device: Option<String>,

    #[serde(default = "default_live_model")]
    pub live_model: String,
    #[serde(default = "default_refine_model")]
    pub refine_model: String,
    /// Whisper language code, or "auto".
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_true")]
    pub auto_refine: bool,

    #[serde(default = "default_me_label")]
    pub me_label: String,
    #[serde(default = "default_them_label")]
    pub them_label: String,
}

fn default_live_model() -> String {
    "parakeet-v2".into()
}
fn default_refine_model() -> String {
    "whisper-large-v3-turbo".into()
}
fn default_language() -> String {
    "en".into()
}
fn default_true() -> bool {
    true
}
fn default_me_label() -> String {
    "Me".into()
}
fn default_them_label() -> String {
    "Them".into()
}

impl Default for AppSettings {
    fn default() -> Self {
        serde_json::from_value(serde_json::json!({})).expect("defaults")
    }
}

const SETTINGS_STORE_PATH: &str = "settings_store.json";

pub fn get_settings(app: &AppHandle) -> AppSettings {
    let store = app
        .store(SETTINGS_STORE_PATH)
        .expect("Failed to initialize store");

    store
        .get("settings")
        .and_then(|v| serde_json::from_value::<AppSettings>(v).ok())
        .unwrap_or_default()
}

pub fn write_settings(app: &AppHandle, settings: &AppSettings) {
    let store = app
        .store(SETTINGS_STORE_PATH)
        .expect("Failed to initialize store");
    store.set("settings", serde_json::to_value(settings).unwrap());
}
