use crate::{
    app_state::AppState,
    auth::{get_session, html_escape_attr, redirect_to_login},
    data::hakushin::{load_hakushin_data, to_asset_url},
    i18n::{Locale, locale_from_headers, t},
    player_state::{load_player_save, resolve_player_uid, save_player_save},
    utils::{audit_log, shared_page_css, svg_data_uri},
};
use axum::{
    extract::{Form, OriginalUri, Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
};
use crate::remielle_save::BuddyItemSave;
use crate::zon::zon_parse_entries;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::{collections::HashMap, fs};

#[derive(Deserialize)]
pub(crate) struct BangbooUpdateForm {
    level: u32,
    rank: u32,
    star: u32,
    skill_manual: u32,
    skill_passive: u32,
    skill_qte: u32,
    skill_aid: u32,
}

pub(crate) async fn bangboo_edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(buddy_id): Path<u32>,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state.clone();
    let uid = resolve_player_uid(&state, session.uid);
    let locale = locale_from_headers(&headers);

    let save = load_player_save(&state, uid).unwrap_or_default();
    let Some(buddy) = save.buddy.iter().find(|b| b.id == buddy_id) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "bangboo.not_found"))).into_response();
    };

    let level = buddy.level;
    let rank = buddy.rank;
    let star = buddy.star;
    let skill_levels = &buddy.skill_levels;

    let hakushin = load_hakushin_data(&state, locale);
    let bangboo_name = hakushin
        .bangboos
        .get(&buddy_id)
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| format!("{} {buddy_id}", t(locale, "fallback.bangboo")));
    let bangboo_img = hakushin
        .bangboos
        .get(&buddy_id)
        .and_then(|entry| entry.image_local.as_deref())
        .map(to_asset_url)
        .unwrap_or_else(|| svg_data_uri(&bangboo_name));

    let lang = locale.lang_attr();
    let edit_title = t(locale, "bangboo.edit");
    let level_label = t(locale, "bangboo.level");
    let rank_label = t(locale, "bangboo.rank");
    let star_label = "Star";
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
    <style>{shared_css}</style>
</head>
<body>
    <div class="container">
        <div class="hero">
            <img src="{bangboo_img}" alt="{bangboo_name}" />
            <div>
                <h1>{edit_title} {bangboo_name}</h1>
                <div class="meta">{uid_label} {buddy_id}</div>
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
                <div>
                    <label>{star_label}</label>
                    <input name="star" type="number" min="0" max="5" value="{star}" />
                </div>
            </div>

            <h3>{skill_label}</h3>
            <div class="row">
                {skills}
            </div>

            <div class="form-actions">
                <a href="/dashboard?tab=bangboos" class="back">{back_label}</a>
                <button type="submit">{save_label}</button>
            </div>
        </form>
    </div>
</body>
</html>"#,
        lang = lang,
        buddy_id = buddy_id,
        bangboo_name = html_escape_attr(&bangboo_name),
        bangboo_img = html_escape_attr(&bangboo_img),
        level = level,
        rank = rank,
        star = star,
        skills = render_bangboo_skill_inputs(locale, skill_levels),
        edit_title = edit_title,
        level_label = level_label,
        rank_label = rank_label,
        star_label = star_label,
        skill_label = skill_label,
        save_label = save_label,
        uid_label = uid_label,
        back_label = t(locale, "bangboo.back"),
        shared_css = shared_page_css(),
    );

    Html(body).into_response()
}

pub(crate) async fn bangboo_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(buddy_id): Path<u32>,
    original_uri: OriginalUri,
    Form(payload): Form<BangbooUpdateForm>,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state.clone();
    let uid = resolve_player_uid(&state, session.uid);
    let locale = locale_from_headers(&headers);

    let mut save = load_player_save(&state, uid).unwrap_or_default();

    let Some(buddy) = save.buddy.iter_mut().find(|b| b.id == buddy_id) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "bangboo.not_found"))).into_response();
    };

    buddy.level = payload.level;
    buddy.rank = payload.rank;
    buddy.star = payload.star;
    buddy.skill_levels = vec![
        payload.skill_manual,
        payload.skill_passive,
        payload.skill_qte,
        payload.skill_aid,
    ];

    save_player_save(&state, uid, &save);

    Redirect::to("/dashboard?tab=bangboos").into_response()
}

pub(crate) fn render_bangboo_cards(state: &AppState, uid: u32, locale: Locale) -> String {
    let save = load_player_save(state, uid).unwrap_or_default();
    let hakushin = load_hakushin_data(state, locale);

    let mut cards = String::new();

    for buddy in &save.buddy {
        let name = hakushin
            .bangboos
            .get(&buddy.id)
            .map(|entry| entry.name.clone())
            .unwrap_or_else(|| format!("{} {}", t(locale, "fallback.bangboo"), buddy.id));
        let img = hakushin
            .bangboos
            .get(&buddy.id)
            .and_then(|entry| entry.image_local.as_deref())
            .map(to_asset_url)
            .unwrap_or_else(|| svg_data_uri(&name));

        let uid_label = t(locale, "uid");
        let level_label = t(locale, "bangboo.level");
        let rank_label = t(locale, "bangboo.rank");

        cards.push_str(&format!(
            "<a class=\"card\" href=\"/bangboo/{id}\"><img class=\"thumb\" src=\"{img}\" alt=\"{name}\" /><span class=\"pill\">{uid_label} {id}</span><h3>{name}</h3><div class=\"meta\">{level_label} {level}</div><div class=\"meta\">{rank_label} {rank}</div></a>",
            id = buddy.id,
            name = html_escape_attr(&name),
            level = buddy.level,
            rank = buddy.rank,
            img = html_escape_attr(&img),
            uid_label = uid_label,
            level_label = level_label,
            rank_label = rank_label,
        ));
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

fn render_bangboo_skill_inputs(locale: Locale, skill_levels: &[u32]) -> String {
    let mut html = String::new();
    for (idx, (name, label_key)) in [
        ("manual", "skill.manual"),
        ("passive", "skill.passive"),
        ("qte", "skill.qte"),
        ("aid", "skill.aid"),
    ]
    .iter()
    .enumerate()
    {
        let value = skill_levels.get(idx).copied().unwrap_or(1);
        let label = t(locale, label_key);
        html.push_str(&format!(
            "<div><label>{label}</label><input name=\"skill_{name}\" type=\"number\" min=\"1\" value=\"{value}\" /></div>",
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
    let active_state = state.clone();
    let uid = resolve_player_uid(&active_state, session.uid);

    let base_path = active_state.asset_dir.join("BuddyBaseTemplateTb.zon");
    let base_data = match fs::read_to_string(&base_path) {
        Ok(d) => d,
        Err(_) => {
            return Html(format!("Failed to read BuddyBaseTemplateTb.zon")).into_response();
        }
    };
    let entries = zon_parse_entries(&base_data);

    let mut save = load_player_save(&active_state, uid).unwrap_or_default();

    let mut existing_ids = HashMap::new();
    for buddy in &save.buddy {
        existing_ids.insert(buddy.id, true);
    }

    for item in &entries {
        let Some(template_id) = item.get("id").and_then(|v: &String| v.parse::<u32>().ok()) else {
            continue;
        };
        if template_id >= 55000 {
            continue;
        }
        if existing_ids.contains_key(&template_id) {
            continue;
        }

            let buddy = BuddyItemSave {
                id: template_id,
                level: 60,
                exp: 0,
                rank: 6,
                star: 1,
                favorite: false,
                skill_levels: vec![12, 5, 12, 12],
            };

            save.buddy.push(buddy);
    }

    save_player_save(&active_state, uid, &save);

    audit_log(
        &active_state.root_dir,
        &session.username,
        session.uid,
        "bangboo_add_all",
        "added all missing bangboos",
    );

    Redirect::to("/dashboard?tab=bangboos").into_response()
}
