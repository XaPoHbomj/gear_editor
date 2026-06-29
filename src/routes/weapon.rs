use crate::{
    app_state::AppState,
    auth::{get_session, html_escape_attr, redirect_to_login},
    data::{
        hakushin::{load_hakushin_data, to_asset_url},
        templates::load_weapon_templates,
    },
    i18n::{Locale, locale_from_headers, t},
    player_state::{load_player_save, resolve_player_uid, save_player_save},
    remielle_save::WeaponItemSave,
    utils::{audit_log, shared_page_css, svg_data_uri},
};
use axum::{
    extract::{Form, OriginalUri, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
pub(crate) struct WeaponUpdateForm {
    level: u32,
    refine_level: u32,
}

#[derive(Deserialize)]
pub(crate) struct AddWeaponForm {
    pub(crate) weapon_id: u32,
    pub(crate) refine_level: u32,
}

#[derive(Deserialize)]
pub(crate) struct WeaponFilterQuery {
    pub(crate) class: Option<String>,
    pub(crate) rarity: Option<String>,
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

    let locale = locale_from_headers(&headers);
    let state = state.clone();
    let uid = resolve_player_uid(&state, session.uid);

    let Some(save) = load_player_save(&state, uid) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "weapon.not_found"))).into_response();
    };

    let Some(weapon) = save.weapon.iter().find(|w| w.uid == weapon_uid) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "weapon.not_found"))).into_response();
    };

    let level = weapon.level;
    let refine_level = weapon.refine;
    let weapon_id = weapon.id;
    let hakushin = load_hakushin_data(&state, locale);
    let weapon_name = hakushin
        .weapons
        .get(&weapon_id)
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| format!("{} {weapon_id}", t(locale, "fallback.weapon")));
    let weapon_img = hakushin
        .weapons
        .get(&weapon_id)
        .and_then(|entry| entry.image_local.as_deref())
        .map(to_asset_url)
        .unwrap_or_else(|| svg_data_uri(&weapon_name));

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
            <img src="{weapon_img}" alt="{weapon_name}" />
            <div>
                <h1>{edit_title} {weapon_name}</h1>
                <div class="meta">{uid_label} {weapon_uid} · {id_label} {weapon_id}</div>
            </div>
        </div>
    <form method="post">
      <label>{level_label}</label>
      <input name="level" type="number" min="1" value="{level}" />
      <label>{overclock_label}</label>
      <input name="refine_level" type="number" min="0" value="{refine_level}" />
      <div class="form-actions">
        <a href="/dashboard?tab=weapons" class="back">{back_label}</a>
        <button type="submit">{save_label}</button>
      </div>
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
        edit_title = t(locale, "weapon.edit"),
        uid_label = t(locale, "weapon.uid"),
        id_label = t(locale, "weapon.id"),
        level_label = t(locale, "weapon.level"),
        overclock_label = t(locale, "weapon.overclock"),
        save_label = t(locale, "weapon.save"),
        back_label = t(locale, "weapon.back"),
        lang = locale.lang_attr(),
        shared_css = shared_page_css(),
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
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let locale = locale_from_headers(&headers);
    let state = state.clone();
    let uid = resolve_player_uid(&state, session.uid);

    let Some(mut save) = load_player_save(&state, uid) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "weapon.not_found"))).into_response();
    };

    let Some(weapon) = save.weapon.iter_mut().find(|w| w.uid == weapon_uid) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "weapon.not_found"))).into_response();
    };

    weapon.level = payload.level;
    weapon.refine = payload.refine_level;

    save_player_save(&state, uid, &save);

    Redirect::to("/dashboard?tab=weapons").into_response()
}

pub(crate) async fn weapon_new(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<WeaponFilterQuery>,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, _session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let locale = locale_from_headers(&headers);
    let filter_class = query.class.unwrap_or_default();
    let filter_rarity = query.rarity.unwrap_or_default();
    let options = render_weapon_select_options(&state, 0, locale, &filter_class, &filter_rarity);

    let hakushin = load_hakushin_data(&state, locale);
    let weapon_images: HashMap<u32, String> = hakushin
        .weapons
        .iter()
        .map(|(id, entry)| {
            let url = entry
                .image_local
                .as_deref()
                .map(to_asset_url)
                .unwrap_or_else(|| svg_data_uri(&entry.name));
            (*id, url)
        })
        .collect();
    let weapon_images_json =
        serde_json::to_string(&weapon_images).unwrap_or_else(|_| "{}".to_string());

    let body = format!(
        r#"<!doctype html>
<html lang="{lang}">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{new_title}</title>
    <style>{shared_css}</style>
</head>
<body>
    <div class="container">
        <h1>{new_title}</h1>
        <form method="get" style="margin-bottom:12px;">
            <div style="display:flex; gap:8px; flex-wrap:wrap; align-items:end;">
                <div style="display:flex; flex-direction:column; gap:4px;">
                    <span style="font-size:11px; color:#9aa4b2;">{class_label}</span>
                    <select name="class" onchange="this.form.submit()" style="width:auto; padding:5px 8px; border-radius:8px; border:1px solid #2a3140; background:#121620; color:#e6e6e6; font-size:12px;">{class_opts}</select>
                </div>
                <div style="display:flex; flex-direction:column; gap:4px;">
                    <span style="font-size:11px; color:#9aa4b2;">{rarity_label}</span>
                    <select name="rarity" onchange="this.form.submit()" style="width:auto; padding:5px 8px; border-radius:8px; border:1px solid #2a3140; background:#121620; color:#e6e6e6; font-size:12px;">{rarity_opts}</select>
                </div>
            </div>
        </form>
        <form method="post">
            <div>
                <img id="weapon_preview" class="preview-img" />
                <label>{weapon_label}</label>
                <select name="weapon_id" id="weapon_id" required>
                    {options}
                </select>
            </div>
            <div class="row">
                <div>
                    <label>{refine_label}</label>
                    <input name="refine_level" type="number" min="0" value="1" />
                </div>
            </div>
            <button type="submit">{create_label}</button>
        </form>
    </div>
    <script>
    var w = {weapon_images_json};
    var p = document.getElementById("weapon_preview");
    var s = document.getElementById("weapon_id");
    s.addEventListener("change", function() {{
        var u = w[s.value];
        if (u) {{ p.src = u; p.style.display = "block"; }}
        else {{ p.style.display = "none"; }}
    }});
    </script>
</body>
</html>"#,
        options = options,
        weapon_images_json = weapon_images_json,
        new_title = t(locale, "weapon.new"),
        weapon_label = t(locale, "avatar.weapon"),
        refine_label = t(locale, "weapon.refine_level"),
        create_label = t(locale, "weapon.create"),
        class_label = t(locale, "weapon.filter_class"),
        rarity_label = t(locale, "weapon.filter_rarity"),
        class_opts = render_weapon_filter_class_opts(locale, &filter_class),
        rarity_opts = render_weapon_filter_rarity_opts(locale, &filter_rarity),
        lang = locale.lang_attr(),
        shared_css = shared_page_css(),
    );

    Html(body).into_response()
}

pub(crate) async fn weapon_add(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
    Form(payload): Form<AddWeaponForm>,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let locale = locale_from_headers(&headers);
    let state = state.clone();
    let uid = resolve_player_uid(&state, session.uid);

    let mut save = load_player_save(&state, uid).unwrap_or_default();

    let next_uid = save.weapon.iter().map(|w| w.uid).max().unwrap_or(0) + 1;
    let new_uid = next_uid.max(1);

    let weapon = WeaponItemSave {
        uid: new_uid,
        id: payload.weapon_id,
        level: 60,
        star: 5,
        refine: payload.refine_level,
    };

    save.weapon.push(weapon);
    save_player_save(&state, uid, &save);

    audit_log(&state.root_dir, &session.username, session.uid, "weapon_add", &format!("created weapon {}", new_uid));
    Redirect::to(&format!("/weapon/{new_uid}")).into_response()
}

pub(crate) fn render_weapon_cards(state: &AppState, uid: u32, locale: Locale, filter_class: &str, filter_rarity: &str) -> String {
    let weapon_templates = load_weapon_templates(&state.asset_dir);
    let hakushin = load_hakushin_data(state, locale);

    let mut cards = String::new();
    if let Some(save) = load_player_save(state, uid) {
        for weapon in &save.weapon {
            let weapon_uid = weapon.uid;
            let weapon_id = weapon.id;
            let level = weapon.level;

            let info = hakushin.weapon_info.get(&weapon_id);

            if !filter_class.is_empty() {
                if info.map(|i| i.weapon_type.as_str()).unwrap_or("") != filter_class {
                    continue;
                }
            }
            if !filter_rarity.is_empty() {
                let rarity_str = match info.map(|i| i.rarity).unwrap_or(0) {
                    4 => "s",
                    3 => "a",
                    _ => "b",
                };
                if rarity_str != filter_rarity {
                    continue;
                }
            }

            let name = hakushin
                .weapons
                .get(&weapon_id)
                .map(|entry| entry.name.clone())
                .or_else(|| weapon_templates.get(&weapon_id).cloned())
                .unwrap_or_else(|| format!("{} {weapon_id}", t(locale, "fallback.weapon")));

            let img = hakushin
                .weapons
                .get(&weapon_id)
                .and_then(|entry| entry.image_local.as_deref())
                .map(to_asset_url)
                .unwrap_or_else(|| svg_data_uri(&name));
            cards.push_str(&format!(
                "<a class=\"card\" href=\"/weapon/{uid}\"><img class=\"thumb\" style=\"object-fit: contain;\" src=\"{img}\" alt=\"{name}\" /><span class=\"pill\">{uid_label} {uid}</span><h3>{name}</h3><div class=\"meta\">{level_label} {level}</div></a>",
                uid = weapon_uid,
                name = html_escape_attr(&name),
                level = level,
                img = html_escape_attr(&img),
                uid_label = t(locale, "weapon.uid"),
                level_label = t(locale, "weapon.level"),
            ));
        }
    }

    if cards.is_empty() {
        cards.push_str(&format!(
            "<p class=\"meta\">{}</p>",
            t(locale, "weapon.no_weapons")
        ));
    }

    let filter_panel = render_weapon_filter_panel(locale, filter_class, filter_rarity);
    let add_panel = render_add_weapon_panel(state, locale);
    format!("{add_panel}{filter_panel}<div class=\"cards\">{cards}</div>")
}

fn render_weapon_filter_panel(locale: Locale, filter_class: &str, filter_rarity: &str) -> String {
    let class_opts = {
        let all_sel = if filter_class.is_empty() { " selected" } else { "" };
        let attack_sel = if filter_class == "Attack" { " selected" } else { "" };
        let stun_sel = if filter_class == "Stun" { " selected" } else { "" };
        let anomaly_sel = if filter_class == "Anomaly" { " selected" } else { "" };
        let defense_sel = if filter_class == "Defense" { " selected" } else { "" };
        let rupture_sel = if filter_class == "Rupture" { " selected" } else { "" };
        let support_sel = if filter_class == "Support" { " selected" } else { "" };
        format!(
            "<option value=\"\"{all_sel}>{all}</option><option value=\"Attack\"{attack_sel}>{attack}</option><option value=\"Stun\"{stun_sel}>{stun}</option><option value=\"Anomaly\"{anomaly_sel}>{anomaly}</option><option value=\"Defense\"{defense_sel}>{defense}</option><option value=\"Rupture\"{rupture_sel}>{rupture}</option><option value=\"Support\"{support_sel}>{support}</option>",
            all = t(locale, "weapon.filter_all"),
            attack = t(locale, "weapon.class_attack"),
            stun = t(locale, "weapon.class_stun"),
            anomaly = t(locale, "weapon.class_anomaly"),
            defense = t(locale, "weapon.class_defense"),
            rupture = t(locale, "weapon.class_rupture"),
            support = t(locale, "weapon.class_support"),
        )
    };
    let rarity_opts = {
        let all_sel = if filter_rarity.is_empty() { " selected" } else { "" };
        let s_sel = if filter_rarity == "s" { " selected" } else { "" };
        let a_sel = if filter_rarity == "a" { " selected" } else { "" };
        let b_sel = if filter_rarity == "b" { " selected" } else { "" };
        format!(
            "<option value=\"\"{all_sel}>{all}</option><option value=\"s\"{s_sel}>{s}</option><option value=\"a\"{a_sel}>{a}</option><option value=\"b\"{b_sel}>{b}</option>",
            all = t(locale, "weapon.filter_all"),
            s = t(locale, "weapon.rarity_s"),
            a = t(locale, "weapon.rarity_a"),
            b = t(locale, "weapon.rarity_b"),
        )
    };

    format!(
        r#"<form method="get" action="/dashboard" style="margin-bottom:12px;">
            <input type="hidden" name="tab" value="weapons" />
            <div style="display:flex; gap:8px; flex-wrap:wrap; align-items:end;">
                <div style="display:flex; flex-direction:column; gap:4px;">
                    <span style="font-size:11px; color:#9aa4b2;">{class_label}</span>
                    <select name="weapon_class" onchange="this.form.submit()" style="width:auto; padding:5px 8px; border-radius:8px; border:1px solid #2a3140; background:#121620; color:#e6e6e6; font-size:12px;">{class_opts}</select>
                </div>
                <div style="display:flex; flex-direction:column; gap:4px;">
                    <span style="font-size:11px; color:#9aa4b2;">{rarity_label}</span>
                    <select name="weapon_rarity" onchange="this.form.submit()" style="width:auto; padding:5px 8px; border-radius:8px; border:1px solid #2a3140; background:#121620; color:#e6e6e6; font-size:12px;">{rarity_opts}</select>
                </div>
            </div>
        </form>"#,
        class_label = t(locale, "weapon.filter_class"),
        rarity_label = t(locale, "weapon.filter_rarity"),
        class_opts = class_opts,
        rarity_opts = rarity_opts,
    )
}

fn render_add_weapon_panel(state: &AppState, locale: Locale) -> String {
    let _ = state;
    format!(
        "<div class=\"panel\"><h3>{}</h3><div style=\"display:flex; gap:8px;\"><a href=\"/weapon/new\">{}</a></div></div>",
        t(locale, "weapon.add"),
        t(locale, "weapon.new_weapon"),
    )
}

fn render_weapon_select_options(state: &AppState, selected_id: u32, locale: Locale, filter_class: &str, filter_rarity: &str) -> String {
    let hakushin = load_hakushin_data(state, locale);
    let mut items: Vec<(u32, String)> = hakushin
        .weapons
        .iter()
        .filter(|(id, _)| {
            let info = hakushin.weapon_info.get(id);
            if !filter_class.is_empty() {
                if info.map(|i| i.weapon_type.as_str()).unwrap_or("") != filter_class {
                    return false;
                }
            }
            if !filter_rarity.is_empty() {
                let rarity_str = match info.map(|i| i.rarity).unwrap_or(0) {
                    4 => "s",
                    3 => "a",
                    _ => "b",
                };
                if rarity_str != filter_rarity {
                    return false;
                }
            }
            true
        })
        .map(|(id, entry)| (*id, entry.name.clone()))
        .collect();
    items.sort_by(|a, b| a.1.cmp(&b.1));

    let mut html = String::new();
    html.push_str(&format!(
        "<option value=\"\" disabled selected>{}</option>",
        t(locale, "weapon.select")
    ));
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

fn render_weapon_filter_class_opts(locale: Locale, filter_class: &str) -> String {
    let all_sel = if filter_class.is_empty() { " selected" } else { "" };
    let attack_sel = if filter_class == "Attack" { " selected" } else { "" };
    let stun_sel = if filter_class == "Stun" { " selected" } else { "" };
    let anomaly_sel = if filter_class == "Anomaly" { " selected" } else { "" };
    let defense_sel = if filter_class == "Defense" { " selected" } else { "" };
    let rupture_sel = if filter_class == "Rupture" { " selected" } else { "" };
    let support_sel = if filter_class == "Support" { " selected" } else { "" };
    format!(
        "<option value=\"\"{all_sel}>{all}</option><option value=\"Attack\"{attack_sel}>{attack}</option><option value=\"Stun\"{stun_sel}>{stun}</option><option value=\"Anomaly\"{anomaly_sel}>{anomaly}</option><option value=\"Defense\"{defense_sel}>{defense}</option><option value=\"Rupture\"{rupture_sel}>{rupture}</option><option value=\"Support\"{support_sel}>{support}</option>",
        all = t(locale, "weapon.filter_all"),
        attack = t(locale, "weapon.class_attack"),
        stun = t(locale, "weapon.class_stun"),
        anomaly = t(locale, "weapon.class_anomaly"),
        defense = t(locale, "weapon.class_defense"),
        rupture = t(locale, "weapon.class_rupture"),
        support = t(locale, "weapon.class_support"),
    )
}

fn render_weapon_filter_rarity_opts(locale: Locale, filter_rarity: &str) -> String {
    let all_sel = if filter_rarity.is_empty() { " selected" } else { "" };
    let s_sel = if filter_rarity == "s" { " selected" } else { "" };
    let a_sel = if filter_rarity == "a" { " selected" } else { "" };
    let b_sel = if filter_rarity == "b" { " selected" } else { "" };
    format!(
        "<option value=\"\"{all_sel}>{all}</option><option value=\"s\"{s_sel}>{s}</option><option value=\"a\"{a_sel}>{a}</option><option value=\"b\"{b_sel}>{b}</option>",
        all = t(locale, "weapon.filter_all"),
        s = t(locale, "weapon.rarity_s"),
        a = t(locale, "weapon.rarity_a"),
        b = t(locale, "weapon.rarity_b"),
    )
}
