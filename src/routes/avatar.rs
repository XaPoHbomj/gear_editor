use crate::{
    app_state::{AppState, state_with_active_server},
    auth::{get_session, get_session_mut, html_escape_attr, redirect_to_login, set_session},
    data::{
        hakushin::{load_hakushin_data, to_asset_url},
        templates::{
            load_avatar_templates, load_player_equips, load_player_weapons, render_equip_selects,
            render_weapon_select,
        },
    },
    player_state::{parse_slot_value, resolve_item_path, resolve_player_uid},
    utils::svg_data_uri,
    zon::{
        read_zon, read_zon_verbose, zon_get_array_numbers, zon_get_number, zon_get_skill_levels,
        zon_serialize, zon_set_dressed_equip, zon_set_number, zon_set_skill_levels,
    },
};
use axum::{
    extract::{Form, OriginalUri, Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
};
use serde::Deserialize;
use std::{collections::HashMap, fs};

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

    let state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&state, session.uid);
    let avatar_path = resolve_item_path(&state.state_dir, uid, "avatar", avatar_id);

    let Some(avatar_zon) = read_zon_verbose(&avatar_path) else {
        return (StatusCode::NOT_FOUND, Html("Avatar not found")).into_response();
    };

    let level = zon_get_number(&avatar_zon, "level").unwrap_or(1) as u32;
    let passive_skill_level =
        zon_get_number(&avatar_zon, "passive_skill_level").unwrap_or(0) as u32;
    let unlocked_talent_num =
        zon_get_number(&avatar_zon, "unlocked_talent_num").unwrap_or(0) as u32;
    let cur_weapon_uid = zon_get_number(&avatar_zon, "cur_weapon_uid").unwrap_or(0) as u32;
    let weapon_options = load_player_weapons(&state, uid);
    let dressed_equip = zon_get_array_numbers(&avatar_zon, "dressed_equip");
    let equip_options = load_player_equips(&state, uid);
    let hakushin = load_hakushin_data(&state);
    let avatar_name = hakushin
        .avatars
        .get(&avatar_id)
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| format!("Avatar {avatar_id}"));
    let avatar_img = hakushin
        .avatars
        .get(&avatar_id)
        .and_then(|entry| entry.image_local.as_deref())
        .map(to_asset_url)
        .unwrap_or_else(|| svg_data_uri(&avatar_name));

    let skill_levels = zon_get_skill_levels(&avatar_zon);

    let body = format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
        <title>Edit Character</title>
  <style>
    body {{ font-family: system-ui, sans-serif; margin: 0; background: #0f1115; color: #e6e6e6; }}
    .container {{ padding: 24px; max-width: 900px; margin: 0 auto; }}
        input, select {{ width: 100%; box-sizing: border-box; padding: 8px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
    label {{ display: block; margin: 12px 0 6px; font-size: 12px; color: #9aa4b2; }}
    button {{ margin-top: 16px; padding: 10px 14px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }}
    .row {{ display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }}
    .row > * {{ min-width: 0; }}
        .hero {{ display: flex; gap: 16px; align-items: center; margin-bottom: 16px; }}
        .hero img {{ width: 120px; height: 120px; border-radius: 12px; object-fit: cover; object-position: top; border: 1px solid #2a3140; background: #0f1115; }}
        .hero h1 {{ margin: 0; }}
        .meta {{ color: #9aa4b2; font-size: 12px; }}
  </style>
</head>
<body>
  <div class="container">
        <div class="hero">
            <img src="{avatar_img}" alt="{avatar_name}" />
            <div>
                <h1>Edit Character {avatar_name}</h1>
                <div class="meta">ID {avatar_id}</div>
            </div>
        </div>
    <form method="post">
      <div class="row">
        <div>
          <label>Level</label>
          <input name="level" type="number" min="1" value="{level}" />
        </div>
        <div>
                    <label>Mindscapes</label>
                    <input name="unlocked_talent_num" type="number" min="0" max="6" value="{unlocked_talent_num}" />
        </div>
                <div>
                    <label>Weapon</label>
                    {weapon_select}
                </div>
      </div>
            <h3>Equipped Disks</h3>
            <div class="row">
                {equip_selects}
            </div>

      <h3>Skill levels</h3>
      <div class="row">
        {skills}
      </div>

      <button type="submit">Save (pending)</button>
    </form>
  </div>
</body>
</html>"#,
        avatar_id = avatar_id,
        avatar_name = html_escape_attr(&avatar_name),
        avatar_img = html_escape_attr(&avatar_img),
        level = level,
        unlocked_talent_num = unlocked_talent_num,
        weapon_select = render_weapon_select(cur_weapon_uid, &weapon_options),
        equip_selects = render_equip_selects(&equip_options, &dressed_equip),
        skills = render_skill_inputs(&skill_levels, passive_skill_level),
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
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&state, session.uid);
    let avatar_path = resolve_item_path(&state.state_dir, uid, "avatar", avatar_id);

    let Some(mut avatar_zon) = read_zon_verbose(&avatar_path) else {
        return (StatusCode::NOT_FOUND, Html("Avatar not found")).into_response();
    };

    zon_set_number(&mut avatar_zon, "level", payload.level as i64);
    zon_set_number(
        &mut avatar_zon,
        "unlocked_talent_num",
        payload.unlocked_talent_num as i64,
    );
    zon_set_number(
        &mut avatar_zon,
        "passive_skill_level",
        payload.core_ability as i64,
    );
    zon_set_number(
        &mut avatar_zon,
        "cur_weapon_uid",
        payload.cur_weapon_uid as i64,
    );

    let equipped = vec![
        parse_slot_value(&payload.equip_slot_1),
        parse_slot_value(&payload.equip_slot_2),
        parse_slot_value(&payload.equip_slot_3),
        parse_slot_value(&payload.equip_slot_4),
        parse_slot_value(&payload.equip_slot_5),
        parse_slot_value(&payload.equip_slot_6),
    ];
    zon_set_dressed_equip(&mut avatar_zon, "dressed_equip", &equipped, 6);

    let mut skill_levels = vec![
        ("common_attack", payload.skill_common_attack),
        ("special_attack", payload.skill_special_attack),
        ("evade", payload.skill_evade),
        ("cooperate_skill", payload.skill_cooperate_skill),
        ("assist_skill", payload.skill_assist_skill),
    ];

    zon_set_skill_levels(&mut avatar_zon, &mut skill_levels);

    let serialized = zon_serialize(&avatar_zon);
    session.pending_writes.insert(avatar_path, serialized);
    set_session(session_id, session);

    Redirect::to("/dashboard?tab=avatars").into_response()
}

pub(crate) fn render_avatar_cards(state: &AppState, uid: u32) -> String {
    let avatar_dir = state.state_dir.join(format!("player/{uid}/avatar"));
    let avatar_templates = load_avatar_templates(&state.asset_dir);
    let hakushin = load_hakushin_data(state);

    let mut cards = String::new();
    if let Ok(entries) = fs::read_dir(&avatar_dir) {
        for entry in entries.flatten() {
            if let Some(file_name) = entry.file_name().to_str().map(|s| s.to_string()) {
                let avatar_id = file_name
                    .strip_suffix(".zon")
                    .unwrap_or(&file_name)
                    .parse::<u32>()
                    .unwrap_or(0);
                let name = hakushin
                    .avatars
                    .get(&avatar_id)
                    .map(|entry| entry.name.clone())
                    .or_else(|| avatar_templates.get(&avatar_id).cloned())
                    .unwrap_or_else(|| format!("Avatar {avatar_id}"));

                let level = read_zon(&entry.path())
                    .and_then(|v| zon_get_number(&v, "level"))
                    .unwrap_or(0);

                let img = hakushin
                    .avatars
                    .get(&avatar_id)
                    .and_then(|entry| entry.image_local.as_deref())
                    .map(to_asset_url)
                    .unwrap_or_else(|| svg_data_uri(&name));
                cards.push_str(&format!(
                    "<a class=\"card\" href=\"/avatar/{id}\"><img class=\"thumb\" src=\"{img}\" alt=\"{name}\" /><span class=\"pill\">ID {id}</span><h3>{name}</h3><div class=\"meta\">Level {level}</div></a>",
                    id = avatar_id,
                    name = html_escape_attr(&name),
                    level = level,
                    img = html_escape_attr(&img)
                ));
            }
        }
    }

    if cards.is_empty() {
        cards.push_str("<p class=\"meta\">No characters found for this account.</p>");
    }

    format!("<div class=\"cards\">{cards}</div>").to_string()
}

fn render_skill_inputs(skill_levels: &HashMap<String, u32>, core_ability: u32) -> String {
    let mut html = String::new();
    for (key, label) in [
        ("common_attack", "Basic attack"),
        ("special_attack", "Special attack"),
        ("evade", "Evade"),
        ("cooperate_skill", "Ultimate"),
        ("assist_skill", "Assist"),
    ] {
        let value = skill_levels.get(key).copied().unwrap_or(1);
        html.push_str(&format!(
            "<div><label>{label}</label><input name=\"skill_{key}\" type=\"number\" min=\"1\" value=\"{value}\" /></div>",
        ));
    }

    html.push_str(&format!(
        "<div><label>Core ability</label><input name=\"core_ability\" type=\"number\" min=\"0\" max=\"6\" value=\"{core_ability}\" /></div>",
    ));

    html
}
