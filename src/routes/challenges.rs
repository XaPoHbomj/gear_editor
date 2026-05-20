use crate::{
    app_state::{AppState, state_with_active_server},
    auth::{get_session, get_session_mut, html_escape_text, redirect_to_login, set_session},
    i18n::{Locale, locale_from_headers, t},
    player_state::resolve_player_uid,
    zon::{
        ZValue, format_zon_pretty, read_zon, read_zon_verbose, zon_get_entrance_zone_id,
        zon_serialize, zon_set_entrance_zone_id,
    },
};
use axum::{
    extract::{Form, OriginalUri, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::{fs, path::Path as FsPath};

#[derive(Deserialize)]
pub(crate) struct DaShiyuForm {
    shiyu_zone_id: u32,
    deadly_assault_zone_id: u32,
}

#[derive(Deserialize)]
pub(crate) struct ShiyuDetailQuery {
    floor: Option<u32>,
}

pub(crate) async fn da_shiyu_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
    Form(payload): Form<DaShiyuForm>,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let locale = locale_from_headers(&headers);
    let state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&state, session.uid);
    let hadal_path = state
        .state_dir
        .join(format!("player/{uid}/hadal_zone/info"));

    let Some(mut hadal_zon) = read_zon_verbose(&hadal_path) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "common.hadal_zone_not_found"))).into_response();
    };

    zon_set_entrance_zone_id(&mut hadal_zon, 1, payload.shiyu_zone_id);
    zon_set_entrance_zone_id(&mut hadal_zon, 9, payload.deadly_assault_zone_id);

    let serialized = zon_serialize(&hadal_zon);
    session.pending_writes.insert(hadal_path, serialized);
    set_session(session_id, session);

    Redirect::to("/dashboard?tab=shiyu").into_response()
}

fn element_icon_path(element: &str) -> &'static str {
    match element.to_lowercase().as_str() {
        "ice" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconIce.webp",
        "fire" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconFire.webp",
        "electric" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconElectric.webp",
        "ether" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconEther.webp",
        "physical" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconPhysical.webp",
        "wind" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconWind.webp",
        _ => "",
    }
}

fn element_label(locale: Locale, element: &str) -> &str {
    match element.to_lowercase().as_str() {
        "ice" => t(locale, "element.ice"),
        "fire" => t(locale, "element.fire"),
        "electric" => t(locale, "element.electric"),
        "ether" => t(locale, "element.ether"),
        "physical" => t(locale, "element.physical"),
        "wind" => t(locale, "element.wind"),
        _ => "",
    }
}

fn boss_image_base_name(image_path: &str) -> String {
    let file_name = image_path.rsplit('/').next().unwrap_or(image_path).trim();
    file_name
        .strip_suffix(".png")
        .or_else(|| file_name.strip_suffix(".webp"))
        .unwrap_or(file_name)
        .to_string()
}

fn format_with_commas(value: i64) -> String {
    let negative = value < 0;
    let digits = value.abs().to_string();
    let mut out = String::new();
    for (idx, ch) in digits.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    let mut formatted: String = out.chars().rev().collect();
    if negative {
        formatted.insert(0, '-');
    }
    formatted
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

    // Drop icon placeholders that have no renderer in this UI.
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

fn da_total_hp_from_base(base_hp: f64) -> i64 {
    // Observed from DA data: total HP is a stable 8.74x of base HP.
    (base_hp * 8.74).round() as i64
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

pub(crate) fn render_da_panel(state: &AppState, uid: u32, locale: Locale) -> String {
    let dump_dir = state.dump_lang_dir(locale);
    let boss_details_path = dump_dir.join("boss_details.json");
    let zone_info_path = state.asset_dir.join("ZoneInfoTemplateTb.json");

    // Load available DA zone IDs (starting with 69) from ZoneInfoTemplateTb.json
    // Keep full zone_id to support DA variants (e.g., 69038 and 690381 are separate)
    let mut available_zones = std::collections::HashSet::new();
    if let Ok(zone_content) = fs::read_to_string(&zone_info_path) {
        if let Ok(zone_data) = serde_json::from_str::<serde_json::Value>(&zone_content) {
            if let Some(data_array) = zone_data.get("data").and_then(|d| d.as_array()) {
                for entry in data_array {
                    if let Some(zone_id) = entry.get("zone_id").and_then(|z| z.as_u64()) {
                        let zone_id = zone_id as u32;
                        // Include all DA zones (base and variants like 69038 and 690381)
                        if zone_id >= 69000 && zone_id < 70000 {
                            available_zones.insert(zone_id);
                        }
                    }
                }
            }
        }
    }

    // Get currently selected DA zone (exact zone_id, supports variants like 69038 and 690381)
    let hadal_path = state
        .state_dir
        .join(format!("player/{uid}/hadal_zone/info"));
    let selected_da = if let Some(hadal_zon) = read_zon(&hadal_path) {
        if let Some(zone_id) = zon_get_entrance_zone_id(&hadal_zon, 9) {
            // Use exact zone_id to support DA variants (69038, 690381, etc.)
            zone_id
        } else {
            0
        }
    } else {
        0
    };

    let mut cards = String::new();

    if let Ok(content) = fs::read_to_string(&boss_details_path) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
            let mut da_entries: Vec<_> = data
                .as_object()
                .unwrap_or(&serde_json::Map::new())
                .iter()
                .filter_map(|(id_str, details)| {
                    let id = id_str.parse::<u32>().ok()?;
                    // Show DA entries when exact zone exists OR when a 6-digit hotfix variant
                    // maps to an available 5-digit base zone (e.g., 690381 -> 69038).
                    let id_str = id.to_string();
                    let base_id = if id_str.len() == 6 { id / 10 } else { id };
                    if !available_zones.contains(&id) && !available_zones.contains(&base_id) {
                        return None;
                    }

                    let name = details
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or_else(|| t(locale, "common.unknown"));

                    let mut boss_names = Vec::new();
                    if let Some(zones) = details.get("zone").and_then(|z| z.as_object()) {
                        for zone in zones.values() {
                            let zone_name = zone
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("")
                                .trim();
                            if !zone_name.is_empty() {
                                boss_names.push(zone_name.to_string());
                            } else if let Some(layer_room) =
                                zone.get("layer_room").and_then(|r| r.as_object())
                            {
                                // Some DA entries have empty zone.name but still include full boss data in layer_room.
                                for room in layer_room.values() {
                                    if let Some(monster_list) =
                                        room.get("monster_list").and_then(|m| m.as_object())
                                    {
                                        for monster in monster_list.values() {
                                            if let Some(monster_name) =
                                                monster.get("name").and_then(|n| n.as_str())
                                            {
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
                    Some((id, name.to_string(), boss_names))
                })
                .collect();

            // Sort: selected first, then by 5-digit base ID desc, then full ID desc.
            // Example: 69027 comes before 690271.
            da_entries.sort_by(|a, b| {
                let a_selected = a.0 == selected_da;
                let b_selected = b.0 == selected_da;
                if a_selected != b_selected {
                    return if a_selected {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    };
                }

                let a_base = if a.0 >= 100000 { a.0 / 10 } else { a.0 };
                let b_base = if b.0 >= 100000 { b.0 / 10 } else { b.0 };

                b_base.cmp(&a_base).then_with(|| b.0.cmp(&a.0))
            });

            for (id, name, boss_names) in da_entries {
                let is_selected = id == selected_da;
                let style = if is_selected {
                    r#"style="text-decoration: none; color: inherit; border: 2px solid #4c7dff; background: rgba(76, 125, 255, 0.1);""#
                } else {
                    r#"style="text-decoration: none; color: inherit;""#
                };
                let boss_list = boss_names.join("<br>");
                let selected_mark = if is_selected { " ✓" } else { "" };
                cards.push_str(&format!(
                    r#"<a href="/da/{id}" class="card" {style}>
                        <h3>{name}{selected_mark}</h3>
                        <div class="meta">{id_label}: {id}<br>{boss_list}</div>
                    </a>"#,
                    id_label = t(locale, "common.id"),
                ));
            }
        }
    }

    if cards.is_empty() {
        cards.push_str(&format!("<p class=\"meta\">{}</p>", t(locale, "da.no_data")));
    }

    format!("<div class=\"cards\">{cards}</div>")
}

pub(crate) fn render_shiyu_panel(state: &AppState, uid: u32, locale: Locale) -> String {
    let dump_dir = state.dump_lang_dir(locale);
    let shiyu_details_path = dump_dir.join("shiyu_details.json");
    let zone_info_path = state.asset_dir.join("ZoneInfoTemplateTb.json");

    // Load available Shiyu zone IDs (starting with 62) from ZoneInfoTemplateTb.json
    let mut available_zones = std::collections::HashSet::new();
    if let Ok(zone_content) = fs::read_to_string(&zone_info_path) {
        if let Ok(zone_data) = serde_json::from_str::<serde_json::Value>(&zone_content) {
            if let Some(data_array) = zone_data.get("data").and_then(|d| d.as_array()) {
                for entry in data_array {
                    if let Some(zone_id) = entry.get("zone_id").and_then(|z| z.as_u64()) {
                        let zone_id = zone_id as u32;
                        let zone_id_str = zone_id.to_string();
                        let is_shiyu_5_digit =
                            zone_id_str.len() == 5 && zone_id_str.starts_with("62");
                        let is_shiyu_6_digit =
                            zone_id_str.len() == 6 && zone_id_str.starts_with("62");
                        if is_shiyu_5_digit || is_shiyu_6_digit {
                            available_zones.insert(zone_id);
                        }
                    }
                }
            }
        }
    }

    // Get currently selected Shiyu zone
    let hadal_path = state
        .state_dir
        .join(format!("player/{uid}/hadal_zone/info"));
    let selected_shiyu = if let Some(hadal_zon) = read_zon(&hadal_path) {
        zon_get_entrance_zone_id(&hadal_zon, 1).unwrap_or(0)
    } else {
        0
    };

    let mut cards = String::new();

    if let Ok(content) = fs::read_to_string(&shiyu_details_path) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
            let mut shiyu_entries: Vec<_> = data
                .as_object()
                .unwrap_or(&serde_json::Map::new())
                .iter()
                .filter_map(|(id_str, details)| {
                    let id = id_str.parse::<u32>().ok()?;
                    // Filter: only show Shiyu nodes that start with 62 and have a matching zone entry.
                    if !id_str.starts_with("62") || !available_zones.contains(&id) {
                        return None;
                    }

                    let name = details
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or_else(|| t(locale, "common.unknown"));

                    let max_stage = shiyu_max_stage(details);
                    let last_floor_zones = shiyu_stage_zones(details, max_stage);
                    let boss_names = shiyu_floor_boss_names(&last_floor_zones);

                    Some((id, name.to_string(), boss_names))
                })
                .collect();

            // Sort: selected first, then by 5-digit base ID desc, then full ID desc.
            shiyu_entries.sort_by(|a, b| {
                let a_selected = a.0 == selected_shiyu;
                let b_selected = b.0 == selected_shiyu;
                if a_selected != b_selected {
                    return if a_selected {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Greater
                    };
                }

                let a_base = if a.0 >= 100000 { a.0 / 10 } else { a.0 };
                let b_base = if b.0 >= 100000 { b.0 / 10 } else { b.0 };

                b_base.cmp(&a_base).then_with(|| b.0.cmp(&a.0))
            });

            for (id, name, boss_names) in shiyu_entries {
                let is_selected = id == selected_shiyu;
                let style = if is_selected {
                    r#"style="text-decoration: none; color: inherit; border: 2px solid #4c7dff; background: rgba(76, 125, 255, 0.1);""#
                } else {
                    r#"style="text-decoration: none; color: inherit;""#
                };
                let selected_mark = if is_selected { " ✓" } else { "" };
                let boss_list = boss_names.join("<br>");
                cards.push_str(&format!(
                    r#"<a href="/shiyu/{id}" class="card" {style}>
                        <h3>{name}{selected_mark}</h3>
                        <div class="meta">{id_label}: {id}<br>{boss_list}</div>
                    </a>"#,
                    id_label = t(locale, "common.id"),
                ));
            }
        }
    }

    if cards.is_empty() {
        cards.push_str(&format!("<p class=\"meta\">{}</p>", t(locale, "shiyu.no_data")));
    }

    format!("<div class=\"cards\">{cards}</div>")
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
                                let _weakness = room
                                    .get("monster_weakness")
                                    .and_then(|w| w.as_object())
                                    .unwrap_or(&empty_map);

                                let base_hp_value =
                                    stats.get("hp").and_then(|h| h.as_f64()).unwrap_or(0.0);
                                let hp = format_with_commas(da_total_hp_from_base(base_hp_value));
                                let base_hp = format_with_commas(base_hp_value.round() as i64);
                                let atk =
                                    format_stat_value(stats.get("attack").and_then(|a| a.as_f64()), locale);
                                let def = format_stat_value(
                                    stats.get("defence").and_then(|d| d.as_f64()), locale,
                                );
                                let stun =
                                    format_stat_value(stats.get("stun").and_then(|st| st.as_f64()), locale);

                                // Correct field mapping: element == 1 means weakness, element == -1 means resistance.
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

                        // Render selectable buffs (only once per DA, from first zone)
                        if buff_cards.is_empty() && !selectable_buffs.is_empty() {
                            let mut buffs_html = String::new();
                            for (_, buff) in selectable_buffs.iter().take(3) {
                                let buff_title =
                                    buff.get("title").and_then(|t| t.as_str()).unwrap_or_else(|| t(locale, "common.buff"));

                                let buff_desc = buff
                                    .get("desc")
                                    .and_then(|d| d.as_str())
                                    .unwrap_or_else(|| t(locale, "common.no_description"));

                                // Remove color tags from description for better readability
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
  <a href="/dashboard?tab=da" class="back">{2}</a>
    <form method="post" action="/da/{1}/select" style="margin: 0;">
        <button type="submit" style="padding: 10px 18px; background: #4c7dff; color: #fff; border: none; border-radius: 8px; font-weight: 600; cursor: pointer; font-size: 14px;">
            {3}
        </button>
    </form>
</header>
<div class="container">
  <h1>{0} {4} {1}</h1>
  <div class="cards">
    {5}
    {6}
  </div>
</div>
</body>
</html>"#,
                da_name, id, t(locale, "da.back"), t(locale, "da.select"), t(locale, "common.id"), buff_cards, boss_cards,
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
                        .unwrap_or_else(|| format!("{} {}", t(locale, "shiyu.fight"), fight_index));
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
                        monster_cards.push_str(&shiyu_render_monster_card(
                            monster,
                            room_weakness,
                            locale,
                        ));
                    }

                    let room_buff_html = if is_new_style && selected_floor == 5 {
                        let buffs = zone
                            .get("layer_buff")
                            .and_then(|b| b.as_object())
                            .cloned()
                            .unwrap_or_default();
                        let mut html = String::new();
                        for (_, buff) in buffs.iter() {
                            let buff_title =
                                buff.get("title").and_then(|v| v.as_str()).unwrap_or_else(|| t(locale, "common.buff"));
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
  <title>{0} - Gear Editor</title>
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
  <a href="/dashboard?tab=shiyu" class="back">{2}</a>
    <form method="post" action="/shiyu/{1}/select" style="margin: 0;">
        <button type="submit" style="padding: 10px 18px; background: #4c7dff; color: #fff; border: none; border-radius: 8px; font-weight: 600; cursor: pointer; font-size: 14px;">
            {3}
        </button>
    </form>
</header>
<div class="container">
  <h1>{0} {4} {1}</h1>
    <div class="floor-tabs">{5}</div>
  <div class="cards">
        {6}
        {7}
  </div>
</div>
</body>
</html>"#,
                shiyu_name, id, t(locale, "shiyu.back"), t(locale, "shiyu.select"), t(locale, "common.id"), tab_html, buff_cards, fight_cards,
                lang = locale.lang_attr()
                );

                return Html(html).into_response();
            }
        }
    }

    Html(t(locale, "shiyu.not_found").to_string()).into_response()
}

pub(crate) async fn shiyu_select(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<u32>,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let locale = locale_from_headers(&headers);
    let state = state_with_active_server(&state, &headers);
    if !is_zone_available_for_prefix(&state.asset_dir, id, "62") {
        return (
            StatusCode::BAD_REQUEST,
            Html(t(locale, "shiyu.zone_unavailable")),
        )
            .into_response();
    }
    let uid = resolve_player_uid(&state, session.uid);
    let hadal_file = state
        .state_dir
        .join(format!("player/{uid}/hadal_zone/info"));

    let mut hadal_zon = match read_zon_verbose(&hadal_file) {
        Some(z) => z,
            None => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Html(t(locale, "common.failed_read_zone").to_string()),
                )
                    .into_response();
            }
        };

        zon_set_entrance_zone_id(&mut hadal_zon, 1, id);
        let zon_content = format_zon_pretty(&zon_serialize(&hadal_zon));
        if let Err(err) = fs::write(&hadal_file, zon_content) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html(format!("{}: {}", t(locale, "common.failed_save_zone"), err)),
            )
                .into_response();
        }

    Redirect::to("/dashboard?tab=shiyu").into_response()
}

pub(crate) async fn da_select(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<u32>,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let locale = locale_from_headers(&headers);
    let state = state_with_active_server(&state, &headers);
    if !is_zone_available_for_prefix(&state.asset_dir, id, "69") {
        return (
            StatusCode::BAD_REQUEST,
            Html(t(locale, "da.zone_unavailable")),
        )
            .into_response();
    }
    let uid = resolve_player_uid(&state, session.uid);

    // Read the hadal_zone/info file
    let hadal_file = state
        .state_dir
        .join(format!("player/{uid}/hadal_zone/info"));

    let mut hadal_zon = match read_zon_verbose(&hadal_file) {
        Some(z) => z,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html(t(locale, "common.failed_read_zone").to_string()),
            )
                .into_response();
        }
    };

    let zone_id_to_set = id;

    // Update the entrance (id=9) zone_id
    let mut modified = false;
    if let ZValue::Object(fields) = &mut hadal_zon {
        if let Some((_, ZValue::Array(entrances))) =
            fields.iter_mut().find(|(k, _)| k == "entrances")
        {
            for entry in entrances {
                if let ZValue::Object(items) = entry {
                    let mut is_target = false;
                    for (k, v) in items.iter() {
                        if k == "id" {
                            if let ZValue::Number(num) = v {
                                if *num as u32 == 9 {
                                    is_target = true;
                                }
                            }
                        }
                    }
                    if is_target {
                        // Update zone_id
                        if let Some((_, v)) = items.iter_mut().find(|(k, _)| k == "zone_id") {
                            *v = ZValue::Number(zone_id_to_set as i64);
                            modified = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    if modified {
        // Apply immediately so DA selection does not require pressing "Apply Changes".
        let zon_content = format_zon_pretty(&zon_serialize(&hadal_zon));
        if let Err(err) = fs::write(&hadal_file, zon_content) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html(format!("{}: {}", t(locale, "common.failed_save_zone"), err)),
            )
                .into_response();
        }
    }

    Redirect::to("/dashboard?tab=da").into_response()
}

fn is_zone_available_for_prefix(asset_dir: &FsPath, zone_id: u32, prefix: &str) -> bool {
    let zone_info_path = asset_dir.join("ZoneInfoTemplateTb.json");
    let Ok(zone_content) = fs::read_to_string(zone_info_path) else {
        return false;
    };
    let Ok(zone_data) = serde_json::from_str::<JsonValue>(&zone_content) else {
        return false;
    };
    let Some(data_array) = zone_data.get("data").and_then(|d| d.as_array()) else {
        return false;
    };

    for entry in data_array {
        let Some(value) = entry.get("zone_id").and_then(|z| z.as_u64()) else {
            continue;
        };
        let value = value as u32;
        let id_str = value.to_string();
        if !id_str.starts_with(prefix) {
            continue;
        }

        // Allow exact match and 6-digit hotfix variants mapping to 5-digit base.
        if value == zone_id {
            return true;
        }
        if zone_id >= 100000 && zone_id / 10 == value {
            return true;
        }
    }

    false
}
