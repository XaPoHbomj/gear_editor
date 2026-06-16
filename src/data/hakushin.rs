use crate::{AppState, i18n::Locale};
use serde_json::Value as JsonValue;
use std::{
    collections::HashMap,
    fs,
    hash::{Hash, Hasher},
    path::Path as FsPath,
    sync::Mutex,
};

static HAKUSHIN_CACHE: std::sync::LazyLock<Mutex<HashMap<(String, u64), HakushinData>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Default, Clone)]
pub(crate) struct HakushinData {
    pub(crate) avatars: HashMap<u32, HakushinEntry>,
    pub(crate) weapons: HashMap<u32, HakushinEntry>,
    pub(crate) discs: HashMap<u32, HakushinEntry>,
    pub(crate) bangboos: HashMap<u32, HakushinEntry>,
    pub(crate) weapon_info: HashMap<u32, WeaponInfo>,
}

#[derive(Default, Clone)]
pub(crate) struct WeaponInfo {
    pub(crate) weapon_type: String,
    pub(crate) rarity: u32,
}

#[derive(Default, Clone)]
pub(crate) struct HakushinEntry {
    pub(crate) name: String,
    pub(crate) image_local: Option<String>,
}

pub(crate) fn to_asset_url(path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        let file_name = path
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .split('?')
            .next()
            .unwrap_or(path)
            .trim();
        let stem = FsPath::new(file_name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(file_name);
        return format!("/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/{stem}.webp");
    }
    if !path.contains('/') && !path.contains('.') {
        return format!("/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/{path}.webp");
    }
    format!("/assets/{}", path.trim_start_matches('/'))
}

pub(crate) fn load_hakushin_data(state: &AppState, locale: Locale) -> HakushinData {
    let lang_dir = state.dump_lang_dir(locale);
    let fingerprint = hakushin_data_fingerprint(&lang_dir);
    let cache_key = (locale.code().to_string(), fingerprint);

    let mut cache = HAKUSHIN_CACHE.lock().unwrap();
    if let Some(cached) = cache.get(&cache_key) {
        return cached.clone();
    }

    let data = HakushinData {
        avatars: load_hakushin_list(
            &state.root_dir,
            &lang_dir.join("characters.json"),
            "name",
            &[
                "image_local",
                "icon_local",
                "cropped_icon_local",
                "image",
                "icon",
                "cropped_icon",
            ],
        ),
        weapons: load_hakushin_list(
            &state.root_dir,
            &lang_dir.join("weapons.json"),
            "name",
            &["icon_local", "icon"],
        ),
        discs: load_hakushin_list(
            &state.root_dir,
            &lang_dir.join("drive_discs.json"),
            "name",
            &["icon_local", "icon"],
        ),
        bangboos: load_hakushin_list(
            &state.root_dir,
            &lang_dir.join("bangboos.json"),
            "name",
            &["icon_local", "icon"],
        ),
        weapon_info: load_weapon_info(&lang_dir.join("weapon_details.json")),
    };

    cache.insert(cache_key, data.clone());
    data
}

fn hakushin_data_fingerprint(dump_dir: &FsPath) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for file_name in [
        "characters.json",
        "weapons.json",
        "drive_discs.json",
        "bangboos.json",
        "weapon_details.json",
    ] {
        let path = dump_dir.join(file_name);
        path.to_string_lossy().hash(&mut hasher);
        if let Ok(metadata) = fs::metadata(&path) {
            metadata.len().hash(&mut hasher);
            if let Ok(modified) = metadata.modified() {
                if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                    duration.as_secs().hash(&mut hasher);
                    duration.subsec_nanos().hash(&mut hasher);
                }
            }
        }
    }
    hasher.finish()
}

fn load_hakushin_list(
    root_dir: &FsPath,
    path: &FsPath,
    name_key: &str,
    image_keys: &[&str],
) -> HashMap<u32, HakushinEntry> {
    let mut result = HashMap::new();
    let Ok(data) = fs::read_to_string(path) else {
        return result;
    };
    let Ok(json) = serde_json::from_str::<JsonValue>(&data) else {
        return result;
    };
    let Some(items) = json.as_array() else {
        return result;
    };

    for item in items {
        let Some(id) = item.get("id").and_then(|v| v.as_u64()) else {
            continue;
        };
        let name = item
            .get(name_key)
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();
        let mut image_local = None;
        for key in image_keys {
            if let Some(value) = item.get(*key).and_then(|v| v.as_str()) {
                if let Some(local_path) = normalize_image_reference(root_dir, value) {
                    image_local = Some(local_path);
                    break;
                }
            }
        }

        result.insert(id as u32, HakushinEntry { name, image_local });
    }

    result
}

fn load_weapon_info(path: &FsPath) -> HashMap<u32, WeaponInfo> {
    let mut result = HashMap::new();
    let Ok(data) = fs::read_to_string(path) else {
        return result;
    };
    let Ok(json) = serde_json::from_str::<JsonValue>(&data) else {
        return result;
    };
    let Some(obj) = json.as_object() else {
        return result;
    };
    for (_key, item) in obj {
        let Some(id) = item.get("id").and_then(|v| v.as_u64()) else {
            continue;
        };
        let weapon_type = item
            .get("weapon_type")
            .and_then(|v| v.as_object())
            .and_then(|m| m.values().next())
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let rarity = item.get("rarity").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        result.insert(id as u32, WeaponInfo { weapon_type, rarity });
    }
    result
}

fn normalize_image_reference(root_dir: &FsPath, value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.starts_with("/assets/") {
        return Some(trimmed.trim_start_matches('/').to_string());
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let file_name = trimmed
            .rsplit('/')
            .next()
            .unwrap_or(trimmed)
            .split('?')
            .next()
            .unwrap_or(trimmed)
            .trim();
        let stem = FsPath::new(file_name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(file_name)
            .trim();
        if stem.is_empty() {
            return None;
        }
        let candidate = format!("zzz_dump/assets/static.nanoka.cc/zzz/UI/{stem}.webp");
        return root_dir.join(&candidate).exists().then_some(candidate);
    }

    if root_dir.join(trimmed).exists() {
        return Some(trimmed.to_string());
    }

    let file_name = FsPath::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(trimmed);
    let stem = FsPath::new(file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(file_name)
        .trim();
    if stem.is_empty() {
        return None;
    }

    let candidate = format!("zzz_dump/assets/static.nanoka.cc/zzz/UI/{stem}.webp");
    root_dir.join(&candidate).exists().then_some(candidate)
}
