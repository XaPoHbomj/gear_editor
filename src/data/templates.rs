use crate::zon::zon_parse_entries;
use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    path::{Path as FsPath, PathBuf},
    sync::{Mutex, OnceLock},
    time::SystemTime,
};

static EQUIP_TEMPLATE_CACHE: OnceLock<Mutex<HashMap<(PathBuf, u64), EquipTemplateIndex>>> = OnceLock::new();

fn file_fingerprint(path: &FsPath) -> u64 {
    let mut hasher = DefaultHasher::new();
    if let Ok(metadata) = fs::metadata(path) {
        metadata.len().hash(&mut hasher);
        if let Ok(modified) = metadata.modified() {
            if let Ok(duration) = modified.duration_since(SystemTime::UNIX_EPOCH) {
                duration.as_secs().hash(&mut hasher);
                duration.subsec_nanos().hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

#[derive(Default, Clone)]
pub(crate) struct EquipTemplateIndex {
    pub(crate) by_item: HashMap<u32, EquipTemplateInfo>,
    pub(crate) by_suit_slot: HashMap<(u32, u32), u32>,
}

#[derive(Clone, Copy)]
pub(crate) struct EquipTemplateInfo {
    pub(crate) suit_type: u32,
    pub(crate) slot: u32,
}

pub(crate) fn load_equip_template_index(asset_dir: &FsPath) -> EquipTemplateIndex {
    let path = asset_dir.join("EquipmentTemplateTb.zon");
    let cache_key = (asset_dir.to_path_buf(), file_fingerprint(&path));
    let cache = EQUIP_TEMPLATE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().unwrap();
    if let Some(index) = cache.get(&cache_key) {
        return index.clone();
    }
    let mut index = EquipTemplateIndex::default();
    let Ok(data) = fs::read_to_string(&path) else {
        cache.insert(cache_key, index.clone());
        return index;
    };
    for entry in zon_parse_entries(&data) {
        let Some(item_id) = entry
            .get("item_id")
            .or_else(|| entry.get("id"))
            .and_then(|v| v.parse::<u32>().ok())
        else {
            continue;
        };
        let slot = entry
            .get("equipment_type")
            .and_then(|v| v.parse::<u32>().ok())
            .or_else(|| Some(item_id % 10))
            .unwrap_or(1);
        let suit_type = entry
            .get("suit_type")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or_else(|| (item_id / 100) * 100);
        let info = EquipTemplateInfo { suit_type, slot };
        index.by_item.insert(item_id, info);
        index
            .by_suit_slot
            .entry((info.suit_type, info.slot))
            .or_insert(item_id);
    }
    cache.insert(cache_key, index.clone());
    index
}

pub(crate) fn equip_set_id(item_id: u32, index: &EquipTemplateIndex) -> u32 {
    index
        .by_item
        .get(&item_id)
        .map(|info| info.suit_type)
        .unwrap_or_else(|| (item_id / 100) * 100)
}

pub(crate) fn equip_slot(item_id: u32, index: &EquipTemplateIndex) -> u32 {
    index
        .by_item
        .get(&item_id)
        .map(|info| info.slot)
        .unwrap_or_else(|| item_id % 10)
}

pub(crate) fn force_disc_fourth_digit(item_id: u32) -> u32 {
    let s = item_id.to_string();
    if s.len() < 4 {
        return item_id;
    }
    let mut chars: Vec<char> = s.chars().collect();
    chars[3] = '4';
    chars
        .iter()
        .collect::<String>()
        .parse::<u32>()
        .unwrap_or(item_id)
}

pub(crate) fn resolve_equip_item_id(
    set_id: u32,
    slot: u32,
    index: &EquipTemplateIndex,
) -> Option<u32> {
    index.by_suit_slot.get(&(set_id, slot)).copied()
}
