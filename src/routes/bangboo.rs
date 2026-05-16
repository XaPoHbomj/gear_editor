use crate::{
    app_state::{AppState, state_with_active_server},
    auth::{get_session, get_session_mut, html_escape_attr, redirect_to_login, set_session},
    data::hakushin::{load_hakushin_data, to_asset_url},
    i18n::{Locale, locale_from_headers, t},
    player_state::{resolve_item_path, resolve_player_uid},
    utils::svg_data_uri,
    zon::{
        format_zon_pretty, read_zon, zon_get_number, zon_get_skill_levels, zon_serialize,
        zon_set_number, zon_set_skill_levels, ZValue,
    },
};
use axum::{
    extract::{Form, OriginalUri, Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::{collections::HashMap, fs};

#[derive(Deserialize)]
pub(crate) struct BangbooUpdateForm {
    level: u32,
    rank: u32,
    skill_manual: u32,
    skill_passive: u32,
    skill_qte: u32,
    skill_aid: u32,
}

pub(crate) async fn bangboo_edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(bangboo_uid): Path<u32>,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&state, session.uid);
    let bangboo_path = resolve_item_path(&state.state_dir, uid, "buddy", bangboo_uid);
    let locale = locale_from_headers(&headers);

    let Some(bangboo_zon) = read_zon(&bangboo_path) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "bangboo.not_found"))).into_response();
    };

    let level = zon_get_number(&bangboo_zon, "level").unwrap_or(1) as u32;
    let rank = zon_get_number(&bangboo_zon, "rank").unwrap_or(1) as u32;
    let skill_levels = zon_get_skill_levels(&bangboo_zon);

    let hakushin = load_hakushin_data(&state, locale);
    let bangboo_name = hakushin
        .bangboos
        .get(&bangboo_uid)
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| format!("{} {bangboo_uid}", t(locale, "fallback.bangboo")));
    let bangboo_img = hakushin
        .bangboos
        .get(&bangboo_uid)
        .and_then(|entry| entry.image_local.as_deref())
        .map(to_asset_url)
        .unwrap_or_else(|| svg_data_uri(&bangboo_name));

    let lang = locale.lang_attr();
    let edit_title = t(locale, "bangboo.edit");
    let level_label = t(locale, "bangboo.level");
    let rank_label = t(locale, "bangboo.rank");
    let skill_label = t(locale, "bangboo.skill_levels");
    let save_label = t(locale, "bangboo.save");
    let uid_label = t(locale, "uid");

    let body = format!(
        r#"<!doctype html>
<html lang="{lang}">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{edit_title}</title>
    <style>
        body {{ font-family: system-ui, sans-serif; margin: 0; background: #0f1115; color: #e6e6e6; }}
        .container {{ padding: 24px; max-width: 900px; margin: 0 auto; }}
        input {{ width: 100%; box-sizing: border-box; padding: 8px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
        label {{ display: block; margin: 12px 0 6px; font-size: 12px; color: #9aa4b2; }}
        button {{ margin-top: 16px; padding: 10px 14px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }}
        .row {{ display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }}
        .row > * {{ min-width: 0; }}
        .hero {{ display: flex; gap: 16px; align-items: center; margin-bottom: 16px; }}
        .hero img {{ width: 120px; height: 120px; border-radius: 12px; object-fit: cover; object-position: top; border: 1px solid #2a3140; background: #0f1115; }}
        .hero h1 {{ margin: 0; }}
        .meta {{ color: #9aa4b2; font-size: 12px; }}
        @media (max-width: 768px) {{
            .container {{ padding: 14px; }}
            .hero {{ flex-direction: column; align-items: flex-start; }}
            .hero img {{ width: 100%; max-width: 240px; height: auto; aspect-ratio: 1 / 1; }}
            .row {{ grid-template-columns: 1fr; }}
            button {{ width: 100%; }}
        }}
    </style>
</head>
<body>
    <div class="container">
        <div class="hero">
            <img src="{bangboo_img}" alt="{bangboo_name}" />
            <div>
                <h1>{edit_title} {bangboo_name}</h1>
                <div class="meta">{uid_label} {bangboo_uid}</div>
            </div>
        </div>
        <form method="post">
            <div class="row">
                <div>
                    <label>{level_label}</label>
                    <input name="level" type="number" min="1" value="{level}" />
                </div>
                <div>
                    <label>{rank_label}</label>
                    <input name="rank" type="number" min="0" value="{rank}" />
                </div>
            </div>

            <h3>{skill_label}</h3>
            <div class="row">
                {skills}
            </div>

            <button type="submit">{save_label}</button>
        </form>
    </div>
</body>
</html>"#,
        lang = lang,
        bangboo_uid = bangboo_uid,
        bangboo_name = html_escape_attr(&bangboo_name),
        bangboo_img = html_escape_attr(&bangboo_img),
        level = level,
        rank = rank,
        skills = render_bangboo_skill_inputs(locale, &skill_levels),
        edit_title = edit_title,
        level_label = level_label,
        rank_label = rank_label,
        skill_label = skill_label,
        save_label = save_label,
        uid_label = uid_label,
    );

    Html(body).into_response()
}

pub(crate) async fn bangboo_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(bangboo_uid): Path<u32>,
    original_uri: OriginalUri,
    Form(payload): Form<BangbooUpdateForm>,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&state, session.uid);
    let bangboo_path = resolve_item_path(&state.state_dir, uid, "buddy", bangboo_uid);
    let locale = locale_from_headers(&headers);

    let Some(mut bangboo_zon) = read_zon(&bangboo_path) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "bangboo.not_found"))).into_response();
    };

    zon_set_number(&mut bangboo_zon, "level", payload.level as i64);
    zon_set_number(&mut bangboo_zon, "rank", payload.rank as i64);

    let mut skill_levels = vec![
        ("manual", payload.skill_manual),
        ("passive", payload.skill_passive),
        ("qte", payload.skill_qte),
        ("aid", payload.skill_aid),
    ];
    zon_set_skill_levels(&mut bangboo_zon, &mut skill_levels);

    let serialized = zon_serialize(&bangboo_zon);
    session.pending_writes.insert(bangboo_path, serialized);
    set_session(session_id, session);

    Redirect::to("/dashboard?tab=bangboos").into_response()
}

pub(crate) fn render_bangboo_cards(state: &AppState, uid: u32, locale: Locale) -> String {
    let bangboo_dir = state.state_dir.join(format!("player/{uid}/buddy"));
    let hakushin = load_hakushin_data(state, locale);

    let mut cards = String::new();
    if let Ok(entries) = fs::read_dir(&bangboo_dir) {
        for entry in entries.flatten() {
            let Some(file_name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            let bangboo_uid = match file_name
                .strip_suffix(".zon")
                .unwrap_or(&file_name)
                .parse::<u32>()
            {
                Ok(value) if value > 0 => value,
                _ => continue,
            };

            let bangboo = read_zon(&entry.path());
            let level = bangboo
                .as_ref()
                .and_then(|v| zon_get_number(v, "level"))
                .unwrap_or(0);
            let rank = bangboo
                .as_ref()
                .and_then(|v| zon_get_number(v, "rank"))
                .unwrap_or(0);

            let name = hakushin
                .bangboos
                .get(&bangboo_uid)
                .map(|entry| entry.name.clone())
                .unwrap_or_else(|| format!("{} {bangboo_uid}", t(locale, "fallback.bangboo")));
            let img = hakushin
                .bangboos
                .get(&bangboo_uid)
                .and_then(|entry| entry.image_local.as_deref())
                .map(to_asset_url)
                .unwrap_or_else(|| svg_data_uri(&name));

            let uid_label = t(locale, "uid");
            let level_label = t(locale, "bangboo.level");
            let rank_label = t(locale, "bangboo.rank");

            cards.push_str(&format!(
                "<a class=\"card\" href=\"/bangboo/{uid}\"><img class=\"thumb\" src=\"{img}\" alt=\"{name}\" /><span class=\"pill\">{uid_label} {uid}</span><h3>{name}</h3><div class=\"meta\">{level_label} {level}</div><div class=\"meta\">{rank_label} {rank}</div></a>",
                uid = bangboo_uid,
                name = html_escape_attr(&name),
                level = level,
                rank = rank,
                img = html_escape_attr(&img),
                uid_label = uid_label,
                level_label = level_label,
                rank_label = rank_label,
            ));
        }
    }

    if cards.is_empty() {
        cards.push_str(&format!(
            "<p class=\"meta\">{}</p>",
            t(locale, "bangboo.no_bangboos")
        ));
    }

    let add_panel = render_add_bangboo_panel(locale);
    format!("{add_panel}<div class=\"cards\">{cards}</div>")
}

fn render_bangboo_skill_inputs(locale: Locale, skill_levels: &HashMap<String, u32>) -> String {
    let mut html = String::new();
    for (key, label_key) in [
        ("manual", "skill.manual"),
        ("passive", "skill.passive"),
        ("qte", "skill.qte"),
        ("aid", "skill.aid"),
    ] {
        let value = skill_levels.get(key).copied().unwrap_or(1);
        let label = t(locale, label_key);
        html.push_str(&format!(
            "<div><label>{label}</label><input name=\"skill_{key}\" type=\"number\" min=\"1\" value=\"{value}\" /></div>",
        ));
    }

    html
}

fn render_add_bangboo_panel(locale: Locale) -> String {
    format!(
        "<div class=\"panel\"><h3>{}</h3><div style=\"display:flex; gap:8px;\"><form method=\"post\" action=\"/bangboo/add-all\" style=\"margin:0;\"><button type=\"submit\">{}</button></form></div></div>",
        t(locale, "nav.bangboos"),
        t(locale, "bangboo.add_all"),
    )
}

pub(crate) async fn bangboo_add_all(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let locale = locale_from_headers(&headers);
    let active_state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&active_state, session.uid);
    let buddy_dir = active_state.state_dir.join(format!("player/{uid}/buddy"));

    let base_path = active_state.asset_dir.join("BuddyBaseTemplateTb.json");
    let base_data = match fs::read_to_string(&base_path) {
        Ok(d) => d,
        Err(_) => {
            return Html(format!("Failed to read BuddyBaseTemplateTb.json")).into_response();
        }
    };
    let base_json: JsonValue = match serde_json::from_str(&base_data) {
        Ok(v) => v,
        Err(_) => {
            return Html(format!("Failed to parse BuddyBaseTemplateTb.json")).into_response();
        }
    };

    let mut existing_ids = HashMap::new();
    if let Ok(entries) = fs::read_dir(&buddy_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str().map(|s| s.to_string()) {
                let id = name.strip_suffix(".zon").unwrap_or(&name).parse::<u32>().unwrap_or(0);
                if id > 0 {
                    existing_ids.insert(id, true);
                }
            }
        }
    }

    let _ = fs::create_dir_all(&buddy_dir);

    if let Some(items) = base_json.get("data").and_then(|v| v.as_array()) {
        for item in items {
            let Some(template_id) = item.get("id").and_then(|v| v.as_u64()).map(|v| v as u32) else {
                continue;
            };
            if template_id >= 55000 {
                continue;
            }
            if existing_ids.contains_key(&template_id) {
                continue;
            }

            let buddy = ZValue::Object(vec![
                ("level".to_string(), ZValue::Number(60)),
                ("exp".to_string(), ZValue::Number(0)),
                ("rank".to_string(), ZValue::Number(6)),
                ("star".to_string(), ZValue::Number(1)),
                (
                    "skill_type_level".to_string(),
                    ZValue::Array(vec![
                        ZValue::Object(vec![
                            ("type".to_string(), ZValue::Enum("manual".to_string())),
                            ("level".to_string(), ZValue::Number(8)),
                        ]),
                        ZValue::Object(vec![
                            ("type".to_string(), ZValue::Enum("passive".to_string())),
                            ("level".to_string(), ZValue::Number(5)),
                        ]),
                        ZValue::Object(vec![
                            ("type".to_string(), ZValue::Enum("qte".to_string())),
                            ("level".to_string(), ZValue::Number(8)),
                        ]),
                        ZValue::Object(vec![
                            ("type".to_string(), ZValue::Enum("aid".to_string())),
                            ("level".to_string(), ZValue::Number(8)),
                        ]),
                    ]),
                ),
                ("is_favorite".to_string(), ZValue::Bool(false)),
            ]);

            let serialized = format_zon_pretty(&zon_serialize(&buddy));
            let buddy_path = buddy_dir.join(template_id.to_string());
            if let Err(err) = fs::write(&buddy_path, serialized) {
                return Html(format!("{}: {}", t(locale, "disc.failed_create"), err)).into_response();
            }
        }
    }

    Redirect::to("/dashboard?tab=bangboos").into_response()
}
