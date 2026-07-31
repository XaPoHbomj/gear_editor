use crate::{
    app_state::AppState,
    domain::discs::stat_label,
    i18n::{Locale, t},
    remielle_save::{self, PlayerSave},
};
use std::{
    fs,
    path::{Path as FsPath, PathBuf},
};

pub(crate) fn load_player_save(state: &AppState, uid: u32) -> Option<PlayerSave> {
    let path = state.state_dir.join(format!("USD_{uid}.bin"));
    let data = fs::read(&path).ok()?;
    remielle_save::decode_player_save(&data)
}

pub(crate) fn save_player_save(state: &AppState, uid: u32, save: &PlayerSave) {
    let path = state.state_dir.join(format!("USD_{uid}.bin"));
    let data = remielle_save::encode_player_save(save);
    let _ = fs::write(&path, &data);
}

pub(crate) fn render_stat_select_options(
    state: &AppState,
    keys: &[u32],
    selected: u32,
    locale: Locale,
) -> String {
    let mut html = String::new();
    let mut unique = keys.to_vec();
    unique.retain(|key| *key > 0);
    if selected > 0 && !unique.contains(&selected) {
        unique.push(selected);
    }
    unique.sort_unstable();
    unique.dedup();
    for key in unique {
        let label = stat_label(state, locale, key);
        html.push_str(&format!(
            "<option value=\"{}\"{}>{}</option>",
            key,
            if key == selected { " selected" } else { "" },
            label
        ));
    }
    html
}

pub(crate) fn render_sub_stat_rows(
    state: &AppState,
    sub_props: &[(u32, u32, u32)],
    options: &[u32],
    _main_key: u32,
    locale: Locale,
    enabled: bool,
) -> String {
    let disabled = if enabled { "" } else { "disabled" };
    let mut rows = String::new();
    for idx in 0..4 {
        let (mut key, _base, add) = sub_props.get(idx).copied().unwrap_or((0, 0, 0));
        if key == 0 {
            if let Some(first) = options.first() {
                key = *first;
            }
        }
        rows.push_str(&format!(
            "<div><label>{}</label><select name=\"sub_key_{}\" {disabled}>{}</select></div>",
            t(locale, "disc.key"),
            idx + 1,
            render_stat_select_options(state, options, key, locale)
        ));
        rows.push_str(&format!(
            "<div><label>{}</label><input name=\"sub_proc_{}\" type=\"number\" min=\"0\" max=\"6\" value=\"{}\" {disabled} /></div>",
            t(locale, "disc.procs"),
            idx + 1,
            add
        ));
    }
    rows
}

pub(crate) fn render_equip_substat_script(
    main_options_by_slot_json: &str,
    sub_options_by_main_json: &str,
    label_map_json: &str,
) -> String {
    let mut script = String::new();
    script.push_str("<script>\n");
    script.push_str("const mainOptionsBySlot = ");
    script.push_str(main_options_by_slot_json);
    script.push_str(";\n");
    script.push_str("const subOptionsByMain = ");
    script.push_str(sub_options_by_main_json);
    script.push_str(";\n");
    script.push_str("const statLabels = ");
    script.push_str(label_map_json);
    script.push_str(";\n");
    script.push_str("const slotSelect = document.getElementById(\"equip_slot\");\n");
    script.push_str("const mainSelect = document.getElementById(\"main_key\");\n");
    script.push_str("const subSelects = Array.from(document.querySelectorAll(\"select[name^='sub_key_']\"));\n\n");

    script.push_str("const renderOptions = (select, keys, selected) => {\n");
    script.push_str("  select.innerHTML = \"\";\n");
    script.push_str("  for (const key of keys) {\n");
    script.push_str("    const option = document.createElement(\"option\");\n");
    script.push_str("    option.value = key;\n");
    script.push_str("    option.textContent = statLabels[key] ?? `Stat ${key}`;\n");
    script.push_str("    if (String(key) === String(selected)) {\n");
    script.push_str("      option.selected = true;\n");
    script.push_str("    }\n");
    script.push_str("    select.appendChild(option);\n");
    script.push_str("  }\n");
    script.push_str("};\n\n");

    script.push_str("const updateSubOptions = () => {\n");
    script.push_str("  const mainKey = Number(mainSelect?.value ?? 0);\n");
    script.push_str("  const subKeys = subOptionsByMain[mainKey] ?? [];\n");
    script.push_str("  const nextValues = [];\n");
    script.push_str("  for (const select of subSelects) {\n");
    script.push_str("    const current = Number(select.value);\n");
    script.push_str("    let chosen = current;\n");
    script.push_str("    if (!subKeys.includes(chosen) || nextValues.includes(chosen)) {\n");
    script.push_str("      chosen = subKeys.find((key) => !nextValues.includes(key));\n");
    script.push_str("      if (chosen === undefined) {\n");
    script.push_str("        chosen = subKeys[0] ?? 0;\n");
    script.push_str("      }\n");
    script.push_str("    }\n");
    script.push_str("    nextValues.push(chosen);\n");
    script.push_str("  }\n");
    script.push_str("  subSelects.forEach((select, idx) => {\n");
    script.push_str("    renderOptions(select, subKeys, nextValues[idx]);\n");
    script.push_str("  });\n");
    script.push_str("};\n\n");

    script.push_str("const updateMainOptions = () => {\n");
    script.push_str("  if (!mainSelect) {\n");
    script.push_str("    return;\n");
    script.push_str("  }\n");
    script.push_str("  if (slotSelect) {\n");
    script.push_str("    const slot = Number(slotSelect.value);\n");
    script.push_str("    const keys = mainOptionsBySlot[slot] ?? [];\n");
    script.push_str("    const current = Number(mainSelect.value);\n");
    script.push_str("    const selected = keys.includes(current) ? current : (keys[0] ?? 0);\n");
    script.push_str("    renderOptions(mainSelect, keys, selected);\n");
    script.push_str("  }\n");
    script.push_str("  updateSubOptions();\n");
    script.push_str("};\n\n");

    script.push_str("if (slotSelect) {\n");
    script.push_str("  slotSelect.addEventListener(\"change\", updateMainOptions);\n");
    script.push_str("}\n");
    script.push_str("if (mainSelect) {\n");
    script.push_str("  mainSelect.addEventListener(\"change\", updateSubOptions);\n");
    script.push_str("}\n");
    script.push_str("for (const select of subSelects) {\n");
    script.push_str("  select.addEventListener(\"change\", updateSubOptions);\n");
    script.push_str("}\n");
    script.push_str("updateMainOptions();\n");
    script.push_str("</script>");
    script
}

pub(crate) fn render_slot_options(locale: Locale, selected: u32) -> String {
    let mut html = String::new();
    for slot in 1..=6 {
        html.push_str(&format!(
            "<option value=\"{}\"{}>{} {}</option>",
            slot,
            if slot == selected { " selected" } else { "" },
            t(locale, "slot"),
            slot
        ));
    }
    html
}

pub(crate) fn parse_slot_value(value: &str) -> u32 {
    value.trim().parse::<u32>().unwrap_or(0)
}

/// Resolve the player UID for this account on the given server, if the account
/// has ever logged in there (a save exists on that server).
pub(crate) fn player_uid_for(state: &AppState, account_uid: i32) -> Option<u32> {
    let base_uid = state.base_player_uid.max(1);
    let map_path = state.state_dir.join("GENERAL_DATA.bin");
    let data = fs::read(&map_path).ok()?;
    if data.len() < 8 || data.len() % 8 != 0 {
        return None;
    }
    let count = data.len() / 8;
    for i in 0..count {
        let start = i * 8;
        let uid_bytes: [u8; 8] = data[start..start + 8].try_into().unwrap();
        let mapped_uid = u64::from_le_bytes(uid_bytes);
        if mapped_uid == account_uid as u64 {
            let candidate = base_uid + i as u32;
            if state
                .state_dir
                .join(format!("USD_{candidate}.bin"))
                .exists()
            {
                return Some(candidate);
            }
        }
    }
    None
}

pub(crate) fn resolve_player_uid(state: &AppState, account_uid: i32) -> Option<u32> {
    player_uid_for(state, account_uid)
}

pub(crate) fn resolve_item_path(state_dir: &FsPath, uid: u32, kind: &str, item_id: u32) -> PathBuf {
    let base = state_dir.join(format!("player/{uid}/{kind}/{item_id}"));
    if base.exists() {
        return base;
    }

    let zon = state_dir.join(format!("player/{uid}/{kind}/{item_id}.zon"));
    if zon.exists() {
        return zon;
    }

    base
}
