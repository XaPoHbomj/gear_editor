use crate::{
    app_state::AppState,
    domain::discs::stat_label,
    i18n::{Locale, t},
    zon::{read_zon, zon_get_number},
};
use std::{
    fs,
    path::{Path as FsPath, PathBuf},
};

pub(crate) fn read_next_uid(dir: &FsPath) -> Option<u32> {
    let next_path = dir.join("next");
    if let Ok(value) = fs::read_to_string(&next_path) {
        if let Ok(parsed) = value.trim().parse::<u32>() {
            return Some(parsed);
        }
    }

    let mut max_id = 0u32;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else {
                continue;
            };
            let Ok(id) = name.trim().parse::<u32>() else {
                continue;
            };
            if id > max_id {
                max_id = id;
            }
        }
    }

    Some(max_id.saturating_add(1).max(1))
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
) -> String {
    let mut rows = String::new();
    for idx in 0..4 {
        let (mut key, _base, add) = sub_props.get(idx).copied().unwrap_or((0, 0, 0));
        if key == 0 {
            if let Some(first) = options.first() {
                key = *first;
            }
        }
        rows.push_str(&format!(
            "<div><label>{}</label><select name=\"sub_key_{}\">{}</select></div>",
            t(locale, "disc.key"),
            idx + 1,
            render_stat_select_options(state, options, key, locale)
        ));
        rows.push_str(&format!(
            "<div><label>{}</label><input name=\"sub_proc_{}\" type=\"number\" min=\"0\" max=\"6\" value=\"{}\" /></div>",
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

pub(crate) fn resolve_player_uid(state: &AppState, account_uid: i32) -> u32 {
    let account_path = state.state_dir.join(format!("account/{account_uid}"));

    if let Some(account_zon) = read_zon(&account_path) {
        if let Some(player_uid) = zon_get_number(&account_zon, "player_uid") {
            return player_uid as u32;
        }
    }

    let direct_path = state.state_dir.join(format!("player/{account_uid}"));
    if direct_path.exists() {
        return account_uid as u32;
    }

    let player_root = state.state_dir.join("player");
    if let Ok(entries) = fs::read_dir(player_root) {
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                if let Ok(uid) = name.parse::<u32>() {
                    return uid;
                }
            }
        }
    }

    account_uid.max(1) as u32
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
