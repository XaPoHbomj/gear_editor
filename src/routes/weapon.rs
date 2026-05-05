use crate::{
    app_state::{AppState, state_with_active_server},
    auth::{get_session, get_session_mut, html_escape_attr, redirect_to_login, set_session},
    data::{
        hakushin::{load_hakushin_data, to_asset_url},
        templates::load_weapon_templates,
    },
    player_state::{read_next_uid, resolve_player_uid},
    utils::svg_data_uri,
    zon::{ZValue, format_zon_pretty, read_zon, zon_get_number, zon_serialize, zon_set_number},
};
use axum::{
    extract::{Form, OriginalUri, Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
};
use serde::Deserialize;
use std::fs;

#[derive(Deserialize)]
pub(crate) struct WeaponUpdateForm {
    level: u32,
    refine_level: u32,
}

#[derive(Deserialize)]
pub(crate) struct AddWeaponForm {
    weapon_id: u32,
    refine_level: u32,
}

pub(crate) async fn weapon_edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(weapon_uid): Path<u32>,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&state, session.uid);
    let weapon_path = state
        .state_dir
        .join(format!("player/{uid}/weapon/{weapon_uid}"));

    let Some(weapon_zon) = read_zon(&weapon_path) else {
        return (StatusCode::NOT_FOUND, Html("Weapon not found")).into_response();
    };

    let level = zon_get_number(&weapon_zon, "level").unwrap_or(1) as u32;
    let refine_level = zon_get_number(&weapon_zon, "refine_level").unwrap_or(1) as u32;
    let weapon_id = zon_get_number(&weapon_zon, "id").unwrap_or(0) as u32;
    let hakushin = load_hakushin_data(&state);
    let weapon_name = hakushin
        .weapons
        .get(&weapon_id)
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| format!("Weapon {weapon_id}"));
    let weapon_img = hakushin
        .weapons
        .get(&weapon_id)
        .and_then(|entry| entry.image_local.as_deref())
        .map(to_asset_url)
        .unwrap_or_else(|| svg_data_uri(&weapon_name));

    let body = format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Edit Weapon</title>
  <style>
    body {{ font-family: system-ui, sans-serif; margin: 0; background: #0f1115; color: #e6e6e6; }}
    .container {{ padding: 24px; max-width: 900px; margin: 0 auto; }}
        input {{ width: 100%; box-sizing: border-box; padding: 8px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
    label {{ display: block; margin: 12px 0 6px; font-size: 12px; color: #9aa4b2; }}
    button {{ margin-top: 16px; padding: 10px 14px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }}
        .hero {{ display: flex; gap: 16px; align-items: center; margin-bottom: 16px; }}
        .hero img {{ width: 120px; height: 120px; border-radius: 12px; object-fit: cover; border: 1px solid #2a3140; background: #0f1115; }}
        .hero h1 {{ margin: 0; }}
        .meta {{ color: #9aa4b2; font-size: 12px; }}
        @media (max-width: 768px) {{
                .container {{ padding: 14px; }}
                .hero {{ flex-direction: column; align-items: flex-start; }}
                .hero img {{ width: 100%; max-width: 240px; height: auto; aspect-ratio: 1 / 1; }}
                button {{ width: 100%; }}
        }}
  </style>
</head>
<body>
  <div class="container">
        <div class="hero">
            <img src="{weapon_img}" alt="{weapon_name}" />
            <div>
                <h1>Edit Weapon {weapon_name}</h1>
                <div class="meta">UID {weapon_uid} · ID {weapon_id}</div>
            </div>
        </div>
    <form method="post">
      <label>Level</label>
      <input name="level" type="number" min="1" value="{level}" />
      <label>Overclock (refine level)</label>
      <input name="refine_level" type="number" min="0" value="{refine_level}" />
      <button type="submit">Save (pending)</button>
    </form>
  </div>
</body>
</html>"#,
        weapon_uid = weapon_uid,
        weapon_id = weapon_id,
        weapon_name = html_escape_attr(&weapon_name),
        weapon_img = html_escape_attr(&weapon_img),
        level = level,
        refine_level = refine_level,
    );

    Html(body).into_response()
}

pub(crate) async fn weapon_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(weapon_uid): Path<u32>,
    original_uri: OriginalUri,
    Form(payload): Form<WeaponUpdateForm>,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&state, session.uid);
    let weapon_path = state
        .state_dir
        .join(format!("player/{uid}/weapon/{weapon_uid}"));

    let Some(mut weapon_zon) = read_zon(&weapon_path) else {
        return (StatusCode::NOT_FOUND, Html("Weapon not found")).into_response();
    };

    zon_set_number(&mut weapon_zon, "level", payload.level as i64);
    zon_set_number(&mut weapon_zon, "refine_level", payload.refine_level as i64);

    let serialized = zon_serialize(&weapon_zon);
    session.pending_writes.insert(weapon_path, serialized);
    set_session(session_id, session);

    Redirect::to("/dashboard?tab=weapons").into_response()
}

pub(crate) async fn weapon_new(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, _session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let options = render_weapon_select_options(&state, 0);

    let body = format!(
        r#"<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>New Weapon</title>
    <style>
        body {{ font-family: system-ui, sans-serif; margin: 0; background: #0f1115; color: #e6e6e6; }}
        .container {{ padding: 24px; max-width: 900px; margin: 0 auto; }}
        input, select {{ width: 100%; box-sizing: border-box; padding: 8px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
        label {{ display: block; margin: 12px 0 6px; font-size: 12px; color: #9aa4b2; }}
        button {{ margin-top: 16px; padding: 10px 14px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }}
        .row {{ display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }}
        .row > * {{ min-width: 0; }}
        @media (max-width: 768px) {{
            .container {{ padding: 14px; }}
            .row {{ grid-template-columns: 1fr; }}
            button {{ width: 100%; }}
        }}
    </style>
</head>
<body>
    <div class="container">
        <h1>New Weapon</h1>
        <form method="post">
            <div>
                <label>Weapon</label>
                <select name="weapon_id" required>
                    {options}
                </select>
            </div>
            <div class="row">
                <div>
                    <label>Refine Level</label>
                    <input name="refine_level" type="number" min="0" value="1" />
                </div>
            </div>
            <button type="submit">Create</button>
        </form>
    </div>
</body>
</html>"#,
        options = options,
    );

    Html(body).into_response()
}

pub(crate) async fn weapon_add(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
    Form(payload): Form<AddWeaponForm>,
) -> impl IntoResponse {
    let Some((session_id, session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&state, session.uid);
    let weapon_dir = state.state_dir.join(format!("player/{uid}/weapon"));
    let next_uid = read_next_uid(&weapon_dir).unwrap_or(1);
    let new_uid = next_uid.max(1);

    let weapon = ZValue::Object(vec![
        ("id".to_string(), ZValue::Number(payload.weapon_id as i64)),
        ("level".to_string(), ZValue::Number(60)),
        ("exp".to_string(), ZValue::Number(0)),
        ("star".to_string(), ZValue::Number(5)),
        (
            "refine_level".to_string(),
            ZValue::Number(payload.refine_level as i64),
        ),
        ("lock".to_string(), ZValue::Bool(false)),
    ]);

    let weapon_path = weapon_dir.join(new_uid.to_string());
    let serialized = format_zon_pretty(&zon_serialize(&weapon));
    if let Some(parent) = weapon_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(err) = fs::write(&weapon_path, serialized) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("Failed to create weapon: {}", err)),
        )
            .into_response();
    }

    let next_path = weapon_dir.join("next");
    if let Err(err) = fs::write(&next_path, format!("{}\n", new_uid + 1)) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("Failed to update weapon counter: {}", err)),
        )
            .into_response();
    }

    set_session(session_id, session);
    Redirect::to(&format!("/weapon/{new_uid}")).into_response()
}

pub(crate) fn render_weapon_cards(state: &AppState, uid: u32) -> String {
    let weapon_dir = state.state_dir.join(format!("player/{uid}/weapon"));
    let weapon_templates = load_weapon_templates(&state.asset_dir);
    let hakushin = load_hakushin_data(state);

    let mut cards = String::new();
    if let Ok(entries) = fs::read_dir(&weapon_dir) {
        for entry in entries.flatten() {
            let Some(file_name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            let weapon_uid = match file_name
                .strip_suffix(".zon")
                .unwrap_or(&file_name)
                .parse::<u32>()
            {
                Ok(value) if value > 0 => value,
                _ => continue,
            };
            let weapon = read_zon(&entry.path());
            let weapon_id = weapon
                .as_ref()
                .and_then(|v| zon_get_number(v, "id"))
                .unwrap_or(0) as u32;
            let level = weapon
                .as_ref()
                .and_then(|v| zon_get_number(v, "level"))
                .unwrap_or(0);

            let name = hakushin
                .weapons
                .get(&weapon_id)
                .map(|entry| entry.name.clone())
                .or_else(|| weapon_templates.get(&weapon_id).cloned())
                .unwrap_or_else(|| format!("Weapon {weapon_id}"));

            let img = hakushin
                .weapons
                .get(&weapon_id)
                .and_then(|entry| entry.image_local.as_deref())
                .map(to_asset_url)
                .unwrap_or_else(|| svg_data_uri(&name));
            cards.push_str(&format!(
                "<a class=\"card\" href=\"/weapon/{uid}\"><img class=\"thumb\" style=\"object-fit: contain;\" src=\"{img}\" alt=\"{name}\" /><span class=\"pill\">UID {uid}</span><h3>{name}</h3><div class=\"meta\">Level {level}</div></a>",
                uid = weapon_uid,
                name = html_escape_attr(&name),
                level = level,
                img = html_escape_attr(&img)
            ));
        }
    }

    if cards.is_empty() {
        cards.push_str("<p class=\"meta\">No weapons found for this account.</p>");
    }

    let add_panel = render_add_weapon_panel(state);
    format!("{add_panel}<div class=\"cards\">{cards}</div>")
}

fn render_add_weapon_panel(state: &AppState) -> String {
    let _ = state;
    "<div class=\"panel\"><h3>Add Weapon</h3><a href=\"/weapon/new\" style=\"display:inline-block; box-sizing:border-box; text-align:center;\">New weapon</a></div>"
                .to_string()
}

fn render_weapon_select_options(state: &AppState, selected_id: u32) -> String {
    let hakushin = load_hakushin_data(state);
    let mut items: Vec<(u32, String)> = hakushin
        .weapons
        .iter()
        .map(|(id, entry)| (*id, entry.name.clone()))
        .collect();
    items.sort_by(|a, b| a.1.cmp(&b.1));

    let mut html = String::new();
    html.push_str("<option value=\"\" disabled selected>Select weapon</option>");
    for (id, name) in items {
        html.push_str(&format!(
            "<option value=\"{}\"{}>{}</option>",
            id,
            if id == selected_id { " selected" } else { "" },
            name
        ));
    }
    html
}
