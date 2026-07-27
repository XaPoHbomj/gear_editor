use crate::{
    app_state::AppState,
    auth::{get_session, html_escape_attr, redirect_to_login},
    ctl,
    data::{
        hakushin::{load_hakushin_data, to_asset_url},
        templates::{
            load_player_equips, load_player_weapons, render_equip_selects,
            render_weapon_select,
        },
    },
    i18n::{Locale, locale_from_headers, t},
    player_state::{load_player_save, parse_slot_value, resolve_player_uid},
    remielle_save::AvatarItemSave,
    utils::{audit_log, shared_page_css, svg_data_uri},
    zon::zon_parse_entries,
};
use axum::{
    extract::{Form, OriginalUri, Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::fs;

#[derive(Deserialize)]
pub(crate) struct AvatarUpdateForm {
    level: u32,
    core_ability: u32,
    unlocked_talent_num: u32,
    cur_weapon_uid: u32,
    equip_slot_1: String,
    equip_slot_2: String,
    equip_slot_3: String,
    equip_slot_4: String,
    equip_slot_5: String,
    equip_slot_6: String,
    skill_common_attack: u32,
    skill_special_attack: u32,
    skill_evade: u32,
    skill_cooperate_skill: u32,
    skill_assist_skill: u32,
}

pub(crate) async fn avatar_edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(avatar_id): Path<u32>,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state.clone();
    let uid = resolve_player_uid(&state, session.uid);
    let locale = locale_from_headers(&headers);

    let save = load_player_save(&state, uid).unwrap_or_default();
    let Some(avatar_item) = save.avatar.iter().find(|a| a.id == avatar_id) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "avatar.not_found"))).into_response();
    };

    let level = avatar_item.level;
    let unlocked_talent_num = avatar_item.rank;
    let cur_weapon_uid = avatar_item.weapon_uid;
    let skill_levels = &avatar_item.skill_levels;

    let weapon_options = load_player_weapons(&state, uid, locale);
    let dressed_equip = &avatar_item.equipment_uids;
    let equip_options = load_player_equips(&state, uid, locale);

    let hakushin = load_hakushin_data(&state, locale);
    let avatar_name = hakushin
        .avatars
        .get(&avatar_id)
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| format!("{} {avatar_id}", t(locale, "fallback.avatar")));
    let avatar_img = hakushin
        .avatars
        .get(&avatar_id)
        .and_then(|entry| entry.image_local.as_deref())
        .map(to_asset_url)
        .unwrap_or_else(|| svg_data_uri(&avatar_name));

    let body = format!(
        r#"<!doctype html>
<html lang="{lang}">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
        <title>{avatar_edit_title}</title>
  <style>{shared_css}</style>
</head>
<body>
  <div class="container">
        <div class="hero">
            <img src="{avatar_img}" alt="{avatar_name}" />
            <div>
                <h1>{avatar_edit_title} {avatar_name}</h1>
                <div class="meta">{id_label} {avatar_id}</div>
            </div>
        </div>
    <form method="post">
      <div class="row">
        <div>
          <label>{level_label}</label>
          <input name="level" type="number" min="1" value="{level}" />
        </div>
        <div>
                    <label>{mindscapes_label}</label>
                    <input name="unlocked_talent_num" type="number" min="0" max="6" value="{unlocked_talent_num}" />
        </div>
                <div>
                    <label>{weapon_label}</label>
                    {weapon_select}
                </div>
      </div>
            <h3>{equipped_discs_label}</h3>
            <div class="row">
                {equip_selects}
            </div>

      <h3>{skill_levels_label}</h3>
      <div class="row">
        {skills}
      </div>

      <div class="form-actions">
        <a href="/dashboard?tab=avatars" class="back">{back_label}</a>
        <button type="submit">{save_label}</button>
      </div>
    </form>
  </div>
</body>
</html>"#,
        avatar_id = avatar_id,
        avatar_name = html_escape_attr(&avatar_name),
        avatar_img = html_escape_attr(&avatar_img),
        level = level,
        unlocked_talent_num = unlocked_talent_num,
        weapon_select = render_weapon_select(locale, cur_weapon_uid, &weapon_options),
        equip_selects = render_equip_selects(locale, &equip_options, dressed_equip),
        skills = render_skill_inputs(locale, skill_levels),
        level_label = t(locale, "avatar.level"),
        mindscapes_label = t(locale, "avatar.mindscapes"),
        weapon_label = t(locale, "avatar.weapon"),
        equipped_discs_label = t(locale, "avatar.equipped_discs"),
        skill_levels_label = t(locale, "avatar.skill_levels"),
        save_label = t(locale, "avatar.save"),
        back_label = t(locale, "avatar.back"),
        id_label = t(locale, "avatar.id"),
        avatar_edit_title = t(locale, "avatar.edit"),
        shared_css = shared_page_css(),
        lang = locale.lang_attr(),
    );
    Html(body).into_response()
}

pub(crate) async fn avatar_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(avatar_id): Path<u32>,
    original_uri: OriginalUri,
    Form(payload): Form<AvatarUpdateForm>,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let locale = locale_from_headers(&headers);
    let uid = resolve_player_uid(&state, session.uid);

    let addr = &state.ctl_addr;

    if let Err(e) = ctl::mod_avatar_meta(addr, uid, avatar_id, 0, payload.level as u64) {
        return Html(format!("ctl error (level): {e}")).into_response();
    }

    if let Err(e) = ctl::mod_avatar_meta(addr, uid, avatar_id, 2, payload.unlocked_talent_num as u64) {
        return Html(format!("ctl error (rank): {e}")).into_response();
    }

    let skill_map: [(u32, u32); 6] = [
        (0, payload.skill_common_attack),
        (1, payload.skill_special_attack),
        (2, payload.skill_evade),
        (3, payload.skill_cooperate_skill),
        (5, payload.core_ability),
        (6, payload.skill_assist_skill),
    ];

    for &(skill_id, level) in &skill_map {
        let packed = (skill_id as u64) | ((level as u64) << 32);
        if let Err(e) = ctl::mod_avatar_meta(addr, uid, avatar_id, 5, packed) {
            return Html(format!("ctl error (skill {skill_id}): {e}")).into_response();
        }
    }

    audit_log(&state.root_dir, &session.username, session.uid, "avatar_update", &format!("avatar_id={}", avatar_id));

    Redirect::to("/dashboard?tab=avatars").into_response()
}

pub(crate) fn render_avatar_cards(state: &AppState, uid: u32, locale: Locale) -> String {
    let save = load_player_save(state, uid).unwrap_or_default();
    let hakushin = load_hakushin_data(state, locale);

    let mut cards = String::new();
    for avatar_item in &save.avatar {
        let name = hakushin
            .avatars
            .get(&avatar_item.id)
            .map(|entry| entry.name.clone())
            .unwrap_or_else(|| format!("Avatar {}", avatar_item.id));

        let img = hakushin
            .avatars
            .get(&avatar_item.id)
            .and_then(|entry| entry.image_local.as_deref())
            .map(to_asset_url)
            .unwrap_or_else(|| svg_data_uri(&name));

        cards.push_str(&format!(
            "<a class=\"card\" href=\"/avatar/{id}\"><img class=\"thumb\" src=\"{img}\" alt=\"{name}\" /><span class=\"pill\">{id_label} {id}</span><h3>{name}</h3><div class=\"meta\">{level_label} {level}</div></a>",
            id = avatar_item.id,
            name = html_escape_attr(&name),
            level = avatar_item.level,
            id_label = t(locale, "avatar.id"),
            level_label = t(locale, "avatar.level"),
            img = html_escape_attr(&img)
        ));
    }

    if cards.is_empty() {
        cards.push_str(&format!("<p class=\"meta\">{}</p>", t(locale, "avatar.no_characters")));
    }

    let add_panel = render_add_avatar_panel(locale);
    format!("{add_panel}<div class=\"cards\">{cards}</div>")
}

fn render_skill_inputs(locale: Locale, skill_levels: &[u32]) -> String {
    let mut html = String::new();
    for (idx, label_key) in [
        (0usize, "skill.basic_attack"),
        (1usize, "skill.special_attack"),
        (2usize, "skill.evade"),
        (3usize, "skill.ultimate"),
        (6usize, "skill.assist"),
    ] {
        let value = skill_levels.get(idx).copied().unwrap_or(1);
        let name = match idx {
            0 => "skill_common_attack",
            1 => "skill_special_attack",
            2 => "skill_evade",
            3 => "skill_cooperate_skill",
            6 => "skill_assist_skill",
            _ => unreachable!(),
        };
        html.push_str(&format!(
            "<div><label>{label}</label><input name=\"{name}\" type=\"number\" min=\"1\" value=\"{value}\" /></div>",
            label = t(locale, label_key),
        ));
    }

    let core_ability = skill_levels.get(5).copied().unwrap_or(1);
    html.push_str(&format!(
        "<div><label>{label}</label><input name=\"core_ability\" type=\"number\" min=\"0\" max=\"6\" value=\"{core_ability}\" /></div>",
        label = t(locale, "avatar.core_ability"),
    ));

    html
}

fn render_add_avatar_panel(locale: Locale) -> String {
    format!(
        "<div class=\"panel\"><h3>{}</h3><div style=\"display:flex; gap:8px;\"><form method=\"post\" action=\"/avatar/add-all\" style=\"margin:0;\"><button type=\"submit\">{}</button></form></div></div>",
        t(locale, "nav.characters"),
        t(locale, "avatar.add_all"),
    )
}

pub(crate) async fn avatar_add_all(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let locale = locale_from_headers(&headers);
    let uid = resolve_player_uid(&state, session.uid);

    let save = load_player_save(&state, uid).unwrap_or_default();
    let existing: std::collections::HashSet<u32> = save.avatar.iter().map(|a| a.id).collect();

    let base_path = state.asset_dir.join("AvatarBaseTemplateTb.zon");
    let base_data = match fs::read_to_string(&base_path) {
        Ok(d) => d,
        Err(_) => {
            return Html(format!("Failed to read AvatarBaseTemplateTb.zon")).into_response();
        }
    };
    let entries = zon_parse_entries(&base_data);

    let pidors: &[u32] = &[];
    let addr = &state.ctl_addr;
    let mut added = 0u32;

    for item in &entries {
        let Some(template_id) = item.get("id").and_then(|v| v.parse::<u32>().ok()) else {
            continue;
        };
        let camp = item.get("camp").and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
        if camp == 0 { continue; }
        if pidors.contains(&template_id) { continue; }
        if existing.contains(&template_id) { continue; }

        if let Err(e) = ctl::mod_avatar_meta(addr, uid, template_id, 0, 60) {
            return Html(format!("ctl error (level) avatar {template_id}: {e}")).into_response();
        }
        if let Err(e) = ctl::mod_avatar_meta(addr, uid, template_id, 2, 6) {
            return Html(format!("ctl error (rank) avatar {template_id}: {e}")).into_response();
        }
        if let Err(e) = ctl::mod_avatar_meta(addr, uid, template_id, 3, 2047) {
            return Html(format!("ctl error (talents) avatar {template_id}: {e}")).into_response();
        }
        for &skill in &[0u32, 1, 2, 3, 5, 6] {
            let packed = (skill as u64) | ((12u64) << 32);
            if let Err(e) = ctl::mod_avatar_meta(addr, uid, template_id, 5, packed) {
                return Html(format!("ctl error (skill) avatar {template_id}: {e}")).into_response();
            }
        }

        added += 1;
    }

    audit_log(&state.root_dir, &session.username, session.uid, "avatar_add_all", &format!("added {} agents", added));
    Redirect::to("/dashboard?tab=avatars").into_response()
}
