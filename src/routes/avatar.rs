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
    i18n::{Locale, locale_from_headers, t},
    player_state::{parse_slot_value, resolve_item_path, resolve_player_uid},
    utils::{audit_log, shared_page_css, svg_data_uri},
    zon::{
        format_zon_pretty, read_zon, read_zon_verbose, zon_get_array_numbers, zon_get_number,
        zon_get_skill_levels, zon_serialize, zon_set_dressed_equip, zon_set_number,
        zon_set_skill_levels, ZValue,
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
    let locale = locale_from_headers(&headers);

    let Some(avatar_zon) = read_zon_verbose(&avatar_path) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "avatar.not_found"))).into_response();
    };

    let level = zon_get_number(&avatar_zon, "level").unwrap_or(1) as u32;
    let passive_skill_level =
        zon_get_number(&avatar_zon, "passive_skill_level").unwrap_or(0) as u32;
    let unlocked_talent_num =
        zon_get_number(&avatar_zon, "unlocked_talent_num").unwrap_or(0) as u32;
    let cur_weapon_uid = zon_get_number(&avatar_zon, "cur_weapon_uid").unwrap_or(0) as u32;
    let weapon_options = load_player_weapons(&state, uid, locale);
    let dressed_equip = zon_get_array_numbers(&avatar_zon, "dressed_equip");
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

    let skill_levels = zon_get_skill_levels(&avatar_zon);

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

      <button type="submit">{save_label}</button>
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
        equip_selects = render_equip_selects(locale, &equip_options, &dressed_equip),
        skills = render_skill_inputs(locale, &skill_levels, passive_skill_level),
        level_label = t(locale, "avatar.level"),
        mindscapes_label = t(locale, "avatar.mindscapes"),
        weapon_label = t(locale, "avatar.weapon"),
        equipped_discs_label = t(locale, "avatar.equipped_discs"),
        skill_levels_label = t(locale, "avatar.skill_levels"),
        save_label = t(locale, "avatar.save"),
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
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&state, session.uid);
    let avatar_path = resolve_item_path(&state.state_dir, uid, "avatar", avatar_id);
    let locale = locale_from_headers(&headers);

    let Some(mut avatar_zon) = read_zon_verbose(&avatar_path) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "avatar.not_found"))).into_response();
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

pub(crate) fn render_avatar_cards(state: &AppState, uid: u32, locale: Locale) -> String {
    let avatar_dir = state.state_dir.join(format!("player/{uid}/avatar"));
    let avatar_templates = load_avatar_templates(&state.asset_dir);
    let hakushin = load_hakushin_data(state, locale);

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

    let add_panel = render_add_avatar_panel(locale);
    format!("{add_panel}<div class=\"cards\">{cards}</div>").to_string()
}

fn render_skill_inputs(
    locale: Locale,
    skill_levels: &HashMap<String, u32>,
    core_ability: u32,
) -> String {
    let mut html = String::new();
    for (key, label_key) in [
        ("common_attack", "skill.basic_attack"),
        ("special_attack", "skill.special_attack"),
        ("evade", "skill.evade"),
        ("cooperate_skill", "skill.ultimate"),
        ("assist_skill", "skill.assist"),
    ] {
        let value = skill_levels.get(key).copied().unwrap_or(1);
        html.push_str(&format!(
            "<div><label>{label}</label><input name=\"skill_{key}\" type=\"number\" min=\"1\" value=\"{value}\" /></div>",
            label = t(locale, label_key),
        ));
    }

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
    let active_state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&active_state, session.uid);
    let avatar_dir = active_state.state_dir.join(format!("player/{uid}/avatar"));

    let base_path = active_state.asset_dir.join("AvatarBaseTemplateTb.json");
    let base_data = match fs::read_to_string(&base_path) {
        Ok(d) => d,
        Err(_) => {
            return Html(format!("Failed to read AvatarBaseTemplateTb.json")).into_response();
        }
    };
    let base_json: JsonValue = match serde_json::from_str(&base_data) {
        Ok(v) => v,
        Err(_) => {
            return Html(format!("Failed to parse AvatarBaseTemplateTb.json")).into_response();
        }
    };

    let form_path = active_state.asset_dir.join("AvatarFormTemplateTb.json");
    let form_json: Option<JsonValue> =
        fs::read_to_string(&form_path).ok().and_then(|d| serde_json::from_str(&d).ok());

    let pidors: &[u32] = &[];

    let mut existing_ids = HashMap::new();
    if let Ok(entries) = fs::read_dir(&avatar_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str().map(|s| s.to_string()) {
                let id = name.strip_suffix(".zon").unwrap_or(&name).parse::<u32>().unwrap_or(0);
                if id > 0 {
                    existing_ids.insert(id, true);
                }
            }
        }
    }

    let _ = fs::create_dir_all(&avatar_dir);

    if let Some(items) = base_json.get("data").and_then(|v| v.as_array()) {
        for item in items {
            let Some(template_id) = item.get("id").and_then(|v| v.as_u64()).map(|v| v as u32) else {
                continue;
            };
            let camp = item.get("camp").and_then(|v| v.as_i64()).unwrap_or(0);
            if camp == 0 {
                continue;
            }
            if pidors.contains(&template_id) {
                continue;
            }
            if existing_ids.contains_key(&template_id) {
                continue;
            }

            let cur_form_id = form_json.as_ref().and_then(|fj| {
                fj.get("data").and_then(|d| d.as_array()).and_then(|arr| {
                    arr.iter().find(|f| {
                        f.get("avatar_id").and_then(|v| v.as_u64()) == Some(template_id as u64)
                            && f.get("index").and_then(|v| v.as_u64()) == Some(1)
                    })
                    .and_then(|f| f.get("id").and_then(|v| v.as_u64()))
                })
            }).unwrap_or(0);

            let avatar = ZValue::Object(vec![
                ("level".to_string(), ZValue::Number(60)),
                ("exp".to_string(), ZValue::Number(0)),
                ("rank".to_string(), ZValue::Number(6)),
                ("unlocked_talent_num".to_string(), ZValue::Number(6)),
                (
                    "talent_switch_list".to_string(),
                    ZValue::Array(vec![
                        ZValue::Bool(false),
                        ZValue::Bool(false),
                        ZValue::Bool(false),
                        ZValue::Bool(true),
                        ZValue::Bool(true),
                        ZValue::Bool(true),
                    ]),
                ),
                ("passive_skill_level".to_string(), ZValue::Number(6)),
                ("cur_weapon_uid".to_string(), ZValue::Number(0)),
                ("is_favorite".to_string(), ZValue::Bool(false)),
                ("avatar_skin_id".to_string(), ZValue::Number(0)),
                ("is_awake_available".to_string(), ZValue::Bool(false)),
                ("awake_id".to_string(), ZValue::Number(0)),
                ("cur_form_id".to_string(), ZValue::Number(cur_form_id as i64)),
                ("is_awake_enabled".to_string(), ZValue::Bool(false)),
                (
                    "dressed_equip".to_string(),
                    ZValue::Array(vec![
                        ZValue::Null,
                        ZValue::Null,
                        ZValue::Null,
                        ZValue::Null,
                        ZValue::Null,
                        ZValue::Null,
                    ]),
                ),
                ("show_weapon_type".to_string(), ZValue::Enum("active".to_string())),
                (
                    "skill_type_level".to_string(),
                    ZValue::Array(vec![
                        ZValue::Object(vec![
                            ("type".to_string(), ZValue::Enum("common_attack".to_string())),
                            ("level".to_string(), ZValue::Number(12)),
                        ]),
                        ZValue::Object(vec![
                            ("type".to_string(), ZValue::Enum("special_attack".to_string())),
                            ("level".to_string(), ZValue::Number(12)),
                        ]),
                        ZValue::Object(vec![
                            ("type".to_string(), ZValue::Enum("evade".to_string())),
                            ("level".to_string(), ZValue::Number(12)),
                        ]),
                        ZValue::Object(vec![
                            ("type".to_string(), ZValue::Enum("cooperate_skill".to_string())),
                            ("level".to_string(), ZValue::Number(12)),
                        ]),
                        ZValue::Object(vec![
                            ("type".to_string(), ZValue::Enum("unique_skill".to_string())),
                            ("level".to_string(), ZValue::Number(12)),
                        ]),
                        ZValue::Object(vec![
                            ("type".to_string(), ZValue::Enum("core_skill".to_string())),
                            ("level".to_string(), ZValue::Number(7)),
                        ]),
                        ZValue::Object(vec![
                            ("type".to_string(), ZValue::Enum("assist_skill".to_string())),
                            ("level".to_string(), ZValue::Number(12)),
                        ]),
                    ]),
                ),
            ]);

            let serialized = format_zon_pretty(&zon_serialize(&avatar));
            let avatar_path = avatar_dir.join(template_id.to_string());
            if let Err(err) = fs::write(&avatar_path, serialized) {
                return Html(format!("{}: {}", t(locale, "disc.failed_create"), err)).into_response();
            }
        }
    }

    audit_log(&active_state.root_dir, &session.username, session.uid, "avatar_add_all", "added all missing agents");

    Redirect::to("/dashboard?tab=avatars").into_response()
}
