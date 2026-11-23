use crate::i18n::Language;
use crate::profile::Profile;
use serde_json::{json, Value};
use std::sync::Mutex;

static USER: Mutex<String> = Mutex::new(String::new());

pub(in crate::app) fn profiles_dir() -> std::path::PathBuf {
    super::super::data_dir().join("profiles")
}

pub(in crate::app) fn user_dir() -> std::path::PathBuf {
    let id = USER.lock().map(|guard| guard.clone()).unwrap_or_default();
    profiles_dir().join(if id.is_empty() { "guest" } else { id.as_str() })
}

pub(in crate::app) fn set_user(id: &str) {
    if let Ok(mut guard) = USER.lock() {
        *guard = id.to_string();
    }
}

pub(in crate::app) fn wipe_guest() {
    let dir = profiles_dir().join("guest");
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
}

pub(in crate::app) fn new_profile_id() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("p{ms:x}")
}

pub(in crate::app) fn load_profiles() -> Vec<Profile> {
    let path = profiles_dir().join("profiles.json");
    let source = std::fs::read_to_string(path).unwrap_or_default();
    let Ok(Value::Array(items)) = serde_json::from_str(&source) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let id = item["id"].as_str()?.to_string();
            let name = item["name"].as_str()?.to_string();
            let tastes = item["tastes"]
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_u64().map(|number| number as usize))
                        .filter(|index| *index < super::super::GENRES.len())
                        .collect()
                })
                .unwrap_or_default();
            let avatar = item["avatar"].as_u64().unwrap_or(0) as usize % super::super::GENRES.len();
            let accent = item["accent"]
                .as_str()
                .and_then(|hex| u32::from_str_radix(hex.trim_start_matches('#'), 16).ok())
                .filter(|value| *value <= 0xFF_FFFF);
            let language = item["language"].as_str().unwrap_or("fr").to_string();
            Some(Profile {
                id,
                name,
                tastes,
                avatar,
                photo: item["photo"].as_bool().unwrap_or(false),
                accent,
                language,
            })
        })
        .collect()
}

pub(in crate::app) fn save_profiles(profiles: &[Profile]) {
    let dir = profiles_dir();
    let _ = std::fs::create_dir_all(&dir);
    let items: Vec<Value> = profiles
        .iter()
        .map(|profile| {
            json!({
                "id": profile.id,
                "name": profile.name,
                "tastes": profile.tastes,
                "avatar": profile.avatar,
                "photo": profile.photo,
                "accent": profile.accent.map(|value| format!("#{:06X}", value)),
                "language": profile.language
            })
        })
        .collect();
    let _ = std::fs::write(
        dir.join("profiles.json"),
        serde_json::to_string_pretty(&Value::Array(items)).unwrap_or_default(),
    );
}

pub(in crate::app) fn load_language() -> Language {
    let code =
        std::fs::read_to_string(super::super::data_dir().join("language.txt")).unwrap_or_else(|_| "fr".to_string());
    Language::from_code(code.trim())
}

pub(in crate::app) fn save_language(language: Language) {
    let dir = super::super::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("language.txt"), language.code());
}

pub(in crate::app) fn migrate_legacy(dir: &std::path::Path) {
    for file in ["history.json", "likes.json", "playlists.json", "recent.txt"] {
        let source = super::super::data_dir().join(file);
        if source.exists() {
            let destination = dir.join(file);
            if std::fs::rename(&source, &destination).is_err() && std::fs::copy(&source, &destination).is_ok() {
                let _ = std::fs::remove_file(&source);
            }
        }
    }
}

pub(in crate::app) fn load_discord_pref() -> bool {
    std::fs::read_to_string(super::super::data_dir().join("discord_rpc.txt")).map_or(true, |s| s.trim() != "0")
}

pub(in crate::app) fn save_discord_pref(on: bool) {
    let dir = super::super::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("discord_rpc.txt"), if on { "1" } else { "0" });
}
