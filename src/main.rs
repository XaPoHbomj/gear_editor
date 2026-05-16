use axum::{
    Router,
    extract::{OriginalUri, Query, State},
    http::{HeaderMap, header},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;
use std::{env, path::PathBuf};

mod app_state;
mod assets;
mod auth;
mod config;
mod data;
mod domain;
mod i18n;
mod player_state;
mod routes;
mod updates;
mod utils;
mod zon;

use app_state::{AppState, ServerMode, active_server_mode, state_with_active_server};
use assets::asset_handler;
use auth::{get_session, redirect_to_login, sanitize_next_path, url_encode_component};
use config::{load_sdk_config, resolve_db_path, resolve_sdk_config_path};
use i18n::{Locale, locale_from_headers, t};
use player_state::resolve_player_uid;
use routes::auth::{login, login_page, switch_server};
use routes::avatar::{avatar_add_all, avatar_edit, avatar_update, render_avatar_cards};
use routes::bangboo::{bangboo_add_all, bangboo_edit, bangboo_update, render_bangboo_cards};
use routes::challenges::{
    da_detail, da_select, da_shiyu_update, render_da_panel, render_shiyu_panel, shiyu_detail,
    shiyu_select,
};
use routes::equip::{
    equip_add, equip_delete_all_unlocked, equip_delete_submit, equip_edit, equip_generate,
    equip_generate_submit, equip_lock_selected, equip_new, equip_update, render_equip_cards,
};
use routes::weapon::{render_weapon_cards, weapon_add, weapon_edit, weapon_new, weapon_update};
use updates::render_client_updates_panel;
use utils::apply_changes;

#[derive(Deserialize)]
struct TabQuery {
    tab: Option<String>,
    delete: Option<u8>,
    lock: Option<u8>,
}

#[derive(Deserialize)]
struct SetLangQuery {
    lang: String,
    next: Option<String>,
}

#[tokio::main]
async fn main() {
    let config_path = resolve_sdk_config_path();
    let config = load_sdk_config(&config_path);
    let db_path = resolve_db_path(&config_path, &config.db_file);

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let state_dir = env::var("GEAR_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("yoshunko/state"));
    let prod_state_dir = env::var("GEAR_STATE_DIR_PROD")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("yoshunko_prod/state"));
    let asset_dir = root.join("yoshunko/assets/Filecfg");
    let prod_asset_dir = env::var("GEAR_ASSET_DIR_PROD")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("yoshunko_prod/assets/Filecfg"));
    let dump_dir = env::var("ZZZ_DUMP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("zzz_dump/latest"));

    let state = AppState {
        db_path,
        state_dir,
        prod_state_dir,
        asset_dir,
        prod_asset_dir,
        dump_dir,
        root_dir: root,
    };

    let app = Router::new()
        .route("/", get(login_page))
        .route("/login", post(login))
        .route("/dashboard", get(dashboard))
        .route("/switch-server", get(switch_server))
        .route("/avatar/:id", get(avatar_edit).post(avatar_update))
        .route("/avatar/add-all", post(avatar_add_all))
        .route("/weapon/:uid", get(weapon_edit).post(weapon_update))
        .route("/weapon/new", get(weapon_new).post(weapon_add))
        .route("/equip/:uid", get(equip_edit).post(equip_update))
        .route("/equip/new", get(equip_new).post(equip_add))
        .route(
            "/equip/generate",
            get(equip_generate).post(equip_generate_submit),
        )
        .route("/equip/delete", post(equip_delete_submit))
        .route(
            "/equip/delete-all-unlocked",
            post(equip_delete_all_unlocked),
        )
        .route("/equip/lock-selected", post(equip_lock_selected))
        .route("/bangboo/:uid", get(bangboo_edit).post(bangboo_update))
        .route("/bangboo/add-all", post(bangboo_add_all))
        .route("/da/:id", get(da_detail))
        .route("/da/:id/select", post(da_select))
        .route("/shiyu/:id", get(shiyu_detail))
        .route("/shiyu/:id/select", post(shiyu_select))
        .route("/da-shiyu", post(da_shiyu_update))
        .route("/apply", post(apply_changes))
        .route("/set-lang", get(set_language))
        .route("/assets/*path", get(asset_handler))
        .with_state(state);

    let addr = env::var("GEAR_EDITOR_ADDR").unwrap_or_else(|_| "0.0.0.0:18080".to_string());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind GEAR_EDITOR_ADDR");

    println!("gear_editor listening on http://{addr}");
    axum::serve(listener, app).await.unwrap();
}

async fn dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TabQuery>,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let tab = query.tab.unwrap_or_else(|| "avatars".to_string());
    let delete_mode = query.delete.unwrap_or(0) == 1;
    let lock_mode = query.lock.unwrap_or(0) == 1;
    let current_mode = active_server_mode(&headers);
    let active_state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&active_state, session.uid);
    let locale = locale_from_headers(&headers);

    let avatar_cards = render_avatar_cards(&active_state, uid, locale);
    let weapon_cards = render_weapon_cards(&active_state, uid, locale);
    let equip_cards = render_equip_cards(&active_state, uid, delete_mode, lock_mode, locale);
    let bangboo_cards = render_bangboo_cards(&active_state, uid, locale);
    let server_host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("localhost:18080");
    let updates_panel = render_client_updates_panel(&state, server_host, locale);
    let da_panel = render_da_panel(&active_state, uid, locale);
    let shiyu_panel = render_shiyu_panel(&active_state, uid, locale);

    let pending_count = session.pending_writes.len();
    let locale = locale_from_headers(&headers);
    let next = sanitize_next_path(
        original_uri
            .0
            .path_and_query()
            .map(|pq| pq.as_str())
            .unwrap_or("/dashboard"),
    )
    .unwrap_or_else(|| "/dashboard".to_string());
    let switch_beta_href = format!(
        "/switch-server?target=beta&next={}",
        url_encode_component(&next)
    );
    let switch_prod_href = format!(
        "/switch-server?target=prod&next={}",
        url_encode_component(&next)
    );

    let encoded_next = url_encode_component(&next);
    let mut lang_opts = String::new();
    for lang in Locale::all() {
        let selected = if locale == *lang { " selected" } else { "" };
        lang_opts.push_str(&format!(
            "<option value=\"{code}\"{selected}>{label}</option>",
            code = lang.code(),
            label = lang.label(),
            selected = selected,
        ));
    }
    let lang_selector = format!(
        "<select onchange=\"location.href='/set-lang?lang='+this.value+'&next={next}'\" style=\"padding:5px 8px; border-radius:8px; border:1px solid #2a3140; background:#121620; color:#e6e6e6; font-size:12px; font-weight:700; cursor:pointer;\">{opts}</select>",
        next = encoded_next,
        opts = lang_opts,
    );

    let body = format!(
        r#"<!doctype html>
<html lang="{lang_attr}">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Gear Editor</title>
  <style>
      body {{ font-family: system-ui, sans-serif; margin: 0; background: #0f1115; color: #e6e6e6; overflow-x: hidden; }}
      header {{ padding: 16px 24px; display: flex; justify-content: space-between; align-items: center; gap: 12px; background: #151a24; position: sticky; top: 0; z-index: 20; }}
        .menu-button {{ display: none; width: 44px; height: 44px; padding: 0; border: 0; border-radius: 10px; background: #2a3140; color: #fff; align-items: center; justify-content: center; cursor: pointer; }}
        .menu-button span {{ display: block; width: 18px; height: 2px; background: #fff; position: relative; }}
        .menu-button span::before, .menu-button span::after {{ content: ""; position: absolute; left: 0; width: 18px; height: 2px; background: #fff; }}
        .menu-button span::before {{ top: -6px; }}
        .menu-button span::after {{ top: 6px; }}
        .tabs {{ display: flex; flex-wrap: wrap; gap: 8px; min-width: 0; }}
        .tabs a {{ margin-right: 0; padding: 8px 12px; border-radius: 8px; text-decoration: none; color: #c7d1e0; background: #1b2230; white-space: nowrap; }}
    .tabs a.active {{ background: #4c7dff; color: #fff; }}
    .tabs a.mode {{ background: #2a3140; color: #c7d1e0; }}
        .container {{ padding: 20px 24px 40px; }}
    .cards {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 14px; }}
    .card {{ background: #1b1f2a; padding: 14px; border-radius: 12px; text-decoration: none; color: #e6e6e6; border: 1px solid #232a38; }}
    .card h3 {{ margin: 6px 0 8px; font-size: 16px; }}
    .thumb {{ display: block; width: 100%; height: 160px; object-fit: cover; object-position: top; border-radius: 8px; background: #0f1115; border: 1px solid #2a3140; }}
    .cards .card .thumb + .pill {{ margin-top: 8px; }}
    .boss-thumb-shiyu {{ display: block; width: 180px; min-width: 180px; flex: 0 0 180px; align-self: stretch; min-height: 0; background-color: #10141d; background-size: cover; background-repeat: no-repeat; background-position: center bottom; border-radius: 8px; }}
    .boss-thumb-da {{ display: block; width: 220px; min-width: 220px; flex: 0 0 220px; align-self: stretch; min-height: 0; background-color: #10141d; background-size: cover; background-repeat: no-repeat; background-position: center bottom; border-radius: 8px; }}
    .meta {{ color: #9aa4b2; font-size: 12px; }}
        .panel {{ background: #1b1f2a; padding: 14px; border-radius: 12px; border: 1px solid #232a38; margin-bottom: 16px; display: flex; align-items: center; justify-content: space-between; gap: 12px; }}
        .panel h3 {{ margin: 0; font-size: 14px; }}
        .panel a {{ display: inline-flex; align-items: center; justify-content: center; margin-top: 16px; padding: 10px 14px; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; text-decoration: none; box-sizing: border-box; }}
        .panel form {{ width: 100%; }}
        .panel label {{ display: block; margin: 12px 0 6px; font-size: 12px; color: #9aa4b2; }}
        .panel input, .panel select {{ width: 100%; box-sizing: border-box; padding: 8px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
        .panel button {{ margin-top: 16px; padding: 10px 14px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }}
        .panel div button, .panel div a {{ margin-top: 0; }}
        .row {{ display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }}
        .row > * {{ min-width: 0; }}
    .apply {{ background: #22c55e; color: #0b1220; border: 0; padding: 8px 14px; border-radius: 8px; font-weight: 600; cursor: pointer; }}
    .pill {{ display: inline-block; padding: 4px 8px; background: #2a3140; border-radius: 999px; font-size: 12px; color: #9aa4b2; }}
        .danger {{ background: #ef4444; color: #fff; border: 0; padding: 8px 14px; border-radius: 8px; font-weight: 600; cursor: pointer; }}
        .select-card {{ cursor: pointer; }}
        .select-card input[type="checkbox"] {{ position: absolute; opacity: 0; pointer-events: none; }}
        .select-card.locked {{ opacity: 0.5; cursor: not-allowed; }}
        .delete-form .select-card:has(input:checked) {{ border-color: #ef4444; }}
        .lock-form .select-card:has(input:checked) {{ border-color: #22c55e; }}
        .mobile-overlay {{ display: none; }}
        .mobile-drawer {{ display: none; }}
    @media (max-width: 768px) {{
        header {{ padding: 12px 14px; }}
        .lang-select {{ display: none; }}
        .menu-button {{ display: inline-flex; flex: 0 0 auto; }}
        .desktop-tabs {{ display: none; }}
        .container {{ padding: 14px; }}
        .cards {{ grid-template-columns: 1fr; }}
        .row {{ grid-template-columns: 1fr; }}
        .panel {{ flex-direction: column; align-items: stretch; }}
        .panel a, .panel button {{ width: 100%; max-width: 100%; box-sizing: border-box; }}
        .card {{ padding: 12px; min-width: 0; }}
        .meta {{ word-break: break-word; }}
        .mobile-overlay {{ display: none; position: fixed; inset: 0; z-index: 30; background: rgba(0, 0, 0, 0.45); }}
        .mobile-overlay.open {{ display: block; }}
        .mobile-drawer {{ display: block; position: fixed; top: 0; left: 0; bottom: 0; width: min(82vw, 320px); background: #151a24; border-right: 1px solid #232a38; padding: 16px; box-sizing: border-box; overflow-y: auto; transform: translateX(-100%); transition: transform 0.2s ease; z-index: 31; }}
        .mobile-drawer.open {{ transform: translateX(0); }}
        .mobile-drawer.tabs {{ display: flex; flex-direction: column; align-items: flex-start; justify-content: flex-start; gap: 8px; }}
        .mobile-drawer.tabs a {{ display: block; width: 100%; box-sizing: border-box; white-space: normal; line-height: 1.25; }}
    }}
  </style>
</head>
<body>
<header>
    <button class="menu-button" type="button" aria-label="Open navigation" onclick="document.querySelector('.mobile-overlay').classList.add('open'); document.querySelector('.mobile-drawer').classList.add('open');">
        <span></span>
    </button>
    <div class="desktop-tabs tabs">
        <a class="{tab_avatar}" href="/dashboard?tab=avatars">{nav_characters}</a>
        <a class="{tab_weapon}" href="/dashboard?tab=weapons">{nav_weapons}</a>
        <a class="{tab_equip}" href="/dashboard?tab=discs">{nav_discs}</a>
        <a class="{tab_bangboo}" href="/dashboard?tab=bangboos">{nav_bangboos}</a>
        <a class="{tab_da}" href="/dashboard?tab=da">{nav_deadly_assault}</a>
        <a class="{tab_shiyu}" href="/dashboard?tab=shiyu">{nav_shiyu}</a>
        <a class="{tab_updates}" href="/dashboard?tab=updates">{nav_client_updates}</a>
    </div>
    <div class="desktop-actions" style="display:flex; align-items:center; gap:10px;">
        <div class="meta">{signed_in_as} {username}</div>
        <div class="lang-select">{lang_selector}</div>
        <a href="{switch_beta_href}" style="padding:6px 10px; border-radius:999px; text-decoration:none; font-size:12px; font-weight:700; {beta_active}">{header_beta}</a>
        <a href="{switch_prod_href}" style="padding:6px 10px; border-radius:999px; text-decoration:none; font-size:12px; font-weight:700; {prod_active}">{header_prod}</a>
        <form method="post" action="/apply" style="margin:0;">
            <input type="hidden" name="session" value="{session_id}" />
            <button class="apply" type="submit">{apply_changes} ({pending_count})</button>
        </form>
    </div>
</header>
<div class="mobile-overlay" onclick="this.classList.remove('open'); document.querySelector('.mobile-drawer').classList.remove('open');"></div>
<aside class="mobile-drawer tabs" aria-hidden="true">
    <a class="{tab_avatar}" href="/dashboard?tab=avatars">{nav_characters}</a>
    <a class="{tab_weapon}" href="/dashboard?tab=weapons">{nav_weapons}</a>
    <a class="{tab_equip}" href="/dashboard?tab=discs">{nav_discs}</a>
    <a class="{tab_bangboo}" href="/dashboard?tab=bangboos">{nav_bangboos}</a>
    <a class="{tab_da}" href="/dashboard?tab=da">{nav_deadly_assault}</a>
    <a class="{tab_shiyu}" href="/dashboard?tab=shiyu">{nav_shiyu}</a>
    <a class="{tab_updates}" href="/dashboard?tab=updates">{nav_client_updates}</a>
    <div style="margin-top:16px; padding-top:12px; border-top:1px solid #2a3140; display:flex; flex-direction:column; gap:10px;">
        <div class="meta">{signed_in_as} {username}</div>
        {lang_selector}
        <div style="display:flex; gap:4px;">
            <a href="{switch_beta_href}" style="padding:6px 10px; border-radius:999px; text-decoration:none; font-size:12px; font-weight:700; {beta_active}">Beta</a>
            <a href="{switch_prod_href}" style="padding:6px 10px; border-radius:999px; text-decoration:none; font-size:12px; font-weight:700; {prod_active}">Prod</a>
        </div>
    </div>
</aside>
<main class="content">
<div class="container">
  {content}
</div>
</main>
</body>
</html>"#,
        tab_avatar = if tab == "avatars" { "active" } else { "" },
        tab_weapon = if tab == "weapons" { "active" } else { "" },
        tab_equip = if tab == "discs" { "active" } else { "" },
        tab_bangboo = if tab == "bangboos" { "active" } else { "" },
        tab_da = if tab == "da" { "active" } else { "" },
        tab_shiyu = if tab == "shiyu" { "active" } else { "" },
        tab_updates = if tab == "updates" { "active" } else { "" },
        content = match tab.as_str() {
            "weapons" => weapon_cards,
            "discs" => equip_cards,
            "bangboos" => bangboo_cards,
            "updates" => updates_panel,
            "da" => da_panel,
            "shiyu" => shiyu_panel,
            _ => avatar_cards,
        },
        session_id = session_id,
        username = session.username,
        pending_count = pending_count,
        switch_beta_href = switch_beta_href,
        switch_prod_href = switch_prod_href,
        lang_selector = lang_selector,
        lang_attr = locale.lang_attr(),
        nav_characters = t(locale, "nav.characters"),
        nav_weapons = t(locale, "nav.weapons"),
        nav_discs = t(locale, "nav.discs"),
        nav_bangboos = t(locale, "nav.bangboos"),
        nav_deadly_assault = t(locale, "nav.deadly_assault"),
        nav_shiyu = t(locale, "nav.shiyu"),
        nav_client_updates = t(locale, "nav.client_updates"),
        signed_in_as = t(locale, "header.signed_in_as"),
        apply_changes = t(locale, "header.apply_changes"),
        header_beta = t(locale, "header.beta"),
        header_prod = t(locale, "header.prod"),
        beta_active = if current_mode == ServerMode::Beta {
            "background:#4c7dff;color:#fff;"
        } else {
            "background:#2a3140;color:#c7d1e0;"
        },
        prod_active = if current_mode == ServerMode::Prod {
            "background:#4c7dff;color:#fff;"
        } else {
            "background:#2a3140;color:#c7d1e0;"
        },
    );

    Html(body).into_response()
}

async fn set_language(
    _headers: HeaderMap,
    Query(params): Query<SetLangQuery>,
) -> impl IntoResponse {
    let locale = params.lang.trim().parse::<Locale>().unwrap_or(Locale::En);
    let next = params
        .next
        .as_deref()
        .and_then(sanitize_next_path)
        .unwrap_or_else(|| "/dashboard".to_string());

    let mut response = Redirect::to(&next).into_response();
    let header_value = format!("gear_lang={}; Path=/; SameSite=Lax", locale.code())
        .parse()
        .unwrap();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, header_value);
    response
}
