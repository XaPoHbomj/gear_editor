use crate::{
    app_state::AppState,
    data::hakushin::load_hakushin_data,
    i18n::{Locale, t},
    zon::{read_zon, zon_get_number, zon_parse_entries},
};
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
        let path = asset_dir.join("EquipmentTemplateTb.zon");
        let Ok(data) = fs::read_to_string(&path) else {
            return index;
        };
        for entry in zon_parse_entries(&data) {
            let Some(item_id) = entry.get("item_id").or_else(|| entry.get("id")).and_then(|v| v.parse::<u32>().ok()) else {
                continue;
            };
            let Some(slot) = entry.get("equipment_type").and_then(|v| v.parse::<u32>().ok()) else {
                continue;
            };
            let Some(suit_type) = entry.get("suit_type").and_then(|v| v.parse::<u32>().ok()) else {
                continue;
            };
            let info = EquipTemplateInfo { suit_type, slot };
            index.by_item.insert(item_id, info);
            index.by_suit_slot.entry((info.suit_type, info.slot)).or_insert(item_id);
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
    let path = asset_dir.join("AvatarBaseTemplateTb.zon");
    let Ok(data) = fs::read_to_string(&path) else {
        return HashMap::new();
    };
    zon_parse_entries(&data)
        .into_iter()
        .filter_map(|entry| {
            let id = entry.get("id").and_then(|v| v.parse::<u32>().ok())?;
            Some((id, format!("{id}")))
        })
        .collect()
}

pub(crate) fn load_weapon_templates(asset_dir: &FsPath) -> HashMap<u32, String> {
    let path = asset_dir.join("WeaponTemplateTb.zon");
    let Ok(data) = fs::read_to_string(&path) else {
        return HashMap::new();
    };
    zon_parse_entries(&data)
        .into_iter()
        .filter_map(|entry| {
            let id = entry.get("id").and_then(|v| v.parse::<u32>().ok())?;
            Some((id, format!("{id}")))
        })
        .collect()
}

pub(crate) fn load_avatar_template_entries(asset_dir: &FsPath) -> Vec<HashMap<String, String>> {
    let path = asset_dir.join("AvatarBaseTemplateTb.zon");
    let Ok(data) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    zon_parse_entries(&data)
}

pub(crate) fn load_buddy_template_ids(asset_dir: &FsPath) -> Vec<u32> {
    let path = asset_dir.join("BuddyBaseTemplateTb.zon");
    let Ok(data) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    zon_parse_entries(&data)
        .into_iter()
        .filter_map(|entry| entry.get("id").and_then(|v| v.parse::<u32>().ok()))
        .filter(|id| *id < 55000)
        .collect()
}

pub(crate) fn load_player_weapons(
    state: &AppState,
    uid: u32,
    locale: Locale,
) -> Vec<(u32, String)> {
    let save = crate::player_state::load_player_save(state, uid).unwrap_or_default();
    let weapon_templates = load_weapon_templates(&state.asset_dir);
    let hakushin = load_hakushin_data(state, locale);
    let mut result = Vec::new();

    for weapon in &save.weapon {
        let name = hakushin
            .weapons
            .get(&weapon.id)
            .map(|entry| entry.name.clone())
            .or_else(|| weapon_templates.get(&weapon.id).cloned())
            .unwrap_or_else(|| format!("{} {}", t(locale, "fallback.weapon"), weapon.id));
        result.push((weapon.uid, format!("{} (UID {})", name, weapon.uid)));
    }

    result.sort_by_key(|(uid, _)| *uid);
    result
}

pub(crate) fn render_weapon_select(
    locale: Locale,
    current_uid: u32,
    options: &[(u32, String)],
) -> String {
    let mut html = String::new();
    html.push_str("<select name=\"cur_weapon_uid\">");
    html.push_str(&format!(
        "<option value=\"0\"{}>{}</option>",
        if current_uid == 0 { " selected" } else { "" },
        t(locale, "none")
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
    let equip_path = asset_dir.join("EquipmentTemplateTb.zon");
    let suit_path = asset_dir.join("EquipmentSuitTemplateTb.zon");

    let equip_data = fs::read_to_string(&equip_path).unwrap_or_default();
    let suit_data = fs::read_to_string(&suit_path).unwrap_or_default();

    let equip_to_suit: HashMap<u32, u32> = zon_parse_entries(&equip_data)
        .into_iter()
        .filter_map(|entry| {
            let id = entry.get("id").and_then(|v| v.parse::<u32>().ok())?;
            let suit_type = entry.get("suit_type").and_then(|v| v.parse::<u32>().ok())?;
            Some((id, suit_type))
        })
        .collect();

    let suit_names: HashMap<u32, String> = zon_parse_entries(&suit_data)
        .into_iter()
        .filter_map(|entry| {
            let id = entry.get("id").and_then(|v| v.parse::<u32>().ok())?;
            let name = entry.get("name").cloned().unwrap_or_else(|| format!("{id}"));
            Some((id, name))
        })
        .collect();

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

pub(crate) fn load_player_equips(
    state: &AppState,
    uid: u32,
    locale: Locale,
) -> Vec<(u32, u32, String)> {
    let save = crate::player_state::load_player_save(state, uid).unwrap_or_default();
    let equip_templates = load_equip_templates(&state.asset_dir);
    let hakushin = load_hakushin_data(state, locale);
    let equip_index = load_equip_template_index(&state.asset_dir);
    let mut result = Vec::new();

    for equip in &save.equip {
        let set_id = equip_set_id(equip.id, equip_index);
        let name = hakushin
            .discs
            .get(&set_id)
            .map(|entry| entry.name.clone())
            .or_else(|| equip_templates.get(&equip.id).cloned())
            .unwrap_or_else(|| format!("{} {}", t(locale, "fallback.disc"), equip.id));
        let slot = equip_slot(equip.id, equip_index);
        result.push((equip.uid, slot, format!("{} (UID {})", name, equip.uid)));
    }

    result.sort_by_key(|(uid, _, _)| *uid);
    result
}

pub(crate) fn render_equip_selects(
    locale: Locale,
    options: &[(u32, u32, String)],
    equipped: &[u32],
) -> String {
    let mut html = String::new();
    for slot in 0..6 {
        let current = *equipped.get(slot).unwrap_or(&0);
        html.push_str("<div><label>");
        html.push_str(t(locale, "slot"));
        html.push_str(" ");
        html.push_str(&(slot + 1).to_string());
        html.push_str("</label><select name=\"equip_slot_");
        html.push_str(&(slot + 1).to_string());
        html.push_str("\">");
        html.push_str(&format!(
            "<option value=\"0\"{}>{}</option>",
            if current == 0 { " selected" } else { "" },
            t(locale, "empty"),
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
