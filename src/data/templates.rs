use crate::{
    app_state::AppState,
    data::hakushin::load_hakushin_data,
    zon::{read_zon, zon_get_number},
};
use serde_json::Value as JsonValue;
use std::{collections::HashMap, fs, path::Path as FsPath, sync::OnceLock};

static EQUIP_TEMPLATE: OnceLock<EquipTemplateIndex> = OnceLock::new();

#[derive(Default)]
pub(crate) struct EquipTemplateIndex {
    pub(crate) by_item: HashMap<u32, EquipTemplateInfo>,
    pub(crate) by_suit_slot: HashMap<(u32, u32), u32>,
}

#[derive(Clone, Copy)]
pub(crate) struct EquipTemplateInfo {
    pub(crate) suit_type: u32,
    pub(crate) slot: u32,
}

pub(crate) fn load_equip_template_index(asset_dir: &FsPath) -> &'static EquipTemplateIndex {
    EQUIP_TEMPLATE.get_or_init(|| {
        let mut index = EquipTemplateIndex::default();
        let path = asset_dir.join("EquipmentTemplateTb.json");
        let Ok(data) = fs::read_to_string(path) else {
            return index;
        };
        let Ok(json) = serde_json::from_str::<JsonValue>(&data) else {
            return index;
        };
        let Some(items) = json.get("data").and_then(|v| v.as_array()) else {
            return index;
        };

        for item in items {
            let Some(item_id) = item.get("item_id").and_then(|v| v.as_u64()) else {
                continue;
            };
            let Some(slot) = item.get("equipment_type").and_then(|v| v.as_u64()) else {
                continue;
            };
            let Some(suit_type) = item.get("suit_type").and_then(|v| v.as_u64()) else {
                continue;
            };
            let info = EquipTemplateInfo {
                suit_type: suit_type as u32,
                slot: slot as u32,
            };
            index.by_item.insert(item_id as u32, info);
            index
                .by_suit_slot
                .entry((info.suit_type, info.slot))
                .or_insert(item_id as u32);
        }

        index
    })
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

pub(crate) fn load_avatar_templates(asset_dir: &FsPath) -> HashMap<u32, String> {
    let path = asset_dir.join("AvatarBaseTemplateTb.json");
    let data = fs::read_to_string(path).unwrap_or_default();
    parse_json_map(&data, "id", "name")
}

pub(crate) fn load_weapon_templates(asset_dir: &FsPath) -> HashMap<u32, String> {
    let path = asset_dir.join("WeaponTemplateTb.json");
    let data = fs::read_to_string(path).unwrap_or_default();
    parse_json_map(&data, "item_id", "weapon_name")
}

pub(crate) fn load_player_weapons(state: &AppState, uid: u32) -> Vec<(u32, String)> {
    let weapon_dir = state.state_dir.join(format!("player/{uid}/weapon"));
    let weapon_templates = load_weapon_templates(&state.asset_dir);
    let hakushin = load_hakushin_data(state);
    let mut result = Vec::new();

    if let Ok(entries) = fs::read_dir(&weapon_dir) {
        for entry in entries.flatten() {
            let Some(file_name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            let uid = match file_name
                .strip_suffix(".zon")
                .unwrap_or(&file_name)
                .parse::<u32>()
            {
                Ok(value) if value > 0 => value,
                _ => continue,
            };
            let weapon = read_zon(&entry.path());
            let weapon_id = weapon
                .as_ref()
                .and_then(|v| zon_get_number(v, "id"))
                .unwrap_or(0) as u32;
            let name = hakushin
                .weapons
                .get(&weapon_id)
                .map(|entry| entry.name.clone())
                .or_else(|| weapon_templates.get(&weapon_id).cloned())
                .unwrap_or_else(|| format!("Weapon {weapon_id}"));
            result.push((uid, format!("{} (UID {})", name, uid)));
        }
    }

    result.sort_by_key(|(uid, _)| *uid);
    result
}

pub(crate) fn render_weapon_select(current_uid: u32, options: &[(u32, String)]) -> String {
    let mut html = String::new();
    html.push_str("<select name=\"cur_weapon_uid\">");
    html.push_str(&format!(
        "<option value=\"0\"{}>None</option>",
        if current_uid == 0 { " selected" } else { "" }
    ));
    for (uid, label) in options {
        html.push_str(&format!(
            "<option value=\"{}\"{}>{}</option>",
            uid,
            if *uid == current_uid { " selected" } else { "" },
            label
        ));
    }
    html.push_str("</select>");
    html
}

pub(crate) fn load_equip_templates(asset_dir: &FsPath) -> HashMap<u32, String> {
    let equip_path = asset_dir.join("EquipmentTemplateTb.json");
    let suit_path = asset_dir.join("EquipmentSuitTemplateTb.json");

    let equip_data = fs::read_to_string(equip_path).unwrap_or_default();
    let suit_data = fs::read_to_string(suit_path).unwrap_or_default();

    let equip_to_suit = parse_json_map_u32(&equip_data, "item_id", "suit_type");
    let suit_names = parse_json_map(&suit_data, "id", "name");

    let mut result = HashMap::new();
    for (item_id, suit_id) in equip_to_suit {
        let name = suit_names
            .get(&suit_id)
            .cloned()
            .unwrap_or_else(|| format!("Suit {suit_id}"));
        result.insert(item_id, name);
    }

    result
}

pub(crate) fn load_player_equips(state: &AppState, uid: u32) -> Vec<(u32, u32, String)> {
    let equip_dir = state.state_dir.join(format!("player/{uid}/equip"));
    let equip_templates = load_equip_templates(&state.asset_dir);
    let hakushin = load_hakushin_data(state);
    let equip_index = load_equip_template_index(&state.asset_dir);
    let mut result = Vec::new();

    if let Ok(entries) = fs::read_dir(&equip_dir) {
        for entry in entries.flatten() {
            let Some(file_name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            let uid = match file_name
                .strip_suffix(".zon")
                .unwrap_or(&file_name)
                .parse::<u32>()
            {
                Ok(value) if value > 0 => value,
                _ => continue,
            };
            let equip = read_zon(&entry.path());
            let equip_item_id = equip
                .as_ref()
                .and_then(|v| zon_get_number(v, "id"))
                .unwrap_or(0) as u32;
            let set_id = equip_set_id(equip_item_id, equip_index);
            let name = hakushin
                .discs
                .get(&set_id)
                .map(|entry| entry.name.clone())
                .or_else(|| equip_templates.get(&equip_item_id).cloned())
                .unwrap_or_else(|| format!("Disc {equip_item_id}"));
            let slot = equip_slot(equip_item_id, equip_index);
            result.push((uid, slot, format!("{} (UID {})", name, uid)));
        }
    }

    result.sort_by_key(|(uid, _, _)| *uid);
    result
}

pub(crate) fn render_equip_selects(options: &[(u32, u32, String)], equipped: &[u32]) -> String {
    let mut html = String::new();
    for slot in 0..6 {
        let current = *equipped.get(slot).unwrap_or(&0);
        html.push_str("<div><label>Slot ");
        html.push_str(&(slot + 1).to_string());
        html.push_str("</label><select name=\"equip_slot_");
        html.push_str(&(slot + 1).to_string());
        html.push_str("\">");
        html.push_str(&format!(
            "<option value=\"0\"{}>Empty</option>",
            if current == 0 { " selected" } else { "" }
        ));
        for (uid, opt_slot, label) in options {
            if *opt_slot != (slot as u32 + 1) {
                continue;
            }
            html.push_str(&format!(
                "<option value=\"{}\"{}>{}</option>",
                uid,
                if *uid == current { " selected" } else { "" },
                label
            ));
        }
        html.push_str("</select></div>");
    }

    html
}

fn parse_json_map(data: &str, key: &str, value: &str) -> HashMap<u32, String> {
    let mut result = HashMap::new();
    let Ok(json) = serde_json::from_str::<JsonValue>(data) else {
        return result;
    };

    let Some(items) = json.get("data").and_then(|v| v.as_array()) else {
        return result;
    };

    for item in items {
        let Some(id) = item.get(key).and_then(|v| v.as_u64()) else {
            continue;
        };
        if let Some(name) = item.get(value).and_then(|v| v.as_str()) {
            result.insert(id as u32, name.to_string());
        }
    }

    result
}

fn parse_json_map_u32(data: &str, key: &str, value: &str) -> HashMap<u32, u32> {
    let mut result = HashMap::new();
    let Ok(json) = serde_json::from_str::<JsonValue>(data) else {
        return result;
    };

    let Some(items) = json.get("data").and_then(|v| v.as_array()) else {
        return result;
    };

    for item in items {
        let Some(id) = item.get(key).and_then(|v| v.as_u64()) else {
            continue;
        };
        if let Some(value) = item.get(value).and_then(|v| v.as_u64()) {
            result.insert(id as u32, value as u32);
        }
    }

    result
}
