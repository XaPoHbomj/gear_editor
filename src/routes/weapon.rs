use crate::{
    app_state::AppState,
    auth::{AuthContext, csrf_input, html_escape_attr},
    ctl,
    data::hakushin::{load_hakushin_data, to_asset_url},
    i18n::{Locale, t},
    player_state::load_player_save,
    utils::{audit_log, page_shell, svg_data_uri, u8_from_u32, u16_from_u32},
};
use axum::{
    extract::{Form, Path, Query},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
pub(crate) struct WeaponUpdateForm {
    level: u32,
    refine_level: u32,
    _csrf: String,
}

#[derive(Deserialize)]
pub(crate) struct AddWeaponForm {
    pub(crate) weapon_id: u32,
    pub(crate) refine_level: u32,
    _csrf: String,
}

#[derive(Deserialize)]
pub(crate) struct WeaponFilterQuery {
    pub(crate) class: Option<String>,
    pub(crate) rarity: Option<String>,
}

pub(crate) async fn weapon_edit(
    auth: AuthContext,
    Path(weapon_uid): Path<u32>,
) -> impl IntoResponse {
    let active_state = &auth.state;
    let locale = auth.locale;
    let uid = match auth.player_uid() {
        Ok(uid) => uid,
        Err(response) => return response,
    };

    let Some(save) = load_player_save(active_state, uid) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "weapon.not_found"))).into_response();
    };

    let Some(weapon) = save.weapon.iter().find(|w| w.uid == weapon_uid) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "weapon.not_found"))).into_response();
    };

    let level = weapon.level;
    let refine_level = weapon.refine;
    let weapon_id = weapon.id;
    let online = ctl::player_is_online(&active_state.ctl_addr, uid);
    let hakushin = load_hakushin_data(active_state, locale);
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

    let inner = format!(
        r#"        <div class="hero">
            <img src="{weapon_img}" alt="{weapon_name}" />
            <div>
                <h1>{edit_title} {weapon_name}</h1>
                <div class="meta">{uid_label} {weapon_uid} · {id_label} {weapon_id}</div>
            </div>
        </div>
    <form method="post">
      {csrf}
      <label>{level_label}</label>
      <input name="level" type="number" min="1" value="{level}" {disabled} />
      <label>{overclock_label}</label>
      <input name="refine_level" type="number" min="1" value="{refine_level}" {disabled} />
      <div class="form-actions">
        <a href="/dashboard?tab=weapons" class="back">{back_label}</a>
        {submit}
      </div>
    </form>"#,
        weapon_uid = weapon_uid,
        weapon_id = weapon_id,
        weapon_name = html_escape_attr(&weapon_name),
        weapon_img = html_escape_attr(&weapon_img),
        level = level,
        refine_level = refine_level,
        disabled = if online { "" } else { "disabled" },
        submit = if online {
            format!("<button type=\"submit\">{}</button>", t(locale, "weapon.save"))
        } else {
            String::new()
        },
        edit_title = t(locale, "weapon.edit"),
        uid_label = t(locale, "weapon.uid"),
        id_label = t(locale, "weapon.id"),
        level_label = t(locale, "weapon.level"),
        overclock_label = t(locale, "weapon.overclock"),
        back_label = t(locale, "weapon.back"),
        csrf = csrf_input(&auth.session_id),
    );

    Html(page_shell(t(locale, "weapon.edit"), locale, &inner)).into_response()
}

pub(crate) async fn weapon_update(
    auth: AuthContext,
    Path(weapon_uid): Path<u32>,
    Form(payload): Form<WeaponUpdateForm>,
) -> impl IntoResponse {
    if let Err(response) = auth.require_csrf(&payload._csrf) {
        return response;
    }

    let active_state = &auth.state;
    let locale = auth.locale;
    let uid = match auth.player_uid() {
        Ok(uid) => uid,
        Err(response) => return response,
    };

    let addr = active_state.ctl_addr.clone();
    if !ctl::player_is_online(&addr, uid) {
        return Html(t(locale, "player.offline")).into_response();
    }

    // Ownership check: the weapon must belong to this player's save.
    let belongs = load_player_save(active_state, uid)
        .map(|s| s.weapon.iter().any(|w| w.uid == weapon_uid))
        .unwrap_or(false);
    if !belongs {
        return (StatusCode::NOT_FOUND, Html(t(locale, "weapon.not_found"))).into_response();
    }

    // Server-side range validation instead of silent `as u8` truncation.
    let Some(level) = u8_from_u32(payload.level).filter(|&l| (1..=60).contains(&l)) else {
        return (StatusCode::BAD_REQUEST, Html(t(locale, "weapon.level_out_of_range"))).into_response();
    };
    let Some(refine) = u8_from_u32(payload.refine_level).filter(|&r| (1..=5).contains(&r)) else {
        return (StatusCode::BAD_REQUEST, Html(t(locale, "weapon.refine_out_of_range"))).into_response();
    };

    if let Err(e) = ctl::mod_weapon(
        &addr,
        uid,
        weapon_uid,
        level,
        5,
        refine,
    ) {
        return Html(format!("ctl error: {e}")).into_response();
    }
    if let Err(e) = ctl::save_player(&addr, uid) {
        return Html(format!("ctl error (save): {e}")).into_response();
    }

    Redirect::to("/dashboard?tab=weapons").into_response()
}

pub(crate) async fn weapon_new(
    auth: AuthContext,
    Query(query): Query<WeaponFilterQuery>,
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

    let filter_class = query.class.unwrap_or_default();
    let filter_rarity = query.rarity.unwrap_or_default();
    let options = render_weapon_select_options(active_state, 0, locale, &filter_class, &filter_rarity);

    let hakushin = load_hakushin_data(active_state, locale);
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

    let inner = format!(
        r#"        <h1>{new_title}</h1>
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
            {csrf}
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
                    <input name="refine_level" type="number" min="1" value="1" />
                </div>
            </div>
            <button type="submit">{create_label}</button>
        </form>
    <script>
    var w = {weapon_images_json};
    var p = document.getElementById("weapon_preview");
    var s = document.getElementById("weapon_id");
    s.addEventListener("change", function() {{
        var u = w[s.value];
        if (u) {{ p.src = u; p.style.display = "block"; }}
        else {{ p.style.display = "none"; }}
    }});
    </script>"#,
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
        csrf = csrf_input(&auth.session_id),
    );

    Html(page_shell(t(locale, "weapon.new"), locale, &inner)).into_response()
}

pub(crate) async fn weapon_add(
    auth: AuthContext,
    Form(payload): Form<AddWeaponForm>,
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

    // Server-side range validation instead of silent `as u16`/`as u8`.
    let Some(weapon_id) = u16_from_u32(payload.weapon_id) else {
        return (StatusCode::BAD_REQUEST, Html(t(locale, "weapon.id_out_of_range"))).into_response();
    };
    let Some(refine) = u8_from_u32(payload.refine_level).filter(|&r| (1..=5).contains(&r)) else {
        return (StatusCode::BAD_REQUEST, Html(t(locale, "weapon.refine_out_of_range"))).into_response();
    };

    if let Err(e) = ctl::create_weapon(
        &addr,
        uid,
        weapon_id,
        60,
        5,
        refine,
    ) {
        return Html(format!("ctl error: {e}")).into_response();
    }
    if let Err(e) = ctl::save_player(&addr, uid) {
        return Html(format!("ctl error (save): {e}")).into_response();
    }

    audit_log(
        &active_state.root_dir,
        &session.username,
        session.uid,
        "weapon_add",
        &format!("weapon_id={}", payload.weapon_id),
    );
    Redirect::to("/dashboard?tab=weapons").into_response()
}

pub(crate) fn render_weapon_cards(
    state: &AppState,
    uid: u32,
    locale: Locale,
    filter_class: &str,
    filter_rarity: &str,
    online: bool,
) -> String {
    let hakushin = load_hakushin_data(state, locale);

    let mut cards = String::new();
    if let Some(save) = load_player_save(state, uid) {
        for weapon in &save.weapon {
            let weapon_uid = weapon.uid;
            let weapon_id = weapon.id;
            let level = weapon.level;

            let info = hakushin.weapon_info.get(&weapon_id);

            if !filter_class.is_empty()
                && info.map(|i| i.weapon_type.as_str()).unwrap_or("") != filter_class {
                    continue;
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
    let add_panel = if online {
        render_add_weapon_panel(state, locale)
    } else {
        String::new()
    };
    format!("{add_panel}{filter_panel}<div class=\"cards\">{cards}</div>")
}

fn render_weapon_filter_panel(locale: Locale, filter_class: &str, filter_rarity: &str) -> String {
    let class_opts = {
        let all_sel = if filter_class.is_empty() {
            " selected"
        } else {
            ""
        };
        let attack_sel = if filter_class == "Attack" {
            " selected"
        } else {
            ""
        };
        let stun_sel = if filter_class == "Stun" {
            " selected"
        } else {
            ""
        };
        let anomaly_sel = if filter_class == "Anomaly" {
            " selected"
        } else {
            ""
        };
        let defense_sel = if filter_class == "Defense" {
            " selected"
        } else {
            ""
        };
        let rupture_sel = if filter_class == "Rupture" {
            " selected"
        } else {
            ""
        };
        let support_sel = if filter_class == "Support" {
            " selected"
        } else {
            ""
        };
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
        let all_sel = if filter_rarity.is_empty() {
            " selected"
        } else {
            ""
        };
        let s_sel = if filter_rarity == "s" {
            " selected"
        } else {
            ""
        };
        let a_sel = if filter_rarity == "a" {
            " selected"
        } else {
            ""
        };
        let b_sel = if filter_rarity == "b" {
            " selected"
        } else {
            ""
        };
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

fn render_weapon_select_options(
    state: &AppState,
    selected_id: u32,
    locale: Locale,
    filter_class: &str,
    filter_rarity: &str,
) -> String {
    let hakushin = load_hakushin_data(state, locale);
    let mut items: Vec<(u32, String)> = hakushin
        .weapons
        .iter()
        .filter(|(id, _)| {
            let info = hakushin.weapon_info.get(id);
            if !filter_class.is_empty()
                && info.map(|i| i.weapon_type.as_str()).unwrap_or("") != filter_class {
                    return false;
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
    let all_sel = if filter_class.is_empty() {
        " selected"
    } else {
        ""
    };
    let attack_sel = if filter_class == "Attack" {
        " selected"
    } else {
        ""
    };
    let stun_sel = if filter_class == "Stun" {
        " selected"
    } else {
        ""
    };
    let anomaly_sel = if filter_class == "Anomaly" {
        " selected"
    } else {
        ""
    };
    let defense_sel = if filter_class == "Defense" {
        " selected"
    } else {
        ""
    };
    let rupture_sel = if filter_class == "Rupture" {
        " selected"
    } else {
        ""
    };
    let support_sel = if filter_class == "Support" {
        " selected"
    } else {
        ""
    };
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
    let all_sel = if filter_rarity.is_empty() {
        " selected"
    } else {
        ""
    };
    let s_sel = if filter_rarity == "s" {
        " selected"
    } else {
        ""
    };
    let a_sel = if filter_rarity == "a" {
        " selected"
    } else {
        ""
    };
    let b_sel = if filter_rarity == "b" {
        " selected"
    } else {
        ""
    };
    format!(
        "<option value=\"\"{all_sel}>{all}</option><option value=\"s\"{s_sel}>{s}</option><option value=\"a\"{a_sel}>{a}</option><option value=\"b\"{b_sel}>{b}</option>",
        all = t(locale, "weapon.filter_all"),
        s = t(locale, "weapon.rarity_s"),
        a = t(locale, "weapon.rarity_a"),
        b = t(locale, "weapon.rarity_b"),
    )
}
