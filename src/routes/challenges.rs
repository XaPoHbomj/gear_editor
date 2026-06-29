use crate::{
    app_state::AppState,
    auth::{get_session, html_escape_text, redirect_to_login},
    i18n::{Locale, locale_from_headers, t},
    player_state::resolve_player_uid,
    zon::read_zon,
};
use axum::{
    extract::{OriginalUri, Path, Query, State},
    http::HeaderMap,
    response::{Html, IntoResponse, Redirect},
};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::{fs, path::Path as FsPath};

#[derive(Deserialize)]
pub(crate) struct ShiyuDetailQuery {
    floor: Option<u32>,
}

fn boss_image_base_name(image_path: &str) -> String {
    if let Some(last) = image_path.rsplit('/').next() {
        last.trim_end_matches(".webp").trim_end_matches(".png").to_string()
    } else {
        image_path.to_string()
    }
}

fn format_with_commas(n: i64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    let mut count = 0;
    for c in s.chars().rev() {
        if count > 0 && count % 3 == 0 {
            out.push(',');
        }
        out.push(c);
        count += 1;
    }
    out.chars().rev().collect()
}

fn da_rotation_from_id(id: u32) -> u32 {
    let s = id.to_string();
    if s.len() >= 5 {
        s[2..5].parse::<u32>().unwrap_or(0)
    } else {
        0
    }
}

fn da_mode_label(locale: Locale, id: u32) -> String {
    let s = id.to_string();
    if s.len() < 6 {
        return t(locale, "da.mode_normal").to_string();
    }
    let rotation = da_rotation_from_id(id);
    if rotation < 42 {
        return t(locale, "da.mode_normal").to_string();
    }
    let mode_digit = s.chars().nth(5).and_then(|c| c.to_digit(10)).unwrap_or(0);
    if mode_digit == 2 {
        t(locale, "da.mode_hardcore").to_string()
    } else {
        t(locale, "da.mode_normal").to_string()
    }
}

fn format_stat_value(value: Option<f64>, locale: Locale) -> String {
    value
        .map(|v| format_with_commas(v.round() as i64))
        .unwrap_or_else(|| t(locale, "common.na").to_string())
}

fn clean_rich_text(text: &str) -> String {
    text.replace("<color=#FFAF2C>", "")
        .replace("<color=#FFFFFF>", "")
        .replace("<color=#2BAD00>", "")
        .replace("<color=#98EFF0>", "")
        .replace("<color=#2EB6FF>", "")
        .replace("</color>", "")
        .trim()
        .trim_start_matches('.')
        .trim()
        .to_string()
}

fn render_rich_text(text: &str) -> String {
    let mut source = text.trim().trim_start_matches('.').trim().to_string();
    while let Some(start) = source.find("<IconMap:") {
        if let Some(end_rel) = source[start..].find('>') {
            let end = start + end_rel + 1;
            source.replace_range(start..end, "");
        } else {
            break;
        }
    }

    let bytes = source.as_bytes();
    let mut out = String::new();
    let mut idx = 0usize;
    let mut open_spans = 0usize;

    while idx < bytes.len() {
        if source[idx..].starts_with("<color=#") {
            let tag_start = idx + "<color=#".len();
            if let Some(end_rel) = source[tag_start..].find('>') {
                let color = &source[tag_start..tag_start + end_rel];
                let valid = color.len() == 6 && color.chars().all(|c| c.is_ascii_hexdigit());
                if valid {
                    out.push_str(&format!("<span style=\"color: #{};\">", color));
                    open_spans += 1;
                    idx = tag_start + end_rel + 1;
                    continue;
                }
            }
        }

        if source[idx..].starts_with("</color>") {
            if open_spans > 0 {
                out.push_str("</span>");
                open_spans -= 1;
            }
            idx += "</color>".len();
            continue;
        }

        if let Some(ch) = source[idx..].chars().next() {
            if ch == '\n' {
                out.push_str("<br>");
            } else {
                out.push_str(&html_escape_text(&ch.to_string()));
            }
            idx += ch.len_utf8();
        } else {
            break;
        }
    }

    for _ in 0..open_spans {
        out.push_str("</span>");
    }

    out
}

fn da_total_hp_from_base(base_hp: f64, da_id: u32) -> i64 {
    let multiplier = if da_mode_label_raw(da_id) == "hardcore" { 8.74 * 2.5 } else { 8.74 };
    (base_hp * multiplier).round() as i64
}

fn da_mode_label_raw(id: u32) -> &'static str {
    let s = id.to_string();
    if s.len() < 6 {
        return "normal";
    }
    let rotation = da_rotation_from_id(id);
    if rotation < 42 {
        return "normal";
    }
    let mode_digit = s.chars().nth(5).and_then(|c| c.to_digit(10)).unwrap_or(0);
    if mode_digit == 2 { "hardcore" } else { "normal" }
}

fn shiyu_max_stage(shiyu_data: &JsonValue) -> u32 {
    shiyu_data
        .get("zone")
        .and_then(|z| z.as_object())
        .map(|zones| {
            zones
                .values()
                .filter_map(|zone| {
                    zone.get("stage_num")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32)
                })
                .max()
                .unwrap_or(1)
        })
        .unwrap_or(1)
}

fn shiyu_stage_zones(shiyu_data: &JsonValue, floor: u32) -> Vec<(u32, JsonValue)> {
    let mut zones: Vec<(u32, JsonValue)> = shiyu_data
        .get("zone")
        .and_then(|z| z.as_object())
        .map(|zone_map| {
            zone_map
                .iter()
                .filter_map(|(zone_id, zone)| {
                    let stage_num = zone
                        .get("stage_num")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32)?;
                    if stage_num != floor {
                        return None;
                    }
                    let zone_id = zone_id.parse::<u32>().ok()?;
                    Some((zone_id, zone.clone()))
                })
                .collect()
        })
        .unwrap_or_default();
    zones.sort_by_key(|(zone_id, _)| *zone_id);
    zones
}

fn shiyu_floor_boss_names(zones: &[(u32, JsonValue)]) -> Vec<String> {
    let mut names = Vec::new();
    for (_, zone) in zones {
        let Some(rooms) = zone.get("layer_room").and_then(|r| r.as_object()) else {
            continue;
        };
        for room in rooms.values() {
            let Some(monsters) = room.get("monster_list").and_then(|m| m.as_object()) else {
                continue;
            };
            let mut top_boss: Option<(&str, f64)> = None;
            for monster in monsters.values() {
                let name = monster
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_else(|| "Unknown");
                let hp = monster
                    .get("stats")
                    .and_then(|s| s.get("hp"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                if top_boss.map(|(_, best_hp)| hp > best_hp).unwrap_or(true) {
                    top_boss = Some((name, hp));
                }
            }
            if let Some((name, _)) = top_boss {
                names.push(name.to_string());
            }
        }
    }
    names
}

fn shiyu_render_monster_card(
    monster: &JsonValue,
    weakness: Option<&serde_json::Map<String, JsonValue>>,
    locale: Locale,
) -> String {
    let empty_map = serde_json::Map::new();
    let boss_name = monster
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or_else(|| t(locale, "common.unknown"));
    let boss_image = monster
        .get("image")
        .and_then(|img| img.as_str())
        .unwrap_or("");
    let image_html = if !boss_image.is_empty() {
        let base = boss_image_base_name(boss_image);
        let local_src = format!(
            "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/{}.webp",
            base
        );
        format!(
            r#"<img class="boss-inline-thumb" src="{}" alt="{}" style="width: 180px; height: 100%; object-fit: cover; background: #10141d; border-radius: 8px; flex-shrink: 0;" />"#,
            local_src, boss_name
        )
    } else {
        String::new()
    };

    let stats = monster
        .get("stats")
        .and_then(|s| s.as_object())
        .unwrap_or(&empty_map);
    let element = monster
        .get("element")
        .and_then(|e| e.as_object())
        .unwrap_or(&empty_map);
    let weakness = weakness.unwrap_or(&empty_map);

    let hp = format_stat_value(stats.get("hp").and_then(|h| h.as_f64()), locale);
    let atk = format_stat_value(stats.get("attack").and_then(|a| a.as_f64()), locale);
    let def = format_stat_value(stats.get("defence").and_then(|d| d.as_f64()), locale);
    let stun = format_stat_value(stats.get("stun").and_then(|st| st.as_f64()), locale);

    let weakness_badges: Vec<String> = weakness
        .iter()
        .filter_map(|(_, v)| {
            if let Some(elem) = v.as_str() {
                let icon_path = element_icon_path(elem);
                let label = element_label(locale, elem);
                if icon_path.is_empty() || label.is_empty() {
                    None
                } else {
                    Some(format!(
                        r#"<span style="display:inline-flex; align-items:center; gap:6px; margin-right:10px; vertical-align:middle;"><img src="{}" alt="{}" title="{}" style="width: 18px; height: 18px; display:block;" />{}</span>"#,
                        icon_path, label, label, label
                    ))
                }
            } else {
                None
            }
        })
        .collect();
    let weakness_str = weakness_badges.join("");

    let resistance_badges: Vec<String> = element
        .iter()
        .filter_map(|(e, v)| {
            if v.as_i64() == Some(-1) {
                let icon_path = element_icon_path(e);
                let label = element_label(locale, e);
                if icon_path.is_empty() || label.is_empty() {
                    None
                } else {
                    Some(format!(
                        r#"<span style="display:inline-flex; align-items:center; gap:6px; margin-right:10px; vertical-align:middle;"><img src="{}" alt="{}" title="{}" style="width: 18px; height: 18px; display:block;" />{}</span>"#,
                        icon_path, label, label, label
                    ))
                }
            } else {
                None
            }
        })
        .collect();
    let resistance_str = resistance_badges.join("");

    format!(
        r#"<div class="card" style="background: #1b1f2a; padding: 12px; border-radius: 12px; border: 1px solid #232a38; display: flex; gap: 14px; align-items: stretch; justify-content: space-between; flex-wrap: nowrap; min-height: 170px;">
            <div style="flex: 1 1 260px; min-width: 220px;">
                <h4 style="margin: 0 0 10px 0; font-size: 16px;">{boss_name}</h4>
                <div style="font-size: 13px; color: #c7d1e0; line-height: 1.45;">
                    <div style="margin-bottom: 5px;"><strong>{hp_label}:</strong> {hp}</div>
                    <div style="margin-bottom: 5px;"><strong>{atk_label}:</strong> {atk}</div>
                    <div style="margin-bottom: 5px;"><strong>{def_label}:</strong> {def}</div>
                    <div style="margin-bottom: 5px;"><strong>{stun_label}:</strong> {stun}</div>
                    {weakness_html}
                    {resistance_html}
                </div>
            </div>
            {image_html}
        </div>"#,
        boss_name = boss_name,
        hp = hp,
        atk = atk,
        def = def,
        stun = stun,
        hp_label = t(locale, "stat.hp"),
        atk_label = t(locale, "stat.atk"),
        def_label = t(locale, "stat.def"),
        stun_label = t(locale, "stat.stun"),
        weakness_html = if !weakness_str.is_empty() {
            format!(
                "<div style=\"display:flex; align-items:center; gap:8px; margin-top:8px; margin-bottom:6px;\"><strong>{weakness_label}:</strong> <span style=\"display:inline-flex; align-items:center; flex-wrap:wrap; gap:6px;\">{}</span></div>",
                weakness_str, weakness_label = t(locale, "stat.weakness"),
            )
        } else {
            String::new()
        },
        resistance_html = if !resistance_str.is_empty() {
            format!(
                "<div style=\"display:flex; align-items:center; gap:8px; margin-top:8px;\"><strong>{resistance_label}:</strong> <span style=\"display:inline-flex; align-items:center; flex-wrap:wrap; gap:6px;\">{}</span></div>",
                resistance_str, resistance_label = t(locale, "stat.resistance"),
            )
        } else {
            String::new()
        },
        image_html = image_html,
    )
}

fn element_icon_path(element: &str) -> String {
    match element.to_lowercase().as_str() {
        "fire" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconFire.webp".to_string(),
        "ice" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconIce.webp".to_string(),
        "electric" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconElectric.webp".to_string(),
        "ether" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconEther.webp".to_string(),
        "physical" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconPhysical.webp".to_string(),
        "wind" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconWind.webp".to_string(),
        _ => String::new(),
    }
}

fn element_label(locale: Locale, element: &str) -> &str {
    match (locale, element.to_lowercase().as_str()) {
        (_, "fire") => "Fire",
        (_, "ice") => "Ice",
        (_, "electric") => "Electric",
        (_, "ether") => "Ether",
        (Locale::Ru, "physical") => "Физический",
        (_, "physical") => "Physical",
        (_, "wind") => "Wind",
        _ => element,
    }
}

pub(crate) fn render_da_shiyu_status(state: &AppState, _uid: u32, locale: Locale) -> String {
    let root = &state.root_dir;
    let mut out = String::new();

    for server in 1..=3 {
        let config_path = root.join(format!("configs_remielle/server{}/config.zon", server));

        let config_content = match fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(_) => {
                out.push_str(&format!(
                    r#"<div class="panel" style="display:block; margin-bottom:16px;">
                        <h3>{} {}</h3>
                        <p class="meta">{}</p>
                    </div>"#,
                    t(locale, "status.server"), server, t(locale, "status.config_not_found")
                ));
                continue;
            }
        };

        let shiyu_zone = extract_entrance_zone(&config_content, "hadal_zone_scheduled");
        let da_normal_zone = extract_entrance_zone(&config_content, "boss_challenge_normal");
        let da_hard_zone = extract_entrance_zone(&config_content, "boss_challenge_hard");

        let dump_dir = state.dump_lang_dir(locale);
        let shiyu_details_path = dump_dir.join("shiyu_details.json");
        let boss_details_path = dump_dir.join("boss_details.json");

        let shiyu_card = render_status_card(locale, shiyu_zone, "/shiyu/", t(locale, "status.shiyu"), &shiyu_details_path, "shiyu");
        let da_card = render_status_card(locale, da_normal_zone, "/da/", t(locale, "status.da"), &boss_details_path, "da");
        let da_hard_card = render_status_card(locale, da_hard_zone, "/da/", t(locale, "status.da_hardcore"), &boss_details_path, "da");

        out.push_str(&format!(
            r#"<div class="panel" style="display:block; margin-bottom:16px;">
                <h3 style="margin:0 0 14px 0; font-size:16px;">{} {}</h3>
                <div class="cards">{}{}{}</div>
            </div>"#,
            t(locale, "status.server"), server, shiyu_card, da_card, da_hard_card
        ));
    }

    out
}

fn extract_entrance_zone(config: &str, entrance_name: &str) -> u32 {
    let prefix = format!(".{}", entrance_name);
    if let Some(pos) = config.find(&prefix) {
        let after = &config[pos + prefix.len()..];
        if let Some(zone_pos) = after.find(".zone = ") {
            let num_start = zone_pos + ".zone = ".len();
            let rest = &after[num_start..];
            let mut num_str = String::new();
            for c in rest.chars() {
                if c.is_ascii_digit() {
                    num_str.push(c);
                } else {
                    break;
                }
            }
            return num_str.parse::<u32>().unwrap_or(0);
        }
    }
    0
}

fn render_status_card(locale: Locale, zone_id: u32, link_prefix: &str, label: &str, details_path: &FsPath, kind: &str) -> String {
    if zone_id == 0 {
        return format!(
            r#"<a class="card" style="text-decoration:none;color:inherit;opacity:0.5;">
                <h3>{}</h3>
                <div class="meta">{}</div>
            </a>"#,
            label, t(locale, "common.na")
        );
    }

    let (_, boss_names) = lookup_zone_detail(details_path, zone_id, kind, locale);
    let boss_list = boss_names.join("<br>");

    format!(
        r#"<a href="{prefix}{zone}" class="card" style="text-decoration:none;color:inherit;">
            <h3>{label}</h3>
            <div class="meta">{id_label}: {zone}<br>{boss_list}</div>
        </a>"#,
        prefix = link_prefix,
        zone = zone_id,
        label = label,
        id_label = t(locale, "common.id"),
        boss_list = boss_list,
    )
}

fn lookup_zone_detail(details_path: &FsPath, zone_id: u32, kind: &str, locale: Locale) -> (String, Vec<String>) {
    if let Ok(content) = fs::read_to_string(details_path) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(entry) = data.get(zone_id.to_string()) {
                if let Some(name) = entry.get("name").and_then(|n| n.as_str()) {
                    if !name.is_empty() {
                        return (name.to_string(), Vec::new());
                    }
                }
                let mut boss_names = Vec::new();
                if let Some(zones) = entry.get("zone").and_then(|z| z.as_object()) {
                    for zone in zones.values() {
                        if let Some(zone_name) = zone.get("name").and_then(|n| n.as_str()) {
                            if !zone_name.trim().is_empty() {
                                boss_names.push(zone_name.to_string());
                            }
                        }
                        if let Some(layer_room) = zone.get("layer_room").and_then(|r| r.as_object()) {
                            for room in layer_room.values() {
                                if let Some(monster_list) = room.get("monster_list").and_then(|m| m.as_object()) {
                                    for monster in monster_list.values() {
                                        if let Some(monster_name) = monster.get("name").and_then(|n| n.as_str()) {
                                            if !monster_name.trim().is_empty() {
                                                boss_names.push(monster_name.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                return (t(locale, "common.unknown").to_string(), boss_names);
            }
        }
    }
    (t(locale, "common.unknown").to_string(), Vec::new())
}

pub(crate) async fn da_detail(
    State(state): State<AppState>,
    Path(id): Path<u32>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let locale = locale_from_headers(&headers);
    let dump_dir = state.dump_lang_dir(locale);
    let boss_details_path = dump_dir.join("boss_details.json");

    if let Ok(content) = fs::read_to_string(&boss_details_path) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(da_data) = data.get(id.to_string()) {
                let da_name = da_data
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or_else(|| t(locale, "common.unknown"));

                let mut buff_cards = String::new();
                let mut boss_cards = String::new();
                let empty_map = serde_json::Map::new();

                if let Some(zones) = da_data.get("zone").and_then(|z| z.as_object()) {
                    for zone in zones.values() {
                        let selectable_buffs = zone
                            .get("selectable_buff")
                            .and_then(|b| b.as_object())
                            .unwrap_or(&empty_map);

                        let layer_room = zone
                            .get("layer_room")
                            .and_then(|r| r.as_object())
                            .unwrap_or(&empty_map);

                        let layer_buffs = zone
                            .get("layer_buff")
                            .and_then(|b| b.as_object())
                            .unwrap_or(&empty_map);

                        let mut layer_buffs_html = String::new();
                        for buff in layer_buffs.values() {
                            let buff_desc = buff.get("desc").and_then(|d| d.as_str()).unwrap_or("");
                            if clean_rich_text(buff_desc).is_empty() {
                                continue;
                            }
                            layer_buffs_html.push_str(&format!(
                                r#"<div style="padding: 8px 10px; border-radius: 8px; background: #10141d; margin-top: 8px; border-left: 3px solid #4c7dff; font-size: 12px; color: #9aa4b2; line-height: 1.4;">{}</div>"#,
                                render_rich_text(buff_desc)
                            ));
                        }

                        let layer_buffs_section = if layer_buffs_html.is_empty() {
                            String::new()
                        } else {
                            format!(
                                r#"<div style="margin-top: 10px;">
                                    <div style="font-size: 12px; font-weight: 700; color: #8fb0ff; margin-bottom: 2px;">{}</div>
                                    {}
                                </div>"#,
                                t(locale, "da.layer_buffs"),
                                layer_buffs_html
                            )
                        };

                        for room in layer_room.values() {
                            let monster_list = room
                                .get("monster_list")
                                .and_then(|m| m.as_object())
                                .unwrap_or(&empty_map);

                            for monster in monster_list.values() {
                                let boss_name = monster
                                    .get("name")
                                    .and_then(|n| n.as_str())
                                    .unwrap_or_else(|| t(locale, "common.unknown"));

                                let boss_image = monster
                                    .get("image")
                                    .and_then(|img| img.as_str())
                                    .unwrap_or("");

                                let image_html = if !boss_image.is_empty() {
                                    let base = boss_image_base_name(boss_image);
                                    let local_src = format!(
                                        "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/{}.webp",
                                        base
                                    );
                                    format!(
                                        r#"<img class="boss-inline-thumb" src="{}" alt="{}" style="width: 220px; height: 100%; object-fit: cover; background: #10141d; border-radius: 8px; flex-shrink: 0;" />"#,
                                        local_src, boss_name
                                    )
                                } else {
                                    String::new()
                                };

                                let stats = monster
                                    .get("stats")
                                    .and_then(|s| s.as_object())
                                    .unwrap_or(&empty_map);
                                let element = monster
                                    .get("element")
                                    .and_then(|e| e.as_object())
                                    .unwrap_or(&empty_map);

                                let base_hp_value =
                                    stats.get("hp").and_then(|h| h.as_f64()).unwrap_or(0.0);
                                let hp = format_with_commas(da_total_hp_from_base(base_hp_value, id));
                                let base_hp = format_with_commas(base_hp_value.round() as i64);
                                let atk = format_stat_value(stats.get("attack").and_then(|a| a.as_f64()), locale);
                                let def = format_stat_value(stats.get("defence").and_then(|d| d.as_f64()), locale);
                                let stun = format_stat_value(stats.get("stun").and_then(|st| st.as_f64()), locale);

                                let weakness_badges: Vec<String> = element
                                    .iter()
                                    .filter_map(|(e, v)| {
                                        if v.as_i64() == Some(1) {
                                            let icon_path = element_icon_path(e);
                                            let label = element_label(locale, e);
                                            if icon_path.is_empty() || label.is_empty() {
                                                None
                                            } else {
                                                Some(format!(
                                                    r#"<span style="display:inline-flex; align-items:center; gap:6px; margin-right:10px; vertical-align:middle;"><img src="{}" alt="{}" title="{}" style="width: 18px; height: 18px; display:block;" />{}</span>"#,
                                                    icon_path, label, label, label
                                                ))
                                            }
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                let weakness_str = weakness_badges.join("");

                                let resistance_badges: Vec<String> = element
                                    .iter()
                                    .filter_map(|(e, v)| {
                                        if v.as_i64() == Some(-1) {
                                            let icon_path = element_icon_path(e);
                                            let label = element_label(locale, e);
                                            if icon_path.is_empty() || label.is_empty() {
                                                None
                                            } else {
                                                Some(format!(
                                                    r#"<span style="display:inline-flex; align-items:center; gap:6px; margin-right:10px; vertical-align:middle;"><img src="{}" alt="{}" title="{}" style="width: 18px; height: 18px; display:block;" />{}</span>"#,
                                                    icon_path, label, label, label
                                                ))
                                            }
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                let resistance = resistance_badges.join("");

                                boss_cards.push_str(&format!(
                                    r#"<div class="card" style="background: #1b1f2a; padding: 14px; border-radius: 12px; border: 1px solid #232a38; display: flex; gap: 16px; align-items: stretch; justify-content: space-between; flex-wrap: nowrap; min-height: 220px;">
                                        <div style="flex: 1 1 260px; min-width: 240px;">
                                            <h3>{boss_name}</h3>
                                            <div style="margin-top: 12px; font-size: 13px; color: #c7d1e0; line-height: 1.45;">
                                                <div style="margin-bottom: 6px;"><strong>{hp_label}:</strong> {hp}</div>
                                                <div style="margin-bottom: 6px;"><strong>{base_hp_label}:</strong> {base_hp}</div>
                                                <div style="margin-bottom: 6px;"><strong>{atk_label}:</strong> {atk}</div>
                                                <div style="margin-bottom: 6px;"><strong>{def_label}:</strong> {def}</div>
                                                <div style="margin-bottom: 6px;"><strong>{stun_label}:</strong> {stun}</div>
                                                {weakness_html}
                                                {resistance_html}
                                                {layer_buffs_section}
                                            </div>
                                        </div>
                                        {image_html}
                                    </div>"#,
                                    boss_name = boss_name,
                                    hp = hp,
                                    base_hp = base_hp,
                                    atk = atk,
                                    def = def,
                                    stun = stun,
                                    hp_label = t(locale, "stat.hp"),
                                    base_hp_label = t(locale, "stat.base_hp"),
                                    atk_label = t(locale, "stat.atk"),
                                    def_label = t(locale, "stat.def"),
                                    stun_label = t(locale, "stat.stun"),
                                    weakness_html = if !weakness_str.is_empty() {
                                        format!("<div style=\"display:flex; align-items:center; gap:8px; margin-top:8px; margin-bottom:6px;\"><strong>{weakness_label}:</strong> <span style=\"display:inline-flex; align-items:center; flex-wrap:wrap; gap:6px;\">{}</span></div>", weakness_str, weakness_label = t(locale, "stat.weakness"))
                                    } else {
                                        String::new()
                                    },
                                    resistance_html = if !resistance.is_empty() {
                                        format!("<div style=\"display:flex; align-items:center; gap:8px; margin-top:8px;\"><strong>{resistance_label}:</strong> <span style=\"display:inline-flex; align-items:center; flex-wrap:wrap; gap:6px;\">{}</span></div>", resistance, resistance_label = t(locale, "stat.resistance"))
                                    } else {
                                        String::new()
                                    },
                                    layer_buffs_section = layer_buffs_section
                                ));
                            }
                        }

                        if buff_cards.is_empty() && !selectable_buffs.is_empty() {
                            let mut buffs_html = String::new();
                            for (_, buff) in selectable_buffs.iter().take(3) {
                                let buff_title =
                                    buff.get("title").and_then(|t| t.as_str()).unwrap_or_else(|| t(locale, "common.buff"));

                                let buff_desc = buff
                                    .get("desc")
                                    .and_then(|d| d.as_str())
                                    .unwrap_or_else(|| t(locale, "common.no_description"));

                                let clean_desc = clean_rich_text(buff_desc);
                                let rich_desc = render_rich_text(buff_desc);
                                if buff_title.trim().is_empty() && clean_desc.is_empty() {
                                    continue;
                                }
                                let display_title = if buff_title.trim().is_empty() {
                                    t(locale, "common.buff")
                                } else {
                                    buff_title
                                };

                                buffs_html.push_str(&format!(
                                    r#"<div style="margin-bottom: 12px; padding: 10px; background: #121620; border-radius: 8px; border-left: 3px solid #4c7dff;">
                                        <h4 style="margin: 0 0 6px 0; color: #4c7dff;">{}</h4>
                                        <p style="margin: 0; font-size: 12px; color: #9aa4b2; line-height: 1.4;">{}</p>
                                    </div>"#,
                                    display_title, rich_desc
                                ));
                            }

                            if !buffs_html.is_empty() {
                                buff_cards.push_str(&format!(
                                    r#"<div class="card" style="background: #1b1f2a; padding: 14px; border-radius: 12px; border: 1px solid #232a38; grid-column: span 1;">
                                        <h3>{}</h3>
                                        {}
                                    </div>"#,
                                    t(locale, "da.selectable_buffs"),
                                    buffs_html
                                ));
                            }
                        }
                    }
                }

                let html = format!(
                    r#"<!doctype html>
<html lang="{lang}">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{da_name} - Gear Editor</title>
  <style>
    body {{ font-family: system-ui, sans-serif; margin: 0; background: #0f1115; color: #e6e6e6; }}
        header {{ padding: 16px 24px; display: flex; justify-content: space-between; align-items: center; gap: 12px; background: #151a24; }}
    .back {{ padding: 8px 12px; border-radius: 8px; background: #4c7dff; color: #fff; text-decoration: none; font-weight: 600; }}
    .container {{ padding: 20px 24px 40px; }}
    h1 {{ margin: 0 0 20px 0; font-size: 28px; }}
    .cards {{ display: grid; grid-template-columns: 1fr; gap: 16px; }}
    .card {{ background: #1b1f2a; padding: 16px; border-radius: 12px; border: 1px solid #232a38; }}
    .card h3 {{ margin: 0 0 12px 0; font-size: 18px; }}
    .meta {{ color: #9aa4b2; font-size: 12px; }}
        .cards .card img {{ display: block; }}
        .cards .card img {{ max-width: 100%; height: auto; }}
        .cards .card > div:first-child {{ min-width: 0; }}
        @media (max-width: 768px) {{
            header {{ padding: 12px 14px; flex-direction: column; align-items: stretch; }}
            header > a, header > form {{ width: 100%; box-sizing: border-box; }}
            header > a {{ align-self: stretch; text-align: center; }}
            header > form button {{ width: 100%; box-sizing: border-box; }}
            .container {{ padding: 14px; }}
            h1 {{ font-size: 22px; }}
            .card {{ padding: 12px; }}
            .cards .card {{ flex-direction: column; align-items: stretch; }}
            .cards .card img {{ width: min(100%, 320px); max-width: 100%; height: auto; margin: 0 auto; }}
            .cards .card img.boss-inline-thumb {{ width: min(100%, 320px) !important; max-width: 100% !important; height: auto !important; margin: 0 auto; }}
        }}
  </style>
</head>
<body>
<header>
  <a href="/dashboard?tab=status" class="back">{2}</a>
</header>
<div class="container">
  <h1>{0} {3} {1}</h1>
  <div class="cards">
    {4}
    {5}
  </div>
</div>
</body>
</html>"#,
                    da_name, id, t(locale, "status.back"), t(locale, "common.id"), buff_cards, boss_cards,
                    lang = locale.lang_attr()
                    );

                return Html(html).into_response();
            }
        }
    }

    Html(t(locale, "da.not_found").to_string()).into_response()
}

pub(crate) async fn shiyu_detail(
    State(state): State<AppState>,
    Path(id): Path<u32>,
    Query(query): Query<ShiyuDetailQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let locale = locale_from_headers(&headers);
    let dump_dir = state.dump_lang_dir(locale);
    let shiyu_details_path = dump_dir.join("shiyu_details.json");

    if let Ok(content) = fs::read_to_string(&shiyu_details_path) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(shiyu_data) = data.get(id.to_string()) {
                let shiyu_name = shiyu_data
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or_else(|| t(locale, "common.unknown"));
                let max_stage = shiyu_max_stage(shiyu_data);
                let selected_floor = query.floor.unwrap_or(max_stage).clamp(1, max_stage);
                let floor_zones = shiyu_stage_zones(shiyu_data, selected_floor);
                let is_new_style = max_stage == 5;

                let tab_html = (1..=max_stage)
                    .map(|floor| {
                        let active = floor == selected_floor;
                        let href = format!("/shiyu/{id}?floor={floor}");
                        let style = if active {
                            r#"display:inline-flex; align-items:center; justify-content:center; min-width: 48px; padding: 8px 12px; border-radius: 10px; border: 2px solid #8fb0ff; text-decoration: none; font-weight: 800; letter-spacing: 0.2px; color: #ffffff; background: linear-gradient(180deg, #4c7dff 0%, #365fcc 100%); box-shadow: 0 0 0 2px rgba(143,176,255,0.25), 0 6px 16px rgba(0,0,0,0.35);"#
                        } else {
                            r#"display:inline-flex; align-items:center; justify-content:center; min-width: 48px; padding: 8px 12px; border-radius: 10px; border: 1px solid #2a3140; text-decoration: none; font-weight: 600; color: #d5dbea; background: #121620;"#
                        };
                        format!(
                            r#"<a href="{}" style="{}">{} {}</a>"#,
                            href, style, t(locale, "shiyu.floor"), floor
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("");

                let buff_zone = floor_zones
                    .iter()
                    .find(|(_, zone)| {
                        zone.get("layer_room")
                            .and_then(|r| r.as_object())
                            .map(|rooms| rooms.is_empty())
                            .unwrap_or(false)
                    })
                    .map(|(_, zone)| zone.clone())
                    .or_else(|| floor_zones.first().map(|(_, zone)| zone.clone()));

                let mut buff_cards = String::new();
                if !(is_new_style && selected_floor == 5) {
                    if let Some(zone) = buff_zone {
                        let selectable_buffs = zone
                            .get("layer_buff")
                            .and_then(|b| b.as_object())
                            .cloned()
                            .unwrap_or_default();
                        let mut buffs_html = String::new();
                        for (_, buff) in selectable_buffs.iter() {
                            let buff_title = buff
                                .get("title")
                                .and_then(|t| t.as_str())
                                .unwrap_or_else(|| t(locale, "common.buff"));
                            let buff_desc = buff
                                .get("desc")
                                .and_then(|d| d.as_str())
                                .unwrap_or_else(|| t(locale, "common.no_description"));
                            let clean_desc = clean_rich_text(buff_desc);
                            let rich_desc = render_rich_text(buff_desc);
                            if buff_title.trim().is_empty() && clean_desc.is_empty() {
                                continue;
                            }
                            let display_title = if buff_title.trim().is_empty() {
                                t(locale, "common.buff")
                            } else {
                                buff_title
                            };

                            buffs_html.push_str(&format!(
                                r#"<div style="margin-bottom: 12px; padding: 10px; background: #121620; border-radius: 8px; border-left: 3px solid #4c7dff;">
                                <h4 style="margin: 0 0 6px 0; color: #4c7dff;">{}</h4>
                                <p style="margin: 0; font-size: 12px; color: #9aa4b2; line-height: 1.4;">{}</p>
                            </div>"#,
                                display_title, rich_desc
                            ));
                        }

                        if !buffs_html.is_empty() {
                            buff_cards = format!(
                                r#"<div class="card" style="background: #1b1f2a; padding: 14px; border-radius: 12px; border: 1px solid #232a38;">
                                <h3>{}</h3>
                                {}
                            </div>"#,
                                t(locale, "shiyu.buffs"),
                                buffs_html
                            );
                        }
                    }
                }

                let mut fight_cards = String::new();
                let mut fight_index = 1u32;
                let mut ordered_rooms: Vec<(u32, String, JsonValue)> = Vec::new();
                for (zone_id, zone) in &floor_zones {
                    if let Some(rooms) = zone.get("layer_room").and_then(|r| r.as_object()) {
                        let mut room_items: Vec<_> = rooms.iter().collect();
                        room_items.sort_by_key(|(rid, _)| rid.parse::<u32>().unwrap_or(0));
                        for (room_id, room) in room_items {
                            ordered_rooms.push((*zone_id, room_id.clone(), room.clone()));
                        }
                    }
                }

                for (zone_id, _room_id, room) in ordered_rooms {
                    let zone = floor_zones
                        .iter()
                        .find(|(id, _)| *id == zone_id)
                        .map(|(_, zone)| zone.clone())
                        .unwrap_or_else(|| JsonValue::Null);
                    let room_title = zone
                        .get("name")
                        .and_then(|n| n.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .or_else(|| {
                            room.get("name")
                                .and_then(|n| n.as_str())
                                .filter(|s| !s.is_empty())
                                .map(|s| s.to_string())
                        })
                        .unwrap_or_else(|| format!("{} {}", t(locale, "shiyu.room"), fight_index));
                    let waves_num = room.get("waves_num").and_then(|v| v.as_u64()).unwrap_or(0);
                    let room_weakness = room.get("monster_weakness").and_then(|w| w.as_object());

                    let mut monsters: Vec<_> = room
                        .get("monster_list")
                        .and_then(|m| m.as_object())
                        .map(|obj| obj.values().collect::<Vec<_>>())
                        .unwrap_or_default();
                    monsters.sort_by(|a, b| {
                        let a_hp = a
                            .get("stats")
                            .and_then(|s| s.get("hp"))
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        let b_hp = b
                            .get("stats")
                            .and_then(|s| s.get("hp"))
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        b_hp.partial_cmp(&a_hp).unwrap_or(std::cmp::Ordering::Equal)
                    });

                    let mut monster_cards = String::new();
                    for monster in monsters {
                        monster_cards.push_str(&shiyu_render_monster_card(monster, room_weakness, locale));
                    }

                    let room_buff_html = if is_new_style && selected_floor == 5 {
                        let buffs = zone
                            .get("layer_buff")
                            .and_then(|b| b.as_object())
                            .cloned()
                            .unwrap_or_default();
                        let mut html = String::new();
                        for (_, buff) in buffs.iter() {
                            let buff_title = buff
                                .get("title")
                                .and_then(|v| v.as_str())
                                .unwrap_or_else(|| t(locale, "common.buff"));
                            let buff_desc = buff.get("desc").and_then(|v| v.as_str()).unwrap_or("");
                            let clean_desc = clean_rich_text(buff_desc);
                            let rich_desc = render_rich_text(buff_desc);
                            if buff_title.trim().is_empty() && clean_desc.is_empty() {
                                continue;
                            }
                            let display_title = if buff_title.trim().is_empty() {
                                t(locale, "common.buff")
                            } else {
                                buff_title
                            };
                            html.push_str(&format!(
                                r#"<div style="padding: 8px 10px; border-radius: 8px; background: #10141d; margin-top: 8px; border-left: 3px solid #4c7dff;">
                                    <strong style="color: #4c7dff;">{}</strong>
                                    <div style="font-size: 12px; color: #9aa4b2; line-height: 1.4; margin-top: 4px;">{}</div>
                                </div>"#,
                                display_title, rich_desc
                            ));
                        }
                        html
                    } else {
                        String::new()
                    };

                    fight_cards.push_str(&format!(
                        r#"<div class="card" style="background: #1b1f2a; padding: 14px; border-radius: 12px; border: 1px solid #232a38;">
                            <h3 style="margin: 0 0 8px 0;">{room_title}</h3>
                            <div class="meta" style="margin-bottom: 12px;">{waves_label}: {waves_num}</div>
                            {room_buff_html}
                            <div style="display: grid; gap: 12px; margin-top: 12px;">{monster_cards}</div>
                        </div>"#,
                        waves_label = t(locale, "shiyu.waves"),
                    ));
                    fight_index += 1;
                }

                let html = format!(
                    r#"<!doctype html>
<html lang="{lang}">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{shiyu_name} - Gear Editor</title>
  <style>
    body {{ font-family: system-ui, sans-serif; margin: 0; background: #0f1115; color: #e6e6e6; }}
        header {{ padding: 16px 24px; display: flex; justify-content: space-between; align-items: center; gap: 12px; background: #151a24; }}
    .back {{ padding: 8px 12px; border-radius: 8px; background: #4c7dff; color: #fff; text-decoration: none; font-weight: 600; }}
    .container {{ padding: 20px 24px 40px; }}
    h1 {{ margin: 0 0 20px 0; font-size: 28px; }}
    .floor-tabs {{ display: flex; flex-wrap: wrap; gap: 8px; margin: 0 0 20px 0; }}
    .section-title {{ margin: 0 0 12px 0; font-size: 18px; }}
        .cards .card img {{ display: block; }}
        .cards .card img {{ max-width: 100%; height: auto; }}
        .cards .card > div:first-child {{ min-width: 0; }}
        @media (max-width: 768px) {{
            header {{ padding: 12px 14px; flex-direction: column; align-items: stretch; }}
            header > a, header > form {{ width: 100%; box-sizing: border-box; }}
            header > a {{ align-self: stretch; text-align: center; }}
            header > form button {{ width: 100%; box-sizing: border-box; }}
            .container {{ padding: 14px; }}
            h1 {{ font-size: 22px; }}
            .cards {{ gap: 12px; }}
            .cards .card {{ flex-direction: column; align-items: stretch; }}
            .cards .card img {{ width: min(100%, 320px); max-width: 100%; height: auto; margin: 0 auto; }}
            .cards .card img.boss-inline-thumb {{ width: min(100%, 320px) !important; max-width: 100% !important; height: auto !important; margin: 0 auto; }}
        }}
  </style>
</head>
<body>
<header>
  <a href="/dashboard?tab=status" class="back">{1}</a>
</header>
<div class="container">
  <h1>{0} {5} {6}</h1>
  <div class="floor-tabs">
    {2}
  </div>
  <div class="cards">
    {3}
    {4}
  </div>
</div>
</body>
</html>"#,
                    shiyu_name,
                    t(locale, "status.back"),
                    tab_html,
                    buff_cards,
                    fight_cards,
                    t(locale, "common.id"),
                    id,
                    lang = locale.lang_attr()
                );

                return Html(html).into_response();
            }
        }
    }

    Html(t(locale, "shiyu.not_found").to_string()).into_response()
}
