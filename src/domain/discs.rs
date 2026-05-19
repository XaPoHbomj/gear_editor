use crate::{
    AppState,
    i18n::{Locale, t},
};
use serde_json::Value as JsonValue;
use std::{collections::HashMap, fs, sync::Mutex};

static STAT_NAMES_CACHE: std::sync::LazyLock<Mutex<HashMap<String, HashMap<u32, String>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

fn load_stat_names(state: &AppState, locale: Locale) -> HashMap<u32, String> {
    let lang_code = locale.code().to_string();
    let mut cache = STAT_NAMES_CACHE.lock().unwrap();
    if let Some(cached) = cache.get(&lang_code) {
        return cached.clone();
    }

    let lang_dir = state.dump_lang_dir(locale);
    let mut map = HashMap::new();

    let mut weapon_prop = HashMap::new();
    let weapon_template = state.asset_dir.join("WeaponTemplateTb.json");
    if let Ok(data) = fs::read_to_string(weapon_template) {
        if let Ok(json) = serde_json::from_str::<JsonValue>(&data) {
            if let Some(items) = json.get("data").and_then(|v| v.as_array()) {
                for item in items {
                    let Some(item_id) = item.get("item_id").and_then(|v| v.as_u64()) else {
                        continue;
                    };
                    let base_prop = item
                        .get("base_property")
                        .and_then(|v| v.get("property"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32;
                    let rand_prop = item
                        .get("rand_property")
                        .and_then(|v| v.get("property"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32;
                    weapon_prop.insert(item_id as u32, (base_prop, rand_prop));
                }
            }
        }
    }

    let weapon_details = lang_dir.join("weapon_details.json");
    if let Ok(data) = fs::read_to_string(weapon_details) {
        if let Ok(json) = serde_json::from_str::<JsonValue>(&data) {
            if let Some(obj) = json.as_object() {
                for (key, details) in obj {
                    let Ok(item_id) = key.parse::<u32>() else {
                        continue;
                    };
                    let Some((base_prop, rand_prop)) = weapon_prop.get(&item_id) else {
                        continue;
                    };
                    if let Some(name) = details
                        .get("base_property")
                        .and_then(|v| v.get("name"))
                        .and_then(|v| v.as_str())
                    {
                        if *base_prop > 0 {
                            map.entry(*base_prop).or_insert_with(|| name.to_string());
                        }
                    }
                    if let Some(name) = details
                        .get("rand_property")
                        .and_then(|v| v.get("name"))
                        .and_then(|v| v.as_str())
                    {
                        if *rand_prop > 0 {
                            map.entry(*rand_prop).or_insert_with(|| name.to_string());
                        }
                    }
                }
            }
        }
    }

    let bangboo_details = lang_dir.join("bangboo_details.json");
    if let Ok(data) = fs::read_to_string(bangboo_details) {
        if let Ok(json) = serde_json::from_str::<JsonValue>(&data) {
            if let Some(obj) = json.as_object() {
                for (_, details) in obj {
                    if let Some(ascensions) = details.get("ascensions").and_then(|v| v.as_object())
                    {
                        for (_, stage) in ascensions {
                            if let Some(extra_props) =
                                stage.get("extra_props").and_then(|v| v.as_array())
                            {
                                for prop in extra_props {
                                    let Some(id) = prop.get("id").and_then(|v| v.as_u64()) else {
                                        continue;
                                    };
                                    let Some(name) = prop.get("name").and_then(|v| v.as_str())
                                    else {
                                        continue;
                                    };
                                    map.entry(id as u32).or_insert_with(|| name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    cache.insert(lang_code, map.clone());
    map
}

const STAT_HP: u32 = 11103;
const STAT_ATK: u32 = 12103;
const STAT_DEF: u32 = 13103;
const STAT_HP_PCT: u32 = 11102;
const STAT_ATK_PCT: u32 = 12102;
const STAT_DEF_PCT: u32 = 13102;
const STAT_CRIT_RATE: u32 = 20103;
const STAT_CRIT_DMG: u32 = 21103;
const STAT_ANOMALY_PROF: u32 = 31203;
const STAT_PEN: u32 = 23203;
const STAT_PEN_RATIO: u32 = 23202;
const STAT_ANOMALY_MASTERY: u32 = 31402;
const STAT_IMPACT: u32 = 12202;
const STAT_ENERGY_REGEN: u32 = 30502;
const STAT_PHYSICAL_DMG: u32 = 31503;
const STAT_FIRE_DMG: u32 = 31603;
const STAT_ICE_DMG: u32 = 31703;
const STAT_ELECTRIC_DMG: u32 = 31803;
const STAT_ETHER_DMG: u32 = 31903;

fn disk_stat_label(locale: Locale, key: u32) -> String {
    match key {
        STAT_HP => t(locale, "stat.hp").to_string(),
        STAT_ATK => t(locale, "stat.atk").to_string(),
        STAT_DEF => t(locale, "stat.def").to_string(),
        STAT_HP_PCT => t(locale, "stat.hp_pct").to_string(),
        STAT_ATK_PCT => t(locale, "stat.atk_pct").to_string(),
        STAT_DEF_PCT => t(locale, "stat.def_pct").to_string(),
        STAT_CRIT_RATE => t(locale, "stat.crit_rate").to_string(),
        STAT_CRIT_DMG => t(locale, "stat.crit_dmg").to_string(),
        STAT_ANOMALY_PROF => t(locale, "stat.anomaly_prof").to_string(),
        STAT_PEN => t(locale, "stat.pen").to_string(),
        STAT_PEN_RATIO => t(locale, "stat.pen_ratio").to_string(),
        STAT_ANOMALY_MASTERY => t(locale, "stat.anomaly_mastery").to_string(),
        STAT_IMPACT => t(locale, "stat.impact").to_string(),
        STAT_ENERGY_REGEN => t(locale, "stat.energy_regen").to_string(),
        STAT_PHYSICAL_DMG => t(locale, "stat.physical_dmg").to_string(),
        STAT_FIRE_DMG => t(locale, "stat.fire_dmg").to_string(),
        STAT_ICE_DMG => t(locale, "stat.ice_dmg").to_string(),
        STAT_ELECTRIC_DMG => t(locale, "stat.electric_dmg").to_string(),
        STAT_ETHER_DMG => t(locale, "stat.ether_dmg").to_string(),
        _ => String::new(),
    }
}

pub(crate) fn disk_main_stat_options(slot: u32) -> Vec<u32> {
    match slot {
        1 => vec![STAT_HP],
        2 => vec![STAT_ATK],
        3 => vec![STAT_DEF],
        4 => vec![
            STAT_ATK_PCT,
            STAT_HP_PCT,
            STAT_DEF_PCT,
            STAT_CRIT_RATE,
            STAT_CRIT_DMG,
            STAT_ANOMALY_PROF,
        ],
        5 => vec![
            STAT_ATK_PCT,
            STAT_HP_PCT,
            STAT_DEF_PCT,
            STAT_PHYSICAL_DMG,
            STAT_ICE_DMG,
            STAT_FIRE_DMG,
            STAT_ELECTRIC_DMG,
            STAT_ETHER_DMG,
            STAT_PEN_RATIO,
        ],
        6 => vec![
            STAT_ATK_PCT,
            STAT_HP_PCT,
            STAT_DEF_PCT,
            STAT_ANOMALY_MASTERY,
            STAT_IMPACT,
            STAT_ENERGY_REGEN,
        ],
        _ => vec![],
    }
}

pub(crate) fn normalize_disk_main_stat(slot: u32, key: u32) -> Option<u32> {
    let options = disk_main_stat_options(slot);
    if options.contains(&key) {
        Some(key)
    } else {
        None
    }
}

pub(crate) fn disk_main_base_value(key: u32) -> Option<u32> {
    match key {
        STAT_HP => Some(550),
        STAT_ATK => Some(79),
        STAT_DEF => Some(46),
        STAT_ATK_PCT => Some(750),
        STAT_HP_PCT => Some(750),
        STAT_DEF_PCT => Some(1200),
        STAT_CRIT_RATE => Some(600),
        STAT_CRIT_DMG => Some(1200),
        STAT_ANOMALY_PROF => Some(23),
        STAT_PHYSICAL_DMG | STAT_ICE_DMG | STAT_FIRE_DMG | STAT_ELECTRIC_DMG | STAT_ETHER_DMG => {
            Some(750)
        }
        STAT_PEN_RATIO => Some(600),
        STAT_ANOMALY_MASTERY => Some(750),
        STAT_IMPACT => Some(450),
        STAT_ENERGY_REGEN => Some(1500),
        _ => None,
    }
}

pub(crate) fn disk_sub_base_value(key: u32) -> Option<u32> {
    match key {
        STAT_HP => Some(112),
        STAT_ATK => Some(19),
        STAT_DEF => Some(15),
        STAT_HP_PCT => Some(300),
        STAT_ATK_PCT => Some(300),
        STAT_DEF_PCT => Some(480),
        STAT_CRIT_DMG => Some(480),
        STAT_CRIT_RATE => Some(240),
        STAT_ANOMALY_PROF => Some(9),
        STAT_PEN => Some(9),
        _ => None,
    }
}

pub(crate) fn disk_sub_stat_options(main_key: u32) -> Vec<u32> {
    let mut options = vec![
        STAT_HP,
        STAT_ATK,
        STAT_DEF,
        STAT_HP_PCT,
        STAT_ATK_PCT,
        STAT_DEF_PCT,
        STAT_CRIT_DMG,
        STAT_CRIT_RATE,
        STAT_ANOMALY_PROF,
        STAT_PEN,
    ];

    match main_key {
        STAT_HP => {
            options.retain(|key| *key != STAT_HP);
        }
        STAT_ATK => {
            options.retain(|key| *key != STAT_ATK);
        }
        STAT_DEF => {
            options.retain(|key| *key != STAT_DEF);
        }
        STAT_HP_PCT => {
            options.retain(|key| *key != STAT_HP_PCT);
        }
        STAT_ATK_PCT => {
            options.retain(|key| *key != STAT_ATK_PCT);
        }
        STAT_DEF_PCT => {
            options.retain(|key| *key != STAT_DEF_PCT);
        }
        STAT_CRIT_RATE => {
            options.retain(|key| *key != STAT_CRIT_RATE);
        }
        STAT_CRIT_DMG => {
            options.retain(|key| *key != STAT_CRIT_DMG);
        }
        STAT_ANOMALY_PROF => {
            options.retain(|key| *key != STAT_ANOMALY_PROF);
        }
        _ => {}
    }

    options
}

pub(crate) fn all_main_stat_keys() -> &'static [u32] {
    &[
        STAT_HP, STAT_ATK, STAT_DEF,
        STAT_HP_PCT, STAT_ATK_PCT, STAT_DEF_PCT,
        STAT_CRIT_RATE, STAT_CRIT_DMG,
        STAT_ANOMALY_PROF, STAT_PEN_RATIO,
        STAT_ANOMALY_MASTERY, STAT_IMPACT,
        STAT_ENERGY_REGEN,
        STAT_PHYSICAL_DMG, STAT_FIRE_DMG,
        STAT_ICE_DMG, STAT_ELECTRIC_DMG, STAT_ETHER_DMG,
    ]
}

pub(crate) fn validate_sub_stats(
    main_key: u32,
    sub_keys: &[u32; 4],
    sub_procs: &[u32; 4],
) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
    let allowed_subs = disk_sub_stat_options(main_key);
    let mut keys = Vec::new();
    let mut base = Vec::new();
    let mut add = Vec::new();
    for idx in 0..sub_keys.len() {
        let key = sub_keys[idx];
        if key == 0 || !allowed_subs.contains(&key) || keys.contains(&key) {
            continue;
        }
        let Some(stat_base) = disk_sub_base_value(key) else {
            continue;
        };
        let mut procs = *sub_procs.get(idx).unwrap_or(&0);
        if procs == 0 {
            procs = 1;
        }
        if procs > 6 {
            procs = 6;
        }
        keys.push(key);
        base.push(stat_base);
        add.push(procs);
    }

    let mut total_procs: u32 = add.iter().sum();
    if total_procs > 9 {
        for proc in add.iter_mut().rev() {
            if total_procs <= 9 {
                break;
            }
            let excess = total_procs - 9;
            let reducible = proc.saturating_sub(1);
            let reduce = excess.min(reducible);
            *proc -= reduce;
            total_procs -= reduce;
        }
    }

    (keys, base, add)
}

pub(crate) fn stat_label(state: &AppState, locale: Locale, key: u32) -> String {
    let label = disk_stat_label(locale, key);
    if !label.is_empty() {
        return label;
    }
    let names = load_stat_names(state, locale);
    names
        .get(&key)
        .cloned()
        .unwrap_or_else(|| format!("{} {key}", t(locale, "stat.unknown")))
}
