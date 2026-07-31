use crate::{
    app_state::{AppState, state_with_active_server},
    auth::{get_session, html_escape_attr, redirect_to_login},
    ctl,
    data::hakushin::{load_hakushin_data, to_asset_url},
    i18n::{Locale, locale_from_headers, t},
    player_state::{load_player_save, resolve_player_uid},
    utils::{audit_log, shared_page_css, svg_data_uri},
};
use axum::{
    extract::{Form, OriginalUri, Path, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct AvatarUpdateForm {
    level: u32,
    core_ability: u32,
    unlocked_talent_num: u32,
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
    let locale = locale_from_headers(&headers);
    let Some(uid) = resolve_player_uid(&state, session.uid) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "player.not_found"))).into_response();
    };

    let save = load_player_save(&state, uid).unwrap_or_default();
    let Some(avatar_item) = save.avatar.iter().find(|a| a.id == avatar_id) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "avatar.not_found"))).into_response();
    };

    let online = ctl::player_is_online(&state.active_ctl_addr(&headers), uid);

    let level = avatar_item.level;
    let unlocked_talent_num = avatar_item.rank;
    let skill_levels = &avatar_item.skill_levels;

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
          <input name="level" type="number" min="1" value="{level}" {disabled} />
        </div>
        <div>
                    <label>{mindscapes_label}</label>
                    <input name="unlocked_talent_num" type="number" min="0" max="6" value="{unlocked_talent_num}" {disabled} />
        </div>
      </div>

      <h3>{skill_levels_label}</h3>
      <div class="row">
        {skills}
      </div>

      <div class="form-actions">
        <a href="/dashboard?tab=avatars" class="back">{back_label}</a>
        {submit}
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
        skills = render_skill_inputs(locale, skill_levels, online),
        disabled = if online { "" } else { "disabled" },
        submit = if online {
            format!("<button type=\"submit\">{}</button>", t(locale, "avatar.save"))
        } else {
            String::new()
        },
        level_label = t(locale, "avatar.level"),
        mindscapes_label = t(locale, "avatar.mindscapes"),
        skill_levels_label = t(locale, "avatar.skill_levels"),
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
    let state = state_with_active_server(&state, &headers);
    let Some(uid) = resolve_player_uid(&state, session.uid) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "player.not_found"))).into_response();
    };

    let addr = state.active_ctl_addr(&headers);
    if !ctl::player_is_online(&addr, uid) {
        return Html(t(locale, "player.offline")).into_response();
    }

    if let Err(e) = ctl::mod_avatar_meta(&addr, uid, avatar_id, 0, payload.level as u64) {
        return Html(format!("ctl error (level): {e}")).into_response();
    }

    if let Err(e) =
        ctl::mod_avatar_meta(&addr, uid, avatar_id, 2, payload.unlocked_talent_num as u64)
    {
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
        if let Err(e) = ctl::mod_avatar_meta(&addr, uid, avatar_id, 5, packed) {
            return Html(format!("ctl error (skill {skill_id}): {e}")).into_response();
        }
    }

    audit_log(
        &state.root_dir,
        &session.username,
        session.uid,
        "avatar_update",
        &format!("avatar_id={}", avatar_id),
    );

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
        cards.push_str(&format!(
            "<p class=\"meta\">{}</p>",
            t(locale, "avatar.no_characters")
        ));
    }

    format!("<div class=\"cards\">{cards}</div>")
}

fn render_skill_inputs(locale: Locale, skill_levels: &[u32], online: bool) -> String {
    let disabled = if online { "" } else { "disabled" };
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
            "<div><label>{label}</label><input name=\"{name}\" type=\"number\" min=\"1\" value=\"{value}\" {disabled} /></div>",
            label = t(locale, label_key),
        ));
    }

    let core_ability = skill_levels.get(5).copied().unwrap_or(1);
    html.push_str(&format!(
        "<div><label>{label}</label><input name=\"core_ability\" type=\"number\" min=\"0\" max=\"6\" value=\"{core_ability}\" {disabled} /></div>",
        label = t(locale, "avatar.core_ability"),
    ));

    html
}
