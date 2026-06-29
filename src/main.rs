use axum::{
    Router,
    extract::{DefaultBodyLimit, OriginalUri, Query, State},
    http::{HeaderMap, header},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;
use std::{env, path::PathBuf};
use tower_http::compression::CompressionLayer;

mod app_state;
mod assets;
mod auth;
mod config;
mod data;
mod domain;
mod i18n;
mod player_state;
mod remielle_save;
mod routes;
mod updates;
mod utils;
mod zon;

use app_state::AppState;
use assets::asset_handler;
use auth::{get_session, is_admin, redirect_to_login, sanitize_next_path, url_encode_component};
use config::{load_sdk_config, resolve_db_path, resolve_sdk_config_path};
use i18n::{Locale, locale_from_headers, t};
use player_state::resolve_player_uid;
use routes::auth::{login, login_page, logout};
use routes::avatar::{avatar_add_all, avatar_edit, avatar_update, render_avatar_cards};
use routes::bangboo::{bangboo_add_all, bangboo_edit, bangboo_update, render_bangboo_cards};
use routes::challenges::{da_detail, render_da_shiyu_status, shiyu_detail};
use routes::admin::{admin_delete_update, admin_upload_update};
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
    set_id: Option<String>,
    slot: Option<String>,
    main_stat: Option<String>,
    status: Option<String>,
    page: Option<String>,
    weapon_class: Option<String>,
    weapon_rarity: Option<String>,
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
        .unwrap_or_else(|_| root.join("bin_remielle/Persistent/LocalStorage"));
    let asset_dir = env::var("GEAR_ASSET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("bin_remielle/assets/filecfg"));
    let dump_dir = env::var("ZZZ_DUMP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("zzz_dump/latest"));

    let state = AppState {
        db_path,
        state_dir,
        asset_dir,
        dump_dir,
        root_dir: root,
    };

    let app = Router::new()
        .route("/", get(login_page))
        .route("/login", post(login))
        .route("/dashboard", get(dashboard))
        .route("/logout", get(logout))
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
        .route("/admin/upload-update", post(admin_upload_update).layer(DefaultBodyLimit::disable()))
        .route("/admin/delete-update", post(admin_delete_update))
        .route("/da/:id", get(da_detail))
        .route("/shiyu/:id", get(shiyu_detail))
        .route("/apply", post(apply_changes))
        .route("/set-lang", get(set_language))
        .route("/assets/*path", get(asset_handler))
        .layer(CompressionLayer::new())
        .with_state(state);

    let addr = env::var("GEAR_EDITOR_ADDR").unwrap_or_else(|_| "127.0.0.1:3001".to_string());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind GEAR_EDITOR_ADDR");

    tokio::spawn(async {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            let removed = auth::gc_sessions(std::time::Duration::from_secs(86400));
            if removed > 0 {
                println!("session GC: removed {} inactive sessions", removed);
            }
        }
    });

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
    let filter_set_id = query.set_id.and_then(|s| s.parse::<u32>().ok());
    let filter_slot = query.slot.and_then(|s| s.parse::<u32>().ok());
    let filter_main_stat = query.main_stat.and_then(|s| s.parse::<u32>().ok());
    let filter_page = query.page.and_then(|s| s.parse::<u32>().ok()).unwrap_or(1);
    let filter_weapon_class = query.weapon_class.unwrap_or_default();
    let filter_weapon_rarity = query.weapon_rarity.unwrap_or_default();
    let uid = resolve_player_uid(&state, session.uid);
    let version = state.read_version();
    let is_admin = is_admin(&session);

    let pending_count = session.pending_writes.len();
    let locale = locale_from_headers(&headers);
    let server_host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("localhost:3001");

    let encoded_next = url_encode_component("/dashboard");
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

    let tab_avatars = if tab == "avatars" { "active" } else { "" };
    let tab_weapons = if tab == "weapons" { "active" } else { "" };
    let tab_discs = if tab == "discs" { "active" } else { "" };
    let tab_bangboos = if tab == "bangboos" { "active" } else { "" };
    let tab_updates = if tab == "updates" { "active" } else { "" };
    let tab_status = if tab == "status" { "active" } else { "" };

    let title_suffix = if version.is_empty() {
        format!(" — Remielle")
    } else {
        format!(" — Remielle {version}")
    };

    let body = format!(
        r#"<!doctype html>
<html lang="{lang_attr}">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Gear Editor{title_suffix}</title>
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

    .meta {{ color: #9aa4b2; font-size: 12px; }}
        .panel {{ background: #1b1f2a; padding: 14px; border-radius: 12px; border: 1px solid #232a38; margin-bottom: 16px; display: flex; align-items: center; justify-content: space-between; gap: 12px; }}
        .panel h3 {{ margin: 0; font-size: 14px; }}
        .panel a {{ display: inline-flex; align-items: center; justify-content: center; margin-top: 16px; padding: 10px 14px; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; text-decoration: none; box-sizing: border-box; }}
        .panel form {{ width: 100%; }}
        .panel label {{ display: block; margin: 12px 0 6px; font-size: 12px; color: #9aa4b2; }}
        .panel input, .panel select {{ width: 100%; box-sizing: border-box; padding: 8px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
        .panel button {{ margin-top: 16px; padding: 10px 14px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; font-family: inherit; font-size: inherit; }}
        .panel div button, .panel div form button, .panel div a {{ margin-top: 0; }}
        .row {{ display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }}
        .row > * {{ min-width: 0; }}
    .apply {{ background: #22c55e; color: #0b1220; border: 0; padding: 8px 14px; border-radius: 8px; font-weight: 600; cursor: pointer; }}
    .pill {{ display: inline-block; padding: 4px 8px; background: #2a3140; border-radius: 999px; font-size: 12px; color: #9aa4b2; }}
        .danger {{ background: #ef4444; color: #fff; border: 0; padding: 8px 14px; border-radius: 8px; font-weight: 600; cursor: pointer; }}
        .select-card {{ cursor: pointer; position: relative; }}
        .select-card input[type="checkbox"] {{ position: absolute; opacity: 0; pointer-events: none; }}
        .select-card.locked {{ opacity: 0.5; cursor: not-allowed; }}
        .select-card .selection-outline {{ display: none; position: absolute; inset: -1px; border-radius: inherit; pointer-events: none; }}
        .delete-form .select-card .selection-outline {{ border: 2px solid #ef4444; box-shadow: inset 0 0 0 1px rgba(239, 68, 68, 0.18); }}
        .lock-form .select-card .selection-outline {{ border: 2px solid #22c55e; box-shadow: inset 0 0 0 1px rgba(34, 197, 94, 0.18); }}
        .delete-form .select-card input:checked ~ .selection-outline {{ display: block; }}
        .lock-form .select-card input:checked ~ .selection-outline {{ display: block; }}
        .delete-form .select-card:has(input:checked) {{ border-color: #ef4444; }}
        .lock-form .select-card:has(input:checked) {{ border-color: #22c55e; }}
        .mobile-overlay {{ display: none; }}
        .mobile-drawer {{ display: none; }}
    @media (max-width: 768px) {{
        header {{ padding: 12px 14px; }}
        .lang-select {{ display: none; }}
        .menu-button {{ display: inline-flex; flex: 0 0 auto; }}
        .desktop-tabs {{ display: none; }}
        .desktop-logout {{ display: none; }}
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
        .mobile-drawer.tabs {{ display: flex; flex-direction: column; align-items: stretch; justify-content: flex-start; gap: 8px; }}
        .mobile-drawer.tabs a {{ display: block; width: 100%; box-sizing: border-box; white-space: normal; line-height: 1.25; }}
        .mobile-drawer select {{ width: 100%; }}
    }}
  </style>
</head>
<body>
<header>
    <button class="menu-button" type="button" aria-label="Open navigation" onclick="document.querySelector('.mobile-overlay').classList.add('open'); document.querySelector('.mobile-drawer').classList.add('open');">
        <span></span>
    </button>
    <div class="desktop-tabs tabs">
        <a class="{tab_avatars}" href="/dashboard?tab=avatars">{nav_characters}</a>
        <a class="{tab_weapons}" href="/dashboard?tab=weapons">{nav_weapons}</a>
        <a class="{tab_discs}" href="/dashboard?tab=discs">{nav_discs}</a>
        <a class="{tab_bangboos}" href="/dashboard?tab=bangboos">{nav_bangboos}</a>
        <a class="{tab_updates}" href="/dashboard?tab=updates">{nav_client_updates}</a>
        <a class="{tab_status}" href="/dashboard?tab=status">{nav_status}</a>
    </div>
    <div class="desktop-actions" style="display:flex; align-items:center; gap:10px;">
        <div class="meta">{signed_in_as} {username}</div>
        <div class="lang-select">{lang_selector}</div>
        <a href="/logout" class="desktop-logout" style="padding:6px 10px; border-radius:8px; background:#2a3140; color:#c7d1e0; text-decoration:none; font-size:12px; font-weight:600;">{logout_label}</a>
        <form method="post" action="/apply" style="margin:0;">
            <input type="hidden" name="session" value="{session_id}" />
            <button class="apply" type="submit">{apply_changes} ({pending_count})</button>
        </form>
    </div>
</header>
<div class="mobile-overlay" onclick="this.classList.remove('open'); document.querySelector('.mobile-drawer').classList.remove('open');"></div>
<aside class="mobile-drawer tabs" aria-hidden="true">
    <a class="{tab_avatars}" href="/dashboard?tab=avatars">{nav_characters}</a>
    <a class="{tab_weapons}" href="/dashboard?tab=weapons">{nav_weapons}</a>
    <a class="{tab_discs}" href="/dashboard?tab=discs">{nav_discs}</a>
    <a class="{tab_bangboos}" href="/dashboard?tab=bangboos">{nav_bangboos}</a>
    <a class="{tab_updates}" href="/dashboard?tab=updates">{nav_client_updates}</a>
    <a class="{tab_status}" href="/dashboard?tab=status">{nav_status}</a>
    <div style="margin-top:16px; padding-top:12px; border-top:1px solid #2a3140; display:flex; flex-direction:column; gap:10px; width:100%; box-sizing:border-box;">
        <div class="meta">{signed_in_as} {username}</div>
        {lang_selector}
        <a href="/logout" style="text-align:center; padding:6px 10px; border-radius:8px; background:#2a3140; color:#c7d1e0; text-decoration:none; font-size:12px; font-weight:600;">{logout_label}</a>
    </div>
</aside>
<main class="content">
<div class="container">
  {content}
</div>
</main>
</body>
</html>"#,
        tab_avatars = tab_avatars,
        tab_weapons = tab_weapons,
        tab_discs = tab_discs,
        tab_bangboos = tab_bangboos,
        tab_status = tab_status,

        content = match tab.as_str() {
            "weapons" => render_weapon_cards(&state, uid, locale, &filter_weapon_class, &filter_weapon_rarity),
            "discs" => render_equip_cards(&state, uid, delete_mode, lock_mode, locale, filter_set_id, filter_slot, filter_main_stat, query.status.as_deref(), filter_page),
            "bangboos" => render_bangboo_cards(&state, uid, locale),

            "updates" => render_client_updates_panel(&state, server_host, locale, is_admin),
            "status" => render_status_tab(&state, uid, locale),
            _ => render_avatar_cards(&state, uid, locale),
        },
        session_id = session_id,
        username = session.username,
        pending_count = pending_count,
        lang_selector = lang_selector,
        lang_attr = locale.lang_attr(),
        title_suffix = title_suffix,
        nav_characters = t(locale, "nav.characters"),
        nav_weapons = t(locale, "nav.weapons"),
        nav_discs = t(locale, "nav.discs"),
        nav_bangboos = t(locale, "nav.bangboos"),

        nav_client_updates = t(locale, "nav.client_updates"),
        nav_status = t(locale, "nav.status"),
        signed_in_as = t(locale, "header.signed_in_as"),
        apply_changes = t(locale, "header.apply_changes"),
        logout_label = t(locale, "header.logout"),
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

fn render_status_tab(state: &AppState, uid: u32, locale: Locale) -> String {
    render_da_shiyu_status(state, uid, locale)
}
