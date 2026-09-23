use crate::{
    app_state::AppState,
    auth::{AuthContext, csrf_input, html_escape_attr},
    ctl,
    data::hakushin::{load_hakushin_data, to_asset_url},
    i18n::{Locale, t},
    player_state::load_player_save,
    utils::{audit_log, page_shell, svg_data_uri, u16_from_u32},
};
use axum::{
    extract::{Form, Path, Query},
    http::StatusCode,
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
    _csrf: String,
}

#[derive(Deserialize)]
pub(crate) struct AddAvatarForm {
    pub(crate) avatar_id: u32,
    _csrf: String,
}

#[derive(Deserialize)]
pub(crate) struct AvatarFilterQuery {
    pub(crate) element: Option<String>,
    pub(crate) rank: Option<String>,
}

pub(crate) async fn avatar_edit(
    auth: AuthContext,
    Path(avatar_id): Path<u32>,
) -> impl IntoResponse {
    let active_state = &auth.state;
    let locale = auth.locale;
    let uid = match auth.player_uid() {
        Ok(uid) => uid,
        Err(response) => return response,
    };

    let save = load_player_save(active_state, uid).unwrap_or_default();
    let Some(avatar_item) = save.avatar.iter().find(|a| a.id == avatar_id) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "avatar.not_found"))).into_response();
    };

    let online = ctl::player_is_online(&active_state.ctl_addr, uid);

    let level = avatar_item.level;
    let unlocked_talent_num = avatar_item.talents;
    let skill_levels = &avatar_item.skill_levels;

    let hakushin = load_hakushin_data(active_state, locale);
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

    let inner = format!(
        r#"        <div class="hero">
            <img src="{avatar_img}" alt="{avatar_name}" />
            <div>
                <h1>{avatar_edit_title} {avatar_name}</h1>
                <div class="meta">{id_label} {avatar_id}</div>
            </div>
        </div>
    <form method="post">
      {csrf}
      <div class="row">
        <div>
          <label>{level_label}</label>
          <input name="level" type="number" min="1" max="60" value="{level}" {disabled} />
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
    </form>"#,
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
        csrf = csrf_input(&auth.session_id),
    );
    Html(page_shell(t(locale, "avatar.edit"), locale, &inner)).into_response()
}

pub(crate) async fn avatar_update(
    auth: AuthContext,
    Path(avatar_id): Path<u32>,
    Form(payload): Form<AvatarUpdateForm>,
) -> impl IntoResponse {
    if let Err(response) = auth.require_csrf(&payload._csrf) {
        return response;
    }

    let active_state = &auth.state;
    let locale = auth.locale;
    let session = &auth.session;
    let uid = match auth.player_uid() {
        Ok(uid) => uid,
        Err(response) => return response,
    };

    let addr = active_state.ctl_addr.clone();
    if !ctl::player_is_online(&addr, uid) {
        return Html(t(locale, "player.offline")).into_response();
    }

    let current = load_player_save(active_state, uid)
        .and_then(|save| save.avatar.into_iter().find(|a| a.id == avatar_id));

    // Ownership check: the avatar must belong to this player's save.
    if current.is_none() {
        return (StatusCode::NOT_FOUND, Html(t(locale, "avatar.not_found"))).into_response();
    }
    let current = current.unwrap();

    if !avatar_values_valid(&payload) {
        return (
            StatusCode::BAD_REQUEST,
            Html(t(locale, "avatar.values_out_of_range")),
        )
            .into_response();
    }

    let current_level = current.level;
    let current_talents = current.talents;

    let mut ops: Vec<(u8, u64)> = Vec::new();
    if payload.level as u64 != current_level as u64 {
        ops.push((0, payload.level as u64));
    }
    if payload.unlocked_talent_num as u64 != current_talents as u64 {
        ops.push((3, payload.unlocked_talent_num as u64));
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
        let current_skill = current
            .skill_levels
            .get(skill_id as usize)
            .copied()
            .unwrap_or(1);
        if level != current_skill {
            let packed = (skill_id as u64) | ((level as u64) << 32);
            ops.push((5, packed));
        }
    }

    for &(field, value) in &ops {
        if let Err(e) = ctl::mod_avatar_meta(&addr, uid, avatar_id, field, value) {
            return Html(format!("ctl error (field {field}): {e}")).into_response();
        }
    }

    if let Err(e) = ctl::save_player(&addr, uid) {
        return Html(format!("ctl error (save): {e}")).into_response();
    }

    audit_log(
        &active_state.root_dir,
        &session.username,
        session.uid,
        "avatar_update",
        &format!("avatar_id={}", avatar_id),
    );

    Redirect::to("/dashboard?tab=avatars").into_response()
}

pub(crate) async fn avatar_new(
    auth: AuthContext,
    Query(query): Query<AvatarFilterQuery>,
) -> impl IntoResponse {
    let active_state = &auth.state;
    let locale = auth.locale;
    let uid = match auth.player_uid() {
        Ok(uid) => uid,
        Err(response) => return response,
    };
    if !ctl::player_is_online(&active_state.ctl_addr, uid) {
        return Html(t(locale, "player.offline")).into_response();
    }

    let filter_element = query.element.unwrap_or_default();
    let filter_rank = query.rank.unwrap_or_default();

    let save = load_player_save(active_state, uid).unwrap_or_default();
    let owned: std::collections::HashSet<u32> = save.avatar.iter().map(|a| a.id).collect();

    let hakushin = load_hakushin_data(active_state, locale);
    let avatar_images: std::collections::HashMap<u32, String> = hakushin
        .avatars
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
    let avatar_images_json =
        serde_json::to_string(&avatar_images).unwrap_or_else(|_| "{}".to_string());

    let (options, available) = render_avatar_select_options(
        &hakushin,
        &owned,
        locale,
        &filter_element,
        &filter_rank,
    );
    let any_unowned = hakushin.avatars.keys().any(|id| !owned.contains(id));

    let filter_panel = render_avatar_filter_panel(locale, &filter_element, &filter_rank);
    let unavailable = if any_unowned && available == 0 {
        format!("<p class=\"meta\">{}</p>", t(locale, "avatar.no_matches"))
    } else if !any_unowned {
        format!("<p class=\"meta\">{}</p>", t(locale, "avatar.all_owned"))
    } else {
        String::new()
    };

    let inner = format!(
        r#"        <h1>{new_title}</h1>
        {filter_panel}
        {unavailable}
        <div class="panel" style="display:block;">
        <form method="post">
            {csrf}
            <div>
                <img id="avatar_preview" class="preview-img" />
                <label>{avatar_label}</label>
                <select name="avatar_id" id="avatar_id" required>
                    {options}
                </select>
            </div>
            <div class="form-actions">
                <a href="/dashboard?tab=avatars" class="back">{back_label}</a>
                <button type="submit">{create_label}</button>
            </div>
        </form>
        </div>
    <script>
    var a = {avatar_images_json};
    var p = document.getElementById("avatar_preview");
    var s = document.getElementById("avatar_id");
    s.addEventListener("change", function() {{
        var u = a[s.value];
        if (u) {{ p.src = u; p.style.display = "block"; }}
        else {{ p.style.display = "none"; }}
    }});
    </script>"#,
        options = options,
        avatar_images_json = avatar_images_json,
        new_title = t(locale, "avatar.new"),
        avatar_label = t(locale, "avatar.select"),
        create_label = t(locale, "avatar.create"),
        back_label = t(locale, "avatar.back"),
        unavailable = unavailable,
        filter_panel = filter_panel,
        csrf = csrf_input(&auth.session_id),
    );

    Html(page_shell(t(locale, "avatar.new"), locale, &inner)).into_response()
}

fn avatar_values_valid(payload: &AvatarUpdateForm) -> bool {
    (1..=60).contains(&payload.level)
        && payload.unlocked_talent_num <= 6
        && (1..=12).contains(&payload.skill_common_attack)
        && (1..=12).contains(&payload.skill_special_attack)
        && (1..=12).contains(&payload.skill_evade)
        && (1..=12).contains(&payload.skill_cooperate_skill)
        && (1..=7).contains(&payload.core_ability)
        && (1..=12).contains(&payload.skill_assist_skill)
}

pub(crate) async fn avatar_add(
    auth: AuthContext,
    Form(payload): Form<AddAvatarForm>,
) -> impl IntoResponse {
    if let Err(response) = auth.require_csrf(&payload._csrf) {
        return response;
    }

    let active_state = &auth.state;
    let locale = auth.locale;
    let session = &auth.session;
    let uid = match auth.player_uid() {
        Ok(uid) => uid,
        Err(response) => return response,
    };

    let addr = active_state.ctl_addr.clone();
    if !ctl::player_is_online(&addr, uid) {
        return Html(t(locale, "player.offline")).into_response();
    }

    // Server-side range validation instead of silent `as u32` truncation.
    let Some(avatar_id) = u16_from_u32(payload.avatar_id) else {
        return (StatusCode::BAD_REQUEST, Html(t(locale, "avatar.invalid_id"))).into_response();
    };
    let avatar_id = avatar_id as u32;

    // Reject characters the account already owns; the server NAKs them too.
    let owned = load_player_save(active_state, uid)
        .map(|save| save.avatar.iter().any(|a| a.id == avatar_id))
        .unwrap_or(false);
    if owned {
        return (StatusCode::CONFLICT, Html(t(locale, "avatar.already_owned"))).into_response();
    }

    if let Err(e) = ctl::create_avatar(&addr, uid, avatar_id) {
        return Html(format!("ctl error: {e}")).into_response();
    }
    if let Err(e) = ctl::save_player(&addr, uid) {
        return Html(format!("ctl error (save): {e}")).into_response();
    }

    audit_log(
        &active_state.root_dir,
        &session.username,
        session.uid,
        "avatar_add",
        &format!("avatar_id={avatar_id}"),
    );

    Redirect::to("/dashboard?tab=avatars").into_response()
}

pub(crate) fn render_avatar_cards(state: &AppState, uid: u32, locale: Locale, online: bool) -> String {
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

    let add_panel = if online {
        format!(
            "<div class=\"panel\"><h3>{}</h3><div style=\"display:flex; gap:8px;\"><a href=\"/avatar/new\">{}</a></div></div>",
            t(locale, "avatar.add"),
            t(locale, "avatar.new")
        )
    } else {
        String::new()
    };

    format!("{add_panel}<div class=\"cards\">{cards}</div>")
}

fn render_avatar_filter_panel(locale: Locale, filter_element: &str, filter_rank: &str) -> String {
    let element_opts = {
        let mut html = format!(
            "<option value=\"\"{sel}>{label}</option>",
            sel = if filter_element.is_empty() { " selected" } else { "" },
            label = t(locale, "avatar.filter_all")
        );
        for (value, key) in [
            ("physical", "element.physical"),
            ("fire", "element.fire"),
            ("ice", "element.ice"),
            ("electric", "element.electric"),
            ("wind", "element.wind"),
            ("ether", "element.ether"),
        ] {
            html.push_str(&format!(
                "<option value=\"{value}\"{sel}>{label}</option>",
                sel = if filter_element == value { " selected" } else { "" },
                label = t(locale, key)
            ));
        }
        html
    };
    let rank_opts = format!(
        "<option value=\"\"{all_sel}>{all}</option><option value=\"4\"{s_sel}>{s}</option><option value=\"3\"{a_sel}>{a}</option>",
        all_sel = if filter_rank.is_empty() { " selected" } else { "" },
        s_sel = if filter_rank == "4" { " selected" } else { "" },
        a_sel = if filter_rank == "3" { " selected" } else { "" },
        all = t(locale, "avatar.filter_all"),
        s = t(locale, "weapon.rarity_s"),
        a = t(locale, "weapon.rarity_a"),
    );

    format!(
        r#"<form method="get" action="/avatar/new" style="margin-bottom:12px;">
            <div style="display:flex; gap:8px; flex-wrap:wrap; align-items:end;">
                <div style="display:flex; flex-direction:column; gap:4px;">
                    <span style="font-size:11px; color:#9aa4b2;">{element_label}</span>
                    <select name="element" onchange="this.form.submit()" style="width:auto; padding:5px 8px; border-radius:8px; border:1px solid #2a3140; background:#121620; color:#e6e6e6; font-size:12px;">{element_opts}</select>
                </div>
                <div style="display:flex; flex-direction:column; gap:4px;">
                    <span style="font-size:11px; color:#9aa4b2;">{rank_label}</span>
                    <select name="rank" onchange="this.form.submit()" style="width:auto; padding:5px 8px; border-radius:8px; border:1px solid #2a3140; background:#121620; color:#e6e6e6; font-size:12px;">{rank_opts}</select>
                </div>
            </div>
        </form>"#,
        element_label = t(locale, "avatar.element"),
        rank_label = t(locale, "avatar.rank"),
        element_opts = element_opts,
        rank_opts = rank_opts,
    )
}

fn avatar_element_name(element: u32) -> &'static str {
    match element {
        200 => "physical",
        201 => "fire",
        202 => "ice",
        203 => "electric",
        204 => "wind",
        205 => "ether",
        _ => "",
    }
}

fn render_avatar_select_options(
    hakushin: &crate::data::hakushin::HakushinData,
    owned: &std::collections::HashSet<u32>,
    locale: Locale,
    filter_element: &str,
    filter_rank: &str,
) -> (String, usize) {
    let mut items: Vec<(u32, String)> = hakushin
        .avatars
        .iter()
        .filter(|(id, _)| !owned.contains(id))
        .filter(|(id, _)| {
            let info = hakushin.avatar_info.get(id);
            if !filter_element.is_empty()
                && info
                    .map(|i| avatar_element_name(i.element))
                    .unwrap_or("")
                    != filter_element
            {
                return false;
            }
            if !filter_rank.is_empty() {
                let rarity = info.map(|i| i.rarity).unwrap_or(0);
                if rarity.to_string() != filter_rank {
                    return false;
                }
            }
            true
        })
        .map(|(id, entry)| (*id, entry.name.clone()))
        .collect();
    items.sort_by(|a, b| a.1.cmp(&b.1));

    let available = items.len();
    let mut html = String::new();
    html.push_str(&format!(
        "<option value=\"\" disabled selected>{}</option>",
        t(locale, "avatar.select")
    ));
    for (id, name) in items {
        html.push_str(&format!("<option value=\"{id}\">{name}</option>"));
    }
    (html, available)
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
            "<div><label>{label}</label><input name=\"{name}\" type=\"number\" min=\"1\" max=\"12\" value=\"{value}\" {disabled} /></div>",
            label = t(locale, label_key),
        ));
    }

    let core_ability = skill_levels.get(5).copied().unwrap_or(1);
    html.push_str(&format!(
        "<div><label>{label}</label><input name=\"core_ability\" type=\"number\" min=\"1\" max=\"7\" value=\"{core_ability}\" {disabled} /></div>",
        label = t(locale, "avatar.core_ability"),
    ));

    html
}
