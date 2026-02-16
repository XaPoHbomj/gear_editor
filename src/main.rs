use axum::{
    body::Body,
    extract::{Form, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use password_hash::PasswordHash;
use pbkdf2::Pbkdf2;
use rand::{distributions::Alphanumeric, Rng};
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::{
    collections::HashMap,
    env,
    fs,
    path::{Path as FsPath, PathBuf},
    sync::{Mutex, OnceLock},
};

static SESSION_STORE: OnceLock<Mutex<HashMap<String, Session>>> = OnceLock::new();
static HAKUSHIN_DATA: OnceLock<HakushinData> = OnceLock::new();
static EQUIP_TEMPLATE: OnceLock<EquipTemplateIndex> = OnceLock::new();
static STAT_NAMES: OnceLock<HashMap<u32, String>> = OnceLock::new();

#[derive(Clone)]
struct AppState {
    db_path: PathBuf,
    state_dir: PathBuf,
    asset_dir: PathBuf,
    dump_dir: PathBuf,
    root_dir: PathBuf,
}

#[derive(Default)]
struct HakushinData {
    avatars: HashMap<u32, HakushinEntry>,
    weapons: HashMap<u32, HakushinEntry>,
    discs: HashMap<u32, HakushinEntry>,
    bangboos: HashMap<u32, HakushinEntry>,
}

#[derive(Default, Clone)]
struct HakushinEntry {
    name: String,
    image_local: Option<String>,
}

#[derive(Default)]
struct EquipTemplateIndex {
    by_item: HashMap<u32, EquipTemplateInfo>,
    by_suit_slot: HashMap<(u32, u32), u32>,
}

#[derive(Clone, Copy)]
struct EquipTemplateInfo {
    suit_type: u32,
    slot: u32,
}

#[derive(Clone)]
struct Session {
    uid: i32,
    username: String,
    pending_writes: HashMap<PathBuf, String>,
}

#[derive(Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct SdkConfig {
    db_file: String,
}

#[derive(Deserialize)]
struct TabQuery {
    tab: Option<String>,
}

#[derive(Deserialize)]
struct AvatarUpdateForm {
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

#[derive(Deserialize)]
struct WeaponUpdateForm {
    level: u32,
    refine_level: u32,
}

#[derive(Deserialize)]
struct AddWeaponForm {
    weapon_id: u32,
    refine_level: u32,
}

#[derive(Deserialize)]
struct BangbooUpdateForm {
    level: u32,
    rank: u32,
    skill_manual: u32,
    skill_passive: u32,
    skill_qte: u32,
    skill_aid: u32,
}

#[derive(Deserialize)]
struct EquipUpdateForm {
    level: u32,
    main_key: u32,
    sub_key_1: u32,
    sub_proc_1: u32,
    sub_key_2: u32,
    sub_proc_2: u32,
    sub_key_3: u32,
    sub_proc_3: u32,
    sub_key_4: u32,
    sub_proc_4: u32,
}

#[derive(Deserialize)]
struct AddEquipForm {
    equip_set_id: u32,
    equip_slot: u32,
    main_key: u32,
    sub_key_1: u32,
    sub_proc_1: u32,
    sub_key_2: u32,
    sub_proc_2: u32,
    sub_key_3: u32,
    sub_proc_3: u32,
    sub_key_4: u32,
    sub_proc_4: u32,
}

#[derive(Deserialize)]
struct DaShiyuForm {
    shiyu_zone_id: u32,
    deadly_assault_zone_id: u32,
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
    let asset_dir = root.join("yoshunko/assets/Filecfg");
    let dump_dir = env::var("ZZZ_DUMP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| root.join("zzz_dump/latest/en"));

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
        .route("/avatar/:id", get(avatar_edit).post(avatar_update))
        .route("/weapon/:uid", get(weapon_edit).post(weapon_update))
        .route("/weapon/new", get(weapon_new).post(weapon_add))
        .route("/equip/:uid", get(equip_edit).post(equip_update))
        .route("/equip/new", get(equip_new).post(equip_add))
        .route("/bangboo/:uid", get(bangboo_edit).post(bangboo_update))
        .route("/da-shiyu", post(da_shiyu_update))
        .route("/apply", post(apply_changes))
        .route("/assets/*path", get(asset_handler))
        .with_state(state);

    let addr = env::var("GEAR_EDITOR_ADDR").unwrap_or_else(|_| "0.0.0.0:18080".to_string());
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind GEAR_EDITOR_ADDR");

    println!("gear_editor listening on http://{addr}");
    axum::serve(listener, app).await.unwrap();
}

async fn login_page() -> Html<&'static str> {
    Html(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Gear Editor - Login</title>
  <style>
    body { font-family: system-ui, sans-serif; background: #0f1115; color: #e6e6e6; display: grid; place-items: center; height: 100vh; margin: 0; }
    form { background: #1b1f2a; padding: 24px; border-radius: 12px; width: 320px; box-shadow: 0 10px 30px rgba(0,0,0,.4); }
    h1 { font-size: 18px; margin: 0 0 16px; }
    label { display: block; margin: 12px 0 6px; font-size: 12px; color: #9aa4b2; }
    input { width: 100%; padding: 10px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }
    button { margin-top: 16px; width: 100%; padding: 10px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }
  </style>
</head>
<body>
  <form method="post" action="/login">
    <h1>Gear Editor</h1>
    <label for="username">Username</label>
    <input id="username" name="username" autocomplete="username" required />
    <label for="password">Password</label>
    <input id="password" name="password" type="password" autocomplete="current-password" required />
    <button type="submit">Sign in</button>
  </form>
</body>
</html>"#,
    )
}

async fn login(State(state): State<AppState>, Form(payload): Form<LoginForm>) -> impl IntoResponse {
    let response: Response = match validate_login(&state.db_path, &payload.username, &payload.password) {
        Ok(Some(session)) => {
            let session_id = new_session_id();
            let store = SESSION_STORE.get_or_init(|| Mutex::new(HashMap::new()));
            store.lock().unwrap().insert(session_id.clone(), session);

            let mut headers = HeaderMap::new();
            headers.insert(
                header::SET_COOKIE,
                format!("ge_session={}; HttpOnly; SameSite=Lax; Path=/", session_id)
                    .parse()
                    .unwrap(),
            );

            (headers, Redirect::to("/dashboard")).into_response()
        }
        Ok(None) => (StatusCode::UNAUTHORIZED, Html("Invalid credentials")).into_response(),
        Err(_) => (StatusCode::INTERNAL_SERVER_ERROR, Html("Login failed")).into_response(),
    };

    response
}

async fn dashboard(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<TabQuery>,
) -> impl IntoResponse {
    let Some((session_id, session)) = get_session(&headers) else {
        return (StatusCode::UNAUTHORIZED, Html("Please log in")).into_response();
    };

    let tab = query.tab.unwrap_or_else(|| "avatars".to_string());
    let uid = resolve_player_uid(&state, session.uid);

    let avatar_cards = render_avatar_cards(&state, uid);
    let weapon_cards = render_weapon_cards(&state, uid);
    let equip_cards = render_equip_cards(&state, uid);
    let bangboo_cards = render_bangboo_cards(&state, uid);
    let da_shiyu_panel = render_da_shiyu_panel(&state, uid);

    let pending_count = session.pending_writes.len();

    let body = format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Gear Editor</title>
  <style>
    body {{ font-family: system-ui, sans-serif; margin: 0; background: #0f1115; color: #e6e6e6; }}
    header {{ padding: 16px 24px; display: flex; justify-content: space-between; align-items: center; background: #151a24; position: sticky; top: 0; }}
    .tabs a {{ margin-right: 12px; padding: 8px 12px; border-radius: 8px; text-decoration: none; color: #c7d1e0; background: #1b2230; }}
    .tabs a.active {{ background: #4c7dff; color: #fff; }}
    .container {{ padding: 20px 24px 40px; }}
    .cards {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 14px; }}
    .card {{ background: #1b1f2a; padding: 14px; border-radius: 12px; text-decoration: none; color: #e6e6e6; border: 1px solid #232a38; }}
    .card h3 {{ margin: 6px 0 8px; font-size: 16px; }}
    .thumb {{ width: 100%; height: 120px; object-fit: cover; object-position: top; border-radius: 8px; background: #0f1115; border: 1px solid #2a3140; }}
    .meta {{ color: #9aa4b2; font-size: 12px; }}
        .panel {{ background: #1b1f2a; padding: 14px; border-radius: 12px; border: 1px solid #232a38; margin-bottom: 16px; display: flex; align-items: center; justify-content: space-between; gap: 12px; }}
        .panel h3 {{ margin: 0; font-size: 14px; }}
        .panel a {{ display: inline-block; padding: 8px 12px; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; text-decoration: none; }}
        .row {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 12px; }}
    .apply {{ background: #22c55e; color: #0b1220; border: 0; padding: 8px 14px; border-radius: 8px; font-weight: 600; cursor: pointer; }}
    .pill {{ display: inline-block; padding: 4px 8px; background: #2a3140; border-radius: 999px; font-size: 12px; color: #9aa4b2; }}
  </style>
</head>
<body>
<header>
  <div class="tabs">
    <a class="{tab_avatar}" href="/dashboard?tab=avatars">Characters</a>
    <a class="{tab_weapon}" href="/dashboard?tab=weapons">Weapons</a>
    <a class="{tab_equip}" href="/dashboard?tab=discs">Discs</a>
    <a class="{tab_bangboo}" href="/dashboard?tab=bangboos">Bangboos</a>
    <a class="{tab_da_shiyu}" href="/dashboard?tab=da-shiyu">DA and Shiyu</a>
  </div>
    <div class="meta">Signed in as {username}</div>
  <form method="post" action="/apply">
    <input type="hidden" name="session" value="{session_id}" />
    <button class="apply" type="submit">Apply changes ({pending_count})</button>
  </form>
</header>
<div class="container">
  {content}
</div>
</body>
</html>"#,
        tab_avatar = if tab == "avatars" { "active" } else { "" },
        tab_weapon = if tab == "weapons" { "active" } else { "" },
        tab_equip = if tab == "discs" { "active" } else { "" },
        tab_bangboo = if tab == "bangboos" { "active" } else { "" },
        tab_da_shiyu = if tab == "da-shiyu" { "active" } else { "" },
        content = match tab.as_str() {
            "weapons" => weapon_cards,
            "discs" => equip_cards,
            "bangboos" => bangboo_cards,
            "da-shiyu" => da_shiyu_panel,
            _ => avatar_cards,
        },
        session_id = session_id,
        username = session.username,
        pending_count = pending_count,
    );

    Html(body).into_response()
}

async fn avatar_edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(avatar_id): Path<u32>,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return (StatusCode::UNAUTHORIZED, Html("Please log in")).into_response();
    };

    let uid = resolve_player_uid(&state, session.uid);
    let avatar_path = resolve_item_path(&state.state_dir, uid, "avatar", avatar_id);


    let Some(avatar_zon) = read_zon_verbose(&avatar_path) else {
        return (StatusCode::NOT_FOUND, Html("Avatar not found")).into_response();
    };

    let level = zon_get_number(&avatar_zon, "level").unwrap_or(1) as u32;
    let passive_skill_level = zon_get_number(&avatar_zon, "passive_skill_level").unwrap_or(0) as u32;
    let unlocked_talent_num = zon_get_number(&avatar_zon, "unlocked_talent_num").unwrap_or(0) as u32;
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
        input, select {{ width: 100%; padding: 8px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
    label {{ display: block; margin: 12px 0 6px; font-size: 12px; color: #9aa4b2; }}
    button {{ margin-top: 16px; padding: 10px 14px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }}
    .row {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 12px; }}
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
        avatar_name = avatar_name,
        avatar_img = avatar_img,
        level = level,
        unlocked_talent_num = unlocked_talent_num,
        weapon_select = render_weapon_select(cur_weapon_uid, &weapon_options),
        equip_selects = render_equip_selects(&equip_options, &dressed_equip),
        skills = render_skill_inputs(&skill_levels, passive_skill_level),
    );

    Html(body).into_response()
}

async fn avatar_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(avatar_id): Path<u32>,
    Form(payload): Form<AvatarUpdateForm>,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return (StatusCode::UNAUTHORIZED, Html("Please log in")).into_response();
    };

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
    session
        .pending_writes
        .insert(avatar_path, serialized);
    set_session(session_id, session);

    Redirect::to("/dashboard?tab=avatars").into_response()
}

async fn weapon_edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(weapon_uid): Path<u32>,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return (StatusCode::UNAUTHORIZED, Html("Please log in")).into_response();
    };

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
        input {{ width: 100%; padding: 8px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
    label {{ display: block; margin: 12px 0 6px; font-size: 12px; color: #9aa4b2; }}
    button {{ margin-top: 16px; padding: 10px 14px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }}
        .hero {{ display: flex; gap: 16px; align-items: center; margin-bottom: 16px; }}
        .hero img {{ width: 120px; height: 120px; border-radius: 12px; object-fit: cover; border: 1px solid #2a3140; background: #0f1115; }}
        .hero h1 {{ margin: 0; }}
        .meta {{ color: #9aa4b2; font-size: 12px; }}
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
        weapon_name = weapon_name,
        weapon_img = weapon_img,
        level = level,
        refine_level = refine_level,
    );

    Html(body).into_response()
}

async fn weapon_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(weapon_uid): Path<u32>,
    Form(payload): Form<WeaponUpdateForm>,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return (StatusCode::UNAUTHORIZED, Html("Please log in")).into_response();
    };

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

async fn weapon_new(
        State(state): State<AppState>,
        headers: HeaderMap,
) -> impl IntoResponse {
        let Some((_session_id, _session)) = get_session(&headers) else {
                return (StatusCode::UNAUTHORIZED, Html("Please log in")).into_response();
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
        input, select {{ width: 100%; padding: 8px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
        label {{ display: block; margin: 12px 0 6px; font-size: 12px; color: #9aa4b2; }}
        button {{ margin-top: 16px; padding: 10px 14px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }}
        .row {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 12px; }}
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

async fn weapon_add(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<AddWeaponForm>,
) -> impl IntoResponse {
    let Some((session_id, session)) = get_session_mut(&headers) else {
        return (StatusCode::UNAUTHORIZED, Html("Please log in")).into_response();
    };

    let uid = resolve_player_uid(&state, session.uid);
    let weapon_dir = state.state_dir.join(format!("player/{uid}/weapon"));
    let next_uid = read_next_uid(&weapon_dir).unwrap_or(1);
    let new_uid = next_uid.max(1);

    let weapon = ZValue::Object(vec![
        ("id".to_string(), ZValue::Number(payload.weapon_id as i64)),
        ("level".to_string(), ZValue::Number(60)),
        ("exp".to_string(), ZValue::Number(0)),
        ("star".to_string(), ZValue::Number(1)),
        ("refine_level".to_string(), ZValue::Number(payload.refine_level as i64)),
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

async fn bangboo_edit(
        State(state): State<AppState>,
        headers: HeaderMap,
        Path(bangboo_uid): Path<u32>,
) -> impl IntoResponse {
        let Some((_session_id, session)) = get_session(&headers) else {
                return (StatusCode::UNAUTHORIZED, Html("Please log in")).into_response();
        };

        let uid = resolve_player_uid(&state, session.uid);
        let bangboo_path = resolve_item_path(&state.state_dir, uid, "buddy", bangboo_uid);

        let Some(bangboo_zon) = read_zon(&bangboo_path) else {
                return (StatusCode::NOT_FOUND, Html("Bangboo not found")).into_response();
        };

        let level = zon_get_number(&bangboo_zon, "level").unwrap_or(1) as u32;
        let rank = zon_get_number(&bangboo_zon, "rank").unwrap_or(1) as u32;
        let skill_levels = zon_get_skill_levels(&bangboo_zon);

        let hakushin = load_hakushin_data(&state);
        let bangboo_name = hakushin
                .bangboos
                .get(&bangboo_uid)
                .map(|entry| entry.name.clone())
                .unwrap_or_else(|| format!("Bangboo {bangboo_uid}"));
        let bangboo_img = hakushin
                .bangboos
                .get(&bangboo_uid)
                .and_then(|entry| entry.image_local.as_deref())
                .map(to_asset_url)
                .unwrap_or_else(|| svg_data_uri(&bangboo_name));

        let body = format!(
                r#"<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Edit Bangboo</title>
    <style>
        body {{ font-family: system-ui, sans-serif; margin: 0; background: #0f1115; color: #e6e6e6; }}
        .container {{ padding: 24px; max-width: 900px; margin: 0 auto; }}
        input {{ width: 100%; padding: 8px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
        label {{ display: block; margin: 12px 0 6px; font-size: 12px; color: #9aa4b2; }}
        button {{ margin-top: 16px; padding: 10px 14px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }}
        .row {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 12px; }}
        .hero {{ display: flex; gap: 16px; align-items: center; margin-bottom: 16px; }}
        .hero img {{ width: 120px; height: 120px; border-radius: 12px; object-fit: cover; object-position: top; border: 1px solid #2a3140; background: #0f1115; }}
        .hero h1 {{ margin: 0; }}
        .meta {{ color: #9aa4b2; font-size: 12px; }}
    </style>
</head>
<body>
    <div class="container">
        <div class="hero">
            <img src="{bangboo_img}" alt="{bangboo_name}" />
            <div>
                <h1>Edit Bangboo {bangboo_name}</h1>
                <div class="meta">UID {bangboo_uid}</div>
            </div>
        </div>
        <form method="post">
            <div class="row">
                <div>
                    <label>Level</label>
                    <input name="level" type="number" min="1" value="{level}" />
                </div>
                <div>
                    <label>Rank</label>
                    <input name="rank" type="number" min="0" value="{rank}" />
                </div>
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
                bangboo_uid = bangboo_uid,
                bangboo_name = bangboo_name,
                bangboo_img = bangboo_img,
                level = level,
                rank = rank,
                skills = render_bangboo_skill_inputs(&skill_levels),
        );

        Html(body).into_response()
}

async fn bangboo_update(
        State(state): State<AppState>,
        headers: HeaderMap,
        Path(bangboo_uid): Path<u32>,
        Form(payload): Form<BangbooUpdateForm>,
) -> impl IntoResponse {
        let Some((session_id, mut session)) = get_session_mut(&headers) else {
                return (StatusCode::UNAUTHORIZED, Html("Please log in")).into_response();
        };

        let uid = resolve_player_uid(&state, session.uid);
        let bangboo_path = resolve_item_path(&state.state_dir, uid, "buddy", bangboo_uid);

        let Some(mut bangboo_zon) = read_zon(&bangboo_path) else {
                return (StatusCode::NOT_FOUND, Html("Bangboo not found")).into_response();
        };

        zon_set_number(&mut bangboo_zon, "level", payload.level as i64);
        zon_set_number(&mut bangboo_zon, "rank", payload.rank as i64);

        let mut skill_levels = vec![
                ("manual", payload.skill_manual),
                ("passive", payload.skill_passive),
                ("qte", payload.skill_qte),
                ("aid", payload.skill_aid),
        ];
        zon_set_skill_levels(&mut bangboo_zon, &mut skill_levels);

        let serialized = zon_serialize(&bangboo_zon);
        session
                .pending_writes
                .insert(bangboo_path, serialized);
        set_session(session_id, session);

        Redirect::to("/dashboard?tab=bangboos").into_response()
}

async fn da_shiyu_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<DaShiyuForm>,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return (StatusCode::UNAUTHORIZED, Html("Please log in")).into_response();
    };

    let uid = resolve_player_uid(&state, session.uid);
    let hadal_path = state
        .state_dir
        .join(format!("player/{uid}/hadal_zone/info"));

    let Some(mut hadal_zon) = read_zon_verbose(&hadal_path) else {
        return (StatusCode::NOT_FOUND, Html("Hadal zone info not found")).into_response();
    };

    zon_set_entrance_zone_id(&mut hadal_zon, 1, payload.shiyu_zone_id);
    zon_set_entrance_zone_id(&mut hadal_zon, 9, payload.deadly_assault_zone_id);

    let serialized = zon_serialize(&hadal_zon);
    session
        .pending_writes
        .insert(hadal_path, serialized);
    set_session(session_id, session);

    Redirect::to("/dashboard?tab=da-shiyu").into_response()
}

async fn equip_edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(equip_uid): Path<u32>,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return (StatusCode::UNAUTHORIZED, Html("Please log in")).into_response();
    };

    let uid = resolve_player_uid(&state, session.uid);
    let equip_path = state
        .state_dir
        .join(format!("player/{uid}/equip/{equip_uid}"));

    let Some(equip_zon) = read_zon(&equip_path) else {
        return (StatusCode::NOT_FOUND, Html("Disc not found")).into_response();
    };

    let level = zon_get_number(&equip_zon, "level").unwrap_or(0) as u32;
    let equip_item_id = zon_get_number(&equip_zon, "id").unwrap_or(0) as u32;
    let hakushin = load_hakushin_data(&state);
    let equip_index = load_equip_template_index(&state.asset_dir);
    let set_id = equip_set_id(equip_item_id, equip_index);
    let slot = equip_slot(equip_item_id, equip_index);
    let equip_name = hakushin
        .discs
        .get(&set_id)
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| format!("Disc {equip_item_id}"));
    let equip_img = hakushin
        .discs
        .get(&set_id)
        .and_then(|entry| entry.image_local.as_deref())
        .map(to_asset_url)
        .unwrap_or_else(|| svg_data_uri(&equip_name));
    let (main_key, _, _) = zon_get_main_property(&equip_zon);
    let sub_props = zon_get_sub_properties_list(&equip_zon);
    let main_options = disk_main_stat_options(slot);
    let normalized_main_key = normalize_disk_main_stat(slot, main_key).unwrap_or_else(|| {
        main_options.first().copied().unwrap_or(0)
    });
    let sub_options = disk_sub_stat_options(normalized_main_key);
    let warning = "";

    let body = format!(
        r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Edit Disc</title>
  <style>
    body {{ font-family: system-ui, sans-serif; margin: 0; background: #0f1115; color: #e6e6e6; }}
    .container {{ padding: 24px; max-width: 900px; margin: 0 auto; }}
    input {{ width: 100%; padding: 8px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
    label {{ display: block; margin: 12px 0 6px; font-size: 12px; color: #9aa4b2; }}
    button {{ margin-top: 16px; padding: 10px 14px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }}
    .row {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 12px; }}
        .hero {{ display: flex; gap: 16px; align-items: center; margin-bottom: 16px; }}
        .hero img {{ width: 120px; height: 120px; border-radius: 12px; object-fit: cover; border: 1px solid #2a3140; background: #0f1115; }}
        .hero h1 {{ margin: 0; }}
        .meta {{ color: #9aa4b2; font-size: 12px; }}
  </style>
</head>
<body>
  <div class="container">
        <div class="hero">
            <img src="{equip_img}" alt="{equip_name}" />
            <div>
                <h1>Edit Disc {equip_name}</h1>
                <div class="meta">UID {equip_uid} · Item {equip_item_id} · Slot {slot}</div>
            </div>
        </div>
    <form method="post">
      <label>Level</label>
      <input name="level" type="number" min="0" value="{level}" />

            <h3>Main stat</h3>
            <div class="row">
                <div>
                    <label>Stat</label>
                    <select name="main_key">
                        {main_options}
                    </select>
                </div>
            </div>

            <h3>Secondary stats</h3>
            <div class="row">
                {sub_stat_rows}
            </div>
      {warning}

      <button type="submit">Save (pending)</button>
    </form>
  </div>
</body>
</html>"#,
        equip_uid = equip_uid,
        equip_item_id = equip_item_id,
        equip_name = equip_name,
        equip_img = equip_img,
        slot = slot,
        level = level,
        main_options = render_stat_select_options(&state, &main_options, normalized_main_key),
        sub_stat_rows = render_sub_stat_rows(&state, &sub_props, &sub_options, normalized_main_key),
        warning = warning,
    );

    Html(body).into_response()
}

async fn equip_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(equip_uid): Path<u32>,
    Form(payload): Form<EquipUpdateForm>,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return (StatusCode::UNAUTHORIZED, Html("Please log in")).into_response();
    };

    let uid = resolve_player_uid(&state, session.uid);
    let equip_path = state
        .state_dir
        .join(format!("player/{uid}/equip/{equip_uid}"));

    let Some(mut equip_zon) = read_zon(&equip_path) else {
        return (StatusCode::NOT_FOUND, Html("Disc not found")).into_response();
    };

    if zon_get_number(&equip_zon, "star").is_none() {
        zon_set_number(&mut equip_zon, "star", 1);
    }
    zon_set_number(&mut equip_zon, "level", payload.level as i64);
    let equip_item_id = zon_get_number(&equip_zon, "id").unwrap_or(0) as u32;
    let equip_index = load_equip_template_index(&state.asset_dir);
    let slot = equip_slot(equip_item_id, equip_index);
    let main_key = normalize_disk_main_stat(slot, payload.main_key)
        .unwrap_or_else(|| disk_main_stat_options(slot).first().copied().unwrap_or(0));
    let main_base = disk_main_base_value(main_key).unwrap_or(0);
    zon_set_main_property(&mut equip_zon, main_key, main_base, 0);

    let sub_keys = vec![
        payload.sub_key_1,
        payload.sub_key_2,
        payload.sub_key_3,
        payload.sub_key_4,
    ];
    let sub_procs = vec![
        payload.sub_proc_1,
        payload.sub_proc_2,
        payload.sub_proc_3,
        payload.sub_proc_4,
    ];
    let allowed_subs = disk_sub_stat_options(main_key);
    let mut keys = Vec::new();
    let mut base = Vec::new();
    let mut add = Vec::new();
    for idx in 0..sub_keys.len() {
        let key = sub_keys[idx];
        if key == 0 || !allowed_subs.contains(&key) || keys.contains(&key) {
            continue;
        }
        let Some(stat_base) = disk_sub_base_value(key) else {
            continue;
        };
        let mut procs = *sub_procs.get(idx).unwrap_or(&0);
        if procs == 0 {
            procs = 1;
        }
        if procs > 6 {
            procs = 6;
        }
        keys.push(key);
        base.push(stat_base);
        add.push(procs);
    }

    let mut total_procs: u32 = add.iter().sum();
    if total_procs > 9 {
        for proc in add.iter_mut().rev() {
            if total_procs <= 9 {
                break;
            }
            let excess = total_procs - 9;
            let reducible = proc.saturating_sub(1);
            let reduce = excess.min(reducible);
            *proc -= reduce;
            total_procs -= reduce;
        }
    }

    zon_set_sub_properties(&mut equip_zon, &keys, &base, &add);

    let serialized = zon_serialize(&equip_zon);
    session.pending_writes.insert(equip_path, serialized);
    set_session(session_id, session);

    Redirect::to("/dashboard?tab=discs").into_response()
}

async fn equip_new(
        State(state): State<AppState>,
        headers: HeaderMap,
) -> impl IntoResponse {
        let Some((_session_id, _session)) = get_session(&headers) else {
                return (StatusCode::UNAUTHORIZED, Html("Please log in")).into_response();
        };

        let options = render_disc_select_options(&state, 0);
        let slot_options = render_slot_options(1);
        let main_options = render_stat_select_options(&state, &disk_main_stat_options(1), 0);
        let sub_options = disk_sub_stat_options(0);
        let sub_stat_rows = render_sub_stat_rows(&state, &[], &sub_options, 0);

        let mut main_options_by_slot = HashMap::new();
        let mut sub_options_by_main = HashMap::new();
        let mut label_map = HashMap::new();
        for slot in 1..=6 {
            let options = disk_main_stat_options(slot);
            for key in &options {
                label_map.entry(*key).or_insert_with(|| stat_label(&state, *key));
                let sub_opts = disk_sub_stat_options(*key);
                for sub_key in &sub_opts {
                    label_map.entry(*sub_key).or_insert_with(|| stat_label(&state, *sub_key));
                }
                sub_options_by_main.insert(*key, sub_opts);
            }
            main_options_by_slot.insert(slot, options);
        }
        let main_options_by_slot_json = serde_json::to_string(&main_options_by_slot).unwrap_or_default();
        let sub_options_by_main_json = serde_json::to_string(&sub_options_by_main).unwrap_or_default();
        let label_map_json = serde_json::to_string(&label_map).unwrap_or_default();
        let script = render_new_equip_script(
            &main_options_by_slot_json,
            &sub_options_by_main_json,
            &label_map_json,
        );

        let body = format!(
                r#"<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>New Disc</title>
    <style>
        body {{ font-family: system-ui, sans-serif; margin: 0; background: #0f1115; color: #e6e6e6; }}
        .container {{ padding: 24px; max-width: 900px; margin: 0 auto; }}
        input, select {{ width: 100%; padding: 8px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
        label {{ display: block; margin: 12px 0 6px; font-size: 12px; color: #9aa4b2; }}
        button {{ margin-top: 16px; padding: 10px 14px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }}
        .row {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 12px; }}
    </style>
</head>
<body>
    <div class="container">
        <h1>New Disc</h1>
        <form method="post">
            <div>
                <label>Disc set</label>
                <select name="equip_set_id" required>
                    {options}
                </select>
            </div>
            <div class="row">
                <div>
                    <label>Slot</label>
                    <select name="equip_slot" id="equip_slot">
                        {slot_options}
                    </select>
                </div>
            </div>

            <h3>Main stat</h3>
            <div class="row">
                <div>
                    <label>Stat</label>
                    <select name="main_key" id="main_key">
                        {main_options}
                    </select>
                </div>
            </div>

            <h3>Secondary stats</h3>
            <div class="row">
                {sub_stat_rows}
            </div>
            {script}
            <button type="submit">Create</button>
        </form>
    </div>
</body>
</html>"#,
                options = options,
                slot_options = slot_options,
                main_options = main_options,
                sub_stat_rows = sub_stat_rows,
                script = script,
        );

        Html(body).into_response()
}

async fn equip_add(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<AddEquipForm>,
) -> impl IntoResponse {
    let Some((session_id, session)) = get_session_mut(&headers) else {
        return (StatusCode::UNAUTHORIZED, Html("Please log in")).into_response();
    };

    let uid = resolve_player_uid(&state, session.uid);
    let equip_dir = state.state_dir.join(format!("player/{uid}/equip"));
    let next_uid = read_next_uid(&equip_dir).unwrap_or(1);
    let new_uid = next_uid.max(1);
    let equip_index = load_equip_template_index(&state.asset_dir);
    let Some(item_id) = resolve_equip_item_id(payload.equip_set_id, payload.equip_slot, equip_index) else {
        return (
            StatusCode::BAD_REQUEST,
            Html("Invalid disc set/slot combination"),
        )
            .into_response();
    };
    let item_id = force_disc_fourth_digit(item_id);

    let main_key = normalize_disk_main_stat(payload.equip_slot, payload.main_key)
        .unwrap_or_else(|| disk_main_stat_options(payload.equip_slot).first().copied().unwrap_or(0));
    let main_base = disk_main_base_value(main_key).unwrap_or(0);
    let main_properties = if main_key == 0 {
        Vec::new()
    } else {
        vec![ZValue::Object(vec![
            ("add_value".to_string(), ZValue::Number(0)),
            ("base_value".to_string(), ZValue::Number(main_base as i64)),
            ("key".to_string(), ZValue::Number(main_key as i64)),
        ])]
    };

    let sub_keys = vec![
        payload.sub_key_1,
        payload.sub_key_2,
        payload.sub_key_3,
        payload.sub_key_4,
    ];
    let sub_procs = vec![
        payload.sub_proc_1,
        payload.sub_proc_2,
        payload.sub_proc_3,
        payload.sub_proc_4,
    ];
    let allowed_subs = disk_sub_stat_options(main_key);
    let mut keys = Vec::new();
    let mut base = Vec::new();
    let mut add = Vec::new();
    for idx in 0..sub_keys.len() {
        let key = sub_keys[idx];
        if key == 0 || !allowed_subs.contains(&key) || keys.contains(&key) {
            continue;
        }
        let Some(stat_base) = disk_sub_base_value(key) else {
            continue;
        };
        let mut procs = *sub_procs.get(idx).unwrap_or(&0);
        if procs == 0 {
            procs = 1;
        }
        if procs > 6 {
            procs = 6;
        }
        keys.push(key);
        base.push(stat_base);
        add.push(procs);
    }

    let mut total_procs: u32 = add.iter().sum();
    if total_procs > 9 {
        for proc in add.iter_mut().rev() {
            if total_procs <= 9 {
                break;
            }
            let excess = total_procs - 9;
            let reducible = proc.saturating_sub(1);
            let reduce = excess.min(reducible);
            *proc -= reduce;
            total_procs -= reduce;
        }
    }

    let equip = ZValue::Object(vec![
        ("id".to_string(), ZValue::Number(item_id as i64)),
        ("level".to_string(), ZValue::Number(15)),
        ("exp".to_string(), ZValue::Number(0)),
        ("lock".to_string(), ZValue::Bool(false)),
        ("star".to_string(), ZValue::Number(1)),
        ("properties".to_string(), ZValue::Array(main_properties)),
        ("sub_properties".to_string(), ZValue::Array(Vec::new())),
    ]);

    let mut equip = equip;
    zon_set_sub_properties(&mut equip, &keys, &base, &add);

    let equip_path = equip_dir.join(new_uid.to_string());
    let serialized = format_zon_pretty(&zon_serialize(&equip));
    if let Some(parent) = equip_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(err) = fs::write(&equip_path, serialized) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("Failed to create disc: {}", err)),
        )
            .into_response();
    }

    let next_path = equip_dir.join("next");
    if let Err(err) = fs::write(&next_path, format!("{}\n", new_uid + 1)) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("Failed to update disc counter: {}", err)),
        )
            .into_response();
    }

    set_session(session_id, session);
    Redirect::to(&format!("/equip/{new_uid}")).into_response()
}

async fn apply_changes(headers: HeaderMap) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return (StatusCode::UNAUTHORIZED, Html("Please log in")).into_response();
    };

    for (path, content) in session.pending_writes.drain() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let formatted = format_zon_pretty(&content);
        if let Err(err) = fs::write(&path, formatted) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html(format!("Failed to write {}: {}", path.display(), err)),
            )
                .into_response();
        }
    }

    set_session(session_id, session);
    Redirect::to("/dashboard").into_response()
}

fn render_avatar_cards(state: &AppState, uid: u32) -> String {
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
                    name = name,
                    level = level,
                    img = img
                ));
            }
        }
    }

    if cards.is_empty() {
        cards.push_str("<p class=\"meta\">No characters found for this account.</p>");
    }

    format!("<div class=\"cards\">{cards}</div>")
}

fn render_weapon_cards(state: &AppState, uid: u32) -> String {
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
                name = name,
                level = level,
                img = img
            ));
        }
    }

    if cards.is_empty() {
        cards.push_str("<p class=\"meta\">No weapons found for this account.</p>");
    }

    let add_panel = render_add_weapon_panel(state);
    format!("{add_panel}<div class=\"cards\">{cards}</div>")
}

fn render_equip_cards(state: &AppState, uid: u32) -> String {
    let equip_dir = state.state_dir.join(format!("player/{uid}/equip"));
    let equip_templates = load_equip_templates(&state.asset_dir);
    let hakushin = load_hakushin_data(state);
    let equip_index = load_equip_template_index(&state.asset_dir);

    let mut cards = String::new();
    if let Ok(entries) = fs::read_dir(&equip_dir) {
        for entry in entries.flatten() {
            let Some(file_name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            let equip_uid = match file_name
                .strip_suffix(".zon")
                .unwrap_or(&file_name)
                .parse::<u32>()
            {
                Ok(value) if value > 0 => value,
                _ => continue,
            };
            let equip = read_zon(&entry.path());
            let equip_item_id = equip
                .as_ref()
                .and_then(|v| zon_get_number(v, "id"))
                .unwrap_or(0) as u32;
            let set_id = equip_set_id(equip_item_id, equip_index);
            let level = equip
                .as_ref()
                .and_then(|v| zon_get_number(v, "level"))
                .unwrap_or(0);

            let name = hakushin
                .discs
                .get(&set_id)
                .map(|entry| entry.name.clone())
                .or_else(|| equip_templates.get(&equip_item_id).cloned())
                .unwrap_or_else(|| format!("Disc {equip_item_id}"));

            let main_stat = equip
                .as_ref()
                .map(zon_get_main_property)
                .unwrap_or((0, 0, 0));
            let slot = equip_slot(equip_item_id, equip_index);

            let img = hakushin
                .discs
                .get(&set_id)
                .and_then(|entry| entry.image_local.as_deref())
                .map(to_asset_url)
                .unwrap_or_else(|| svg_data_uri(&name));
            let main_label = stat_label(state, main_stat.0);
            cards.push_str(&format!(
                "<a class=\"card\" href=\"/equip/{uid}\"><img class=\"thumb\" src=\"{img}\" alt=\"{name}\" /><span class=\"pill\">UID {uid}</span><h3>{name}</h3><div class=\"meta\">Slot {slot} · Level {level}</div><div class=\"meta\">Main: {main_label} ({main_base}+{main_add})</div></a>",
                uid = equip_uid,
                name = name,
                level = level,
                slot = slot,
                main_label = main_label,
                main_base = main_stat.1,
                main_add = main_stat.2,
                img = img
            ));
        }
    }

    if cards.is_empty() {
        cards.push_str("<p class=\"meta\">No discs found for this account.</p>");
    }

    let add_panel = render_add_equip_panel(state);
    format!("{add_panel}<div class=\"cards\">{cards}</div>")
}

fn render_bangboo_cards(state: &AppState, uid: u32) -> String {
    let bangboo_dir = state.state_dir.join(format!("player/{uid}/buddy"));
    let hakushin = load_hakushin_data(state);

    let mut cards = String::new();
    if let Ok(entries) = fs::read_dir(&bangboo_dir) {
        for entry in entries.flatten() {
            let Some(file_name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            let bangboo_uid = match file_name
                .strip_suffix(".zon")
                .unwrap_or(&file_name)
                .parse::<u32>()
            {
                Ok(value) if value > 0 => value,
                _ => continue,
            };

            let bangboo = read_zon(&entry.path());
            let level = bangboo
                .as_ref()
                .and_then(|v| zon_get_number(v, "level"))
                .unwrap_or(0);
            let rank = bangboo
                .as_ref()
                .and_then(|v| zon_get_number(v, "rank"))
                .unwrap_or(0);

            let name = hakushin
                .bangboos
                .get(&bangboo_uid)
                .map(|entry| entry.name.clone())
                .unwrap_or_else(|| format!("Bangboo {bangboo_uid}"));
            let img = hakushin
                .bangboos
                .get(&bangboo_uid)
                .and_then(|entry| entry.image_local.as_deref())
                .map(to_asset_url)
                .unwrap_or_else(|| svg_data_uri(&name));

            cards.push_str(&format!(
                "<a class=\"card\" href=\"/bangboo/{uid}\"><img class=\"thumb\" src=\"{img}\" alt=\"{name}\" /><span class=\"pill\">UID {uid}</span><h3>{name}</h3><div class=\"meta\">Level {level}</div><div class=\"meta\">Rank {rank}</div></a>",
                uid = bangboo_uid,
                name = name,
                level = level,
                rank = rank,
                img = img
            ));
        }
    }

    if cards.is_empty() {
        cards.push_str("<p class=\"meta\">No bangboos found for this account.</p>");
    }

    format!("<div class=\"cards\">{cards}</div>")
}

fn render_da_shiyu_panel(state: &AppState, uid: u32) -> String {
        let hadal_path = state
                .state_dir
                .join(format!("player/{uid}/hadal_zone/info"));
        let hadal_zon = read_zon(&hadal_path);
        let shiyu_zone_id = hadal_zon
                .as_ref()
                .and_then(|v| zon_get_entrance_zone_id(v, 1))
                .unwrap_or(0);
        let deadly_assault_zone_id = hadal_zon
            .as_ref()
            .and_then(|v| zon_get_entrance_zone_id(v, 9))
            .unwrap_or(0);

        format!(
                r#"<div class="panel">
    <div>
        <h3>DA and Shiyu</h3>
        <div class="meta">Edit Hadal Zone entrances for player {uid}</div>
    </div>
</div>
<div class="panel">
    <form method="post" action="/da-shiyu">
        <div class="row">
            <div>
                <label>Shiyu (id=1 zone_id)</label>
                <input name="shiyu_zone_id" type="number" min="0" value="{shiyu_zone_id}" />
            </div>
            <div>
                <label>Deadly Assault (id=9 zone_id)</label>
                <input name="deadly_assault_zone_id" type="number" min="0" value="{deadly_assault_zone_id}" />
            </div>
        </div>
        <button type="submit" style="margin-top:12px;">Save (pending)</button>
    </form>
</div>"#,
                uid = uid,
                shiyu_zone_id = shiyu_zone_id,
                deadly_assault_zone_id = deadly_assault_zone_id,
        )
}

fn render_add_weapon_panel(state: &AppState) -> String {
        let _ = state;
        "<div class=\"panel\"><h3>Add Weapon</h3><a href=\"/weapon/new\">New weapon</a></div>"
                .to_string()
}

fn render_add_equip_panel(state: &AppState) -> String {
        let _ = state;
        "<div class=\"panel\"><h3>Add Disc</h3><a href=\"/equip/new\">New disc</a></div>"
                .to_string()
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

fn render_disc_select_options(state: &AppState, selected_id: u32) -> String {
    let hakushin = load_hakushin_data(state);
    let mut items: Vec<(u32, String)> = hakushin
        .discs
        .iter()
        .map(|(id, entry)| (*id, entry.name.clone()))
        .collect();
    items.sort_by(|a, b| a.1.cmp(&b.1));

    let mut html = String::new();
    html.push_str("<option value=\"\" disabled selected>Select disc</option>");
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

fn render_bangboo_skill_inputs(skill_levels: &HashMap<String, u32>) -> String {
    let mut html = String::new();
    for (key, label) in [
        ("manual", "Manual"),
        ("passive", "Passive"),
        ("qte", "QTE"),
        ("aid", "Aid"),
    ] {
        let value = skill_levels.get(key).copied().unwrap_or(1);
        html.push_str(&format!(
            "<div><label>{label}</label><input name=\"skill_{key}\" type=\"number\" min=\"1\" value=\"{value}\" /></div>",
        ));
    }

    html
}

async fn asset_handler(
    State(state): State<AppState>,
    Path(path): Path<String>,
) -> impl IntoResponse {
    let rel_path = FsPath::new(&path);
    if rel_path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::RootDir))
    {
        return StatusCode::BAD_REQUEST.into_response();
    }

    let full_path = state.root_dir.join(rel_path);
    let Ok(bytes) = fs::read(&full_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let content_type = content_type_for_path(&full_path);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(bytes))
        .unwrap()
}

fn content_type_for_path(path: &FsPath) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}

fn to_asset_url(path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }
    format!("/assets/{}", path.trim_start_matches('/'))
}

fn load_hakushin_data(state: &AppState) -> &'static HakushinData {
    HAKUSHIN_DATA.get_or_init(|| HakushinData {
        avatars: load_hakushin_list(
            &state.root_dir,
            &state.dump_dir.join("characters.json"),
            "name",
            &[
                "image_local",
                "icon_local",
                "cropped_icon_local",
                "image",
                "icon",
                "cropped_icon",
            ],
        ),
        weapons: load_hakushin_list(
            &state.root_dir,
            &state.dump_dir.join("weapons.json"),
            "name",
            &["icon_local", "icon"],
        ),
        discs: load_hakushin_list(
            &state.root_dir,
            &state.dump_dir.join("drive_discs.json"),
            "name",
            &["icon_local", "icon"],
        ),
        bangboos: load_hakushin_list(
            &state.root_dir,
            &state.dump_dir.join("bangboos.json"),
            "name",
            &["icon_local", "icon"],
        ),
    })
}

fn load_hakushin_list(
    root_dir: &FsPath,
    path: &FsPath,
    name_key: &str,
    image_keys: &[&str],
) -> HashMap<u32, HakushinEntry> {
    let mut result = HashMap::new();
    let Ok(data) = fs::read_to_string(path) else {
        return result;
    };
    let Ok(json) = serde_json::from_str::<JsonValue>(&data) else {
        return result;
    };
    let Some(items) = json.as_array() else {
        return result;
    };

    for item in items {
        let Some(id) = item.get("id").and_then(|v| v.as_u64()) else {
            continue;
        };
        let name = item
            .get(name_key)
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();
        let mut image_local = None;
        for key in image_keys {
            if let Some(value) = item.get(*key).and_then(|v| v.as_str()) {
                if value.starts_with("http://") || value.starts_with("https://") {
                    image_local = Some(value.to_string());
                    break;
                }
                if root_dir.join(value).exists() {
                    image_local = Some(value.to_string());
                    break;
                }
            }
        }

        result.insert(
            id as u32,
            HakushinEntry {
                name,
                image_local,
            },
        );
    }

    result
}
fn validate_login(db_path: &FsPath, username: &str, password: &str) -> Result<Option<Session>, String> {
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    let mut stmt = conn
        .prepare("SELECT uid, username, password FROM t_sdk_account WHERE username = ?1")
        .map_err(|e| e.to_string())?;

    let row = stmt.query_row(params![username], |row| {
        let uid: i32 = row.get(0)?;
        let username: String = row.get(1)?;
        let hash: String = row.get(2)?;
        Ok((uid, username, hash))
    });

    let (uid, username, hash) = match row {
        Ok(v) => v,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };

    let hash = PasswordHash::new(&hash).map_err(|e| e.to_string())?;
    if hash.verify_password(&[&Pbkdf2], password).is_ok() {
        Ok(Some(Session {
            uid,
            username,
            pending_writes: HashMap::new(),
        }))
    } else {
        Ok(None)
    }
}

fn resolve_sdk_config_path() -> PathBuf {
    if let Ok(path) = env::var("HOYO_SDK_CONFIG") {
        return PathBuf::from(path);
    }

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let cfg = root.join("hoyo-sdk/sdk_server.toml");
    if cfg.exists() {
        return cfg;
    }

    root.join("hoyo-sdk/sdk.default.toml")
}

fn load_sdk_config(path: &FsPath) -> SdkConfig {
    let data = fs::read_to_string(path).expect("Failed to read SDK config");
    toml::from_str(&data).expect("Invalid SDK config")
}

fn resolve_db_path(config_path: &FsPath, db_file: &str) -> PathBuf {
    let db_path = PathBuf::from(db_file);
    if db_path.is_absolute() {
        return db_path;
    }

    config_path
        .parent()
        .unwrap_or_else(|| FsPath::new("."))
        .join(db_path)
}

fn new_session_id() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(48)
        .map(char::from)
        .collect()
}

fn get_session(headers: &HeaderMap) -> Option<(String, Session)> {
    let session_id = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| cookies.split(';').find_map(|c| c.trim().strip_prefix("ge_session=")))?
        .to_string();

    let store = SESSION_STORE.get_or_init(|| Mutex::new(HashMap::new()));
    store
        .lock()
        .unwrap()
        .get(&session_id)
        .cloned()
        .map(|s| (session_id, s))
}

fn get_session_mut(headers: &HeaderMap) -> Option<(String, Session)> {
    get_session(headers)
}

fn set_session(session_id: String, session: Session) {
    let store = SESSION_STORE.get_or_init(|| Mutex::new(HashMap::new()));
    store.lock().unwrap().insert(session_id, session);
}

fn read_next_uid(dir: &FsPath) -> Option<u32> {
    let next_path = dir.join("next");
    if let Ok(value) = fs::read_to_string(&next_path) {
        if let Ok(parsed) = value.trim().parse::<u32>() {
            return Some(parsed);
        }
    }

    let mut max_id = 0u32;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let Some(name) = file_name.to_str() else {
                continue;
            };
            let Ok(id) = name.trim().parse::<u32>() else {
                continue;
            };
            if id > max_id {
                max_id = id;
            }
        }
    }

    Some(max_id.saturating_add(1).max(1))
}

fn load_equip_template_index(asset_dir: &FsPath) -> &'static EquipTemplateIndex {
    EQUIP_TEMPLATE.get_or_init(|| {
        let mut index = EquipTemplateIndex::default();
        let path = asset_dir.join("EquipmentTemplateTb.json");
        let Ok(data) = fs::read_to_string(path) else {
            return index;
        };
        let Ok(json) = serde_json::from_str::<JsonValue>(&data) else {
            return index;
        };
        let Some(items) = json.get("data").and_then(|v| v.as_array()) else {
            return index;
        };

        for item in items {
            let Some(item_id) = item.get("item_id").and_then(|v| v.as_u64()) else {
                continue;
            };
            let Some(slot) = item.get("equipment_type").and_then(|v| v.as_u64()) else {
                continue;
            };
            let Some(suit_type) = item.get("suit_type").and_then(|v| v.as_u64()) else {
                continue;
            };
            let info = EquipTemplateInfo {
                suit_type: suit_type as u32,
                slot: slot as u32,
            };
            index.by_item.insert(item_id as u32, info);
            index
                .by_suit_slot
                .entry((info.suit_type, info.slot))
                .or_insert(item_id as u32);
        }

        index
    })
}

fn equip_set_id(item_id: u32, index: &EquipTemplateIndex) -> u32 {
    index
        .by_item
        .get(&item_id)
        .map(|info| info.suit_type)
        .unwrap_or_else(|| (item_id / 100) * 100)
}

fn equip_slot(item_id: u32, index: &EquipTemplateIndex) -> u32 {
    index
        .by_item
        .get(&item_id)
        .map(|info| info.slot)
        .unwrap_or_else(|| item_id % 10)
}

fn force_disc_fourth_digit(item_id: u32) -> u32 {
    let s = item_id.to_string();
    if s.len() < 4 {
        return item_id;
    }
    let mut chars: Vec<char> = s.chars().collect();
    chars[3] = '4';
    chars.iter().collect::<String>().parse::<u32>().unwrap_or(item_id)
}

fn resolve_equip_item_id(
    set_id: u32,
    slot: u32,
    index: &EquipTemplateIndex,
) -> Option<u32> {
    index
        .by_suit_slot
        .get(&(set_id, slot))
        .copied()
}

fn load_stat_names(state: &AppState) -> &'static HashMap<u32, String> {
    STAT_NAMES.get_or_init(|| {
        let mut map = HashMap::new();

        let mut weapon_prop = HashMap::new();
        let weapon_template = state.asset_dir.join("WeaponTemplateTb.json");
        if let Ok(data) = fs::read_to_string(weapon_template) {
            if let Ok(json) = serde_json::from_str::<JsonValue>(&data) {
                if let Some(items) = json.get("data").and_then(|v| v.as_array()) {
                    for item in items {
                        let Some(item_id) = item.get("item_id").and_then(|v| v.as_u64()) else {
                            continue;
                        };
                        let base_prop = item
                            .get("base_property")
                            .and_then(|v| v.get("property"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32;
                        let rand_prop = item
                            .get("rand_property")
                            .and_then(|v| v.get("property"))
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32;
                        weapon_prop.insert(item_id as u32, (base_prop, rand_prop));
                    }
                }
            }
        }

        let weapon_details = state.dump_dir.join("weapon_details.json");
        if let Ok(data) = fs::read_to_string(weapon_details) {
            if let Ok(json) = serde_json::from_str::<JsonValue>(&data) {
                if let Some(obj) = json.as_object() {
                    for (key, details) in obj {
                        let Ok(item_id) = key.parse::<u32>() else {
                            continue;
                        };
                        let Some((base_prop, rand_prop)) = weapon_prop.get(&item_id) else {
                            continue;
                        };
                        if let Some(name) = details
                            .get("base_property")
                            .and_then(|v| v.get("name"))
                            .and_then(|v| v.as_str())
                        {
                            if *base_prop > 0 {
                                map.entry(*base_prop).or_insert_with(|| name.to_string());
                            }
                        }
                        if let Some(name) = details
                            .get("rand_property")
                            .and_then(|v| v.get("name"))
                            .and_then(|v| v.as_str())
                        {
                            if *rand_prop > 0 {
                                map.entry(*rand_prop).or_insert_with(|| name.to_string());
                            }
                        }
                    }
                }
            }
        }

        let bangboo_details = state.dump_dir.join("bangboo_details.json");
        if let Ok(data) = fs::read_to_string(bangboo_details) {
            if let Ok(json) = serde_json::from_str::<JsonValue>(&data) {
                if let Some(obj) = json.as_object() {
                    for (_, details) in obj {
                        if let Some(ascensions) = details.get("ascensions").and_then(|v| v.as_object()) {
                            for (_, stage) in ascensions {
                                if let Some(extra_props) = stage.get("extra_props").and_then(|v| v.as_array()) {
                                    for prop in extra_props {
                                        let Some(id) = prop.get("id").and_then(|v| v.as_u64()) else {
                                            continue;
                                        };
                                        let Some(name) = prop.get("name").and_then(|v| v.as_str()) else {
                                            continue;
                                        };
                                        map.entry(id as u32).or_insert_with(|| name.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        map
    })
}

const STAT_HP: u32 = 11103;
const STAT_ATK: u32 = 12103;
const STAT_DEF: u32 = 13103;
const STAT_HP_PCT: u32 = 11102;
const STAT_ATK_PCT: u32 = 12102;
const STAT_DEF_PCT: u32 = 13102;
const STAT_CRIT_RATE: u32 = 20103;
const STAT_CRIT_DMG: u32 = 21103;
const STAT_ANOMALY_PROF: u32 = 31203;
const STAT_PEN: u32 = 23203;
const STAT_PEN_RATIO: u32 = 23202;
const STAT_ANOMALY_MASTERY: u32 = 31402;
const STAT_IMPACT: u32 = 12202;
const STAT_ENERGY_REGEN: u32 = 30502;
const STAT_PHYSICAL_DMG: u32 = 31503;
const STAT_FIRE_DMG: u32 = 31603;
const STAT_ICE_DMG: u32 = 31703;
const STAT_ELECTRIC_DMG: u32 = 31803;
const STAT_ETHER_DMG: u32 = 31903;

fn disk_stat_label(key: u32) -> Option<&'static str> {
    match key {
        STAT_HP => Some("HP"),
        STAT_ATK => Some("ATK"),
        STAT_DEF => Some("DEF"),
        STAT_HP_PCT => Some("HP%"),
        STAT_ATK_PCT => Some("ATK%"),
        STAT_DEF_PCT => Some("DEF%"),
        STAT_CRIT_RATE => Some("CRIT Rate%"),
        STAT_CRIT_DMG => Some("CRIT DMG%"),
        STAT_ANOMALY_PROF => Some("Anomaly Proficiency"),
        STAT_PEN => Some("PEN"),
        STAT_PEN_RATIO => Some("PEN Ratio%"),
        STAT_ANOMALY_MASTERY => Some("Anomaly Mastery%"),
        STAT_IMPACT => Some("Impact%"),
        STAT_ENERGY_REGEN => Some("Energy Regen%"),
        STAT_PHYSICAL_DMG => Some("Physical DMG%"),
        STAT_FIRE_DMG => Some("Fire DMG%"),
        STAT_ICE_DMG => Some("Ice DMG%"),
        STAT_ELECTRIC_DMG => Some("Electric DMG%"),
        STAT_ETHER_DMG => Some("Ether DMG%"),
        _ => None,
    }
}

fn disk_main_stat_options(slot: u32) -> Vec<u32> {
    match slot {
        1 => vec![STAT_HP],
        2 => vec![STAT_ATK],
        3 => vec![STAT_DEF],
        4 => vec![
            STAT_ATK_PCT,
            STAT_HP_PCT,
            STAT_DEF_PCT,
            STAT_CRIT_RATE,
            STAT_CRIT_DMG,
            STAT_ANOMALY_PROF,
        ],
        5 => vec![
            STAT_ATK_PCT,
            STAT_HP_PCT,
            STAT_DEF_PCT,
            STAT_PHYSICAL_DMG,
            STAT_ICE_DMG,
            STAT_FIRE_DMG,
            STAT_ELECTRIC_DMG,
            STAT_ETHER_DMG,
            STAT_PEN_RATIO,
        ],
        6 => vec![
            STAT_ATK_PCT,
            STAT_HP_PCT,
            STAT_DEF_PCT,
            STAT_ANOMALY_MASTERY,
            STAT_IMPACT,
            STAT_ENERGY_REGEN,
        ],
        _ => vec![],
    }
}

fn normalize_disk_main_stat(slot: u32, key: u32) -> Option<u32> {
    let options = disk_main_stat_options(slot);
    if options.contains(&key) {
        Some(key)
    } else {
        None
    }
}

fn disk_main_base_value(key: u32) -> Option<u32> {
    match key {
        STAT_HP => Some(550),
        STAT_ATK => Some(79),
        STAT_DEF => Some(46),
        STAT_ATK_PCT => Some(750),
        STAT_HP_PCT => Some(750),
        STAT_DEF_PCT => Some(1200),
        STAT_CRIT_RATE => Some(600),
        STAT_CRIT_DMG => Some(1200),
        STAT_ANOMALY_PROF => Some(23),
        STAT_PHYSICAL_DMG
        | STAT_ICE_DMG
        | STAT_FIRE_DMG
        | STAT_ELECTRIC_DMG
        | STAT_ETHER_DMG => Some(750),
        STAT_PEN_RATIO => Some(600),
        STAT_ANOMALY_MASTERY => Some(750),
        STAT_IMPACT => Some(450),
        STAT_ENERGY_REGEN => Some(1500),
        _ => None,
    }
}

fn disk_sub_base_value(key: u32) -> Option<u32> {
    match key {
        STAT_HP => Some(112),
        STAT_ATK => Some(19),
        STAT_DEF => Some(15),
        STAT_HP_PCT => Some(300),
        STAT_ATK_PCT => Some(300),
        STAT_DEF_PCT => Some(480),
        STAT_CRIT_DMG => Some(480),
        STAT_CRIT_RATE => Some(240),
        STAT_ANOMALY_PROF => Some(9),
        STAT_PEN => Some(9),
        _ => None,
    }
}

fn disk_sub_stat_options(main_key: u32) -> Vec<u32> {
    let mut options = vec![
        STAT_HP,
        STAT_ATK,
        STAT_DEF,
        STAT_HP_PCT,
        STAT_ATK_PCT,
        STAT_DEF_PCT,
        STAT_CRIT_DMG,
        STAT_CRIT_RATE,
        STAT_ANOMALY_PROF,
        STAT_PEN,
    ];

    match main_key {
        STAT_HP_PCT => {
            options.retain(|key| *key != STAT_HP && *key != STAT_HP_PCT);
        }
        STAT_ATK_PCT => {
            options.retain(|key| *key != STAT_ATK && *key != STAT_ATK_PCT);
        }
        STAT_DEF_PCT => {
            options.retain(|key| *key != STAT_DEF && *key != STAT_DEF_PCT);
        }
        STAT_CRIT_RATE => {
            options.retain(|key| *key != STAT_CRIT_RATE);
        }
        STAT_CRIT_DMG => {
            options.retain(|key| *key != STAT_CRIT_DMG);
        }
        STAT_ANOMALY_PROF => {
            options.retain(|key| *key != STAT_ANOMALY_PROF);
        }
        _ => {}
    }

    options
}

fn stat_label(state: &AppState, key: u32) -> String {
    if let Some(label) = disk_stat_label(key) {
        return label.to_string();
    }
    let names = load_stat_names(state);
    names
        .get(&key)
        .cloned()
        .unwrap_or_else(|| format!("Stat {key}"))
}

fn render_stat_select_options(state: &AppState, keys: &[u32], selected: u32) -> String {
    let mut html = String::new();
    let mut unique = keys.to_vec();
    unique.retain(|key| *key > 0);
    if selected > 0 && !unique.contains(&selected) {
        unique.push(selected);
    }
    unique.sort_unstable();
    unique.dedup();
    for key in unique {
        let label = stat_label(state, key);
        html.push_str(&format!(
            "<option value=\"{}\"{}>{}</option>",
            key,
            if key == selected { " selected" } else { "" },
            label
        ));
    }
    html
}

fn render_sub_stat_rows(
    state: &AppState,
    sub_props: &[(u32, u32, u32)],
    options: &[u32],
    _main_key: u32,
) -> String {
    let mut rows = String::new();
    for idx in 0..4 {
        let (mut key, _base, add) = sub_props
            .get(idx)
            .copied()
            .unwrap_or((0, 0, 0));
        if key == 0 {
            if let Some(first) = options.first() {
                key = *first;
            }
        }
        rows.push_str(&format!(
            "<div><label>Key</label><select name=\"sub_key_{}\">{}</select></div>",
            idx + 1,
            render_stat_select_options(state, options, key)
        ));
        rows.push_str(&format!(
            "<div><label>Procs</label><input name=\"sub_proc_{}\" type=\"number\" min=\"0\" max=\"6\" value=\"{}\" /></div>",
            idx + 1,
            add
        ));
    }
    rows
}

fn render_new_equip_script(
    main_options_by_slot_json: &str,
    sub_options_by_main_json: &str,
    label_map_json: &str,
) -> String {
    let mut script = String::new();
    script.push_str("<script>\n");
    script.push_str("const mainOptionsBySlot = ");
    script.push_str(main_options_by_slot_json);
    script.push_str(";\n");
    script.push_str("const subOptionsByMain = ");
    script.push_str(sub_options_by_main_json);
    script.push_str(";\n");
    script.push_str("const statLabels = ");
    script.push_str(label_map_json);
    script.push_str(";\n");
    script.push_str("const slotSelect = document.getElementById(\"equip_slot\");\n");
    script.push_str("const mainSelect = document.getElementById(\"main_key\");\n");
    script.push_str("const subSelects = Array.from(document.querySelectorAll(\"select[name^='sub_key_']\"));\n\n");

    script.push_str("const renderOptions = (select, keys, selected) => {\n");
    script.push_str("  select.innerHTML = \"\";\n");
    script.push_str("  for (const key of keys) {\n");
    script.push_str("    const option = document.createElement(\"option\");\n");
    script.push_str("    option.value = key;\n");
    script.push_str("    option.textContent = statLabels[key] ?? `Stat ${key}`;\n");
    script.push_str("    if (String(key) === String(selected)) {\n");
    script.push_str("      option.selected = true;\n");
    script.push_str("    }\n");
    script.push_str("    select.appendChild(option);\n");
    script.push_str("  }\n");
    script.push_str("};\n\n");

    script.push_str("const updateSubOptions = () => {\n");
    script.push_str("  const mainKey = Number(mainSelect.value);\n");
    script.push_str("  const subKeys = subOptionsByMain[mainKey] ?? [];\n");
    script.push_str("  for (const select of subSelects) {\n");
    script.push_str("    const current = select.value;\n");
    script.push_str("    renderOptions(select, subKeys, subKeys.includes(Number(current)) ? current : subKeys[0]);\n");
    script.push_str("  }\n");
    script.push_str("};\n\n");

    script.push_str("const updateMainOptions = () => {\n");
    script.push_str("  const slot = Number(slotSelect.value);\n");
    script.push_str("  const keys = mainOptionsBySlot[slot] ?? [];\n");
    script.push_str("  renderOptions(mainSelect, keys, keys[0]);\n");
    script.push_str("  updateSubOptions();\n");
    script.push_str("};\n\n");

    script.push_str("slotSelect.addEventListener(\"change\", updateMainOptions);\n");
    script.push_str("mainSelect.addEventListener(\"change\", updateSubOptions);\n");
    script.push_str("updateMainOptions();\n");
    script.push_str("</script>");
    script
}

fn render_slot_options(selected: u32) -> String {
    let mut html = String::new();
    for slot in 1..=6 {
        html.push_str(&format!(
            "<option value=\"{}\"{}>Slot {}</option>",
            slot,
            if slot == selected { " selected" } else { "" },
            slot
        ));
    }
    html
}

fn parse_slot_value(value: &str) -> u32 {
    value.trim().parse::<u32>().unwrap_or(0)
}

fn resolve_player_uid(state: &AppState, account_uid: i32) -> u32 {
    let account_path = state
        .state_dir
        .join(format!("account/{account_uid}"));

    if let Some(account_zon) = read_zon(&account_path) {
        if let Some(player_uid) = zon_get_number(&account_zon, "player_uid") {
            return player_uid as u32;
        }
    }

    let direct_path = state
        .state_dir
        .join(format!("player/{account_uid}"));
    if direct_path.exists() {
        return account_uid as u32;
    }

    let player_root = state.state_dir.join("player");
    if let Ok(entries) = fs::read_dir(player_root) {
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else { continue; };
            if !file_type.is_dir() {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                if let Ok(uid) = name.parse::<u32>() {
                    return uid;
                }
            }
        }
    }

    account_uid.max(1) as u32
}

fn resolve_item_path(state_dir: &FsPath, uid: u32, kind: &str, item_id: u32) -> PathBuf {
    let base = state_dir.join(format!("player/{uid}/{kind}/{item_id}"));
    if base.exists() {
        return base;
    }

    let zon = state_dir.join(format!("player/{uid}/{kind}/{item_id}.zon"));
    if zon.exists() {
        return zon;
    }

    base
}

fn load_avatar_templates(asset_dir: &FsPath) -> HashMap<u32, String> {
    let path = asset_dir.join("AvatarBaseTemplateTb.json");
    let data = fs::read_to_string(path).unwrap_or_default();
    parse_json_map(&data, "id", "name")
}

fn load_weapon_templates(asset_dir: &FsPath) -> HashMap<u32, String> {
    let path = asset_dir.join("WeaponTemplateTb.json");
    let data = fs::read_to_string(path).unwrap_or_default();
    parse_json_map(&data, "item_id", "weapon_name")
}

fn load_player_weapons(state: &AppState, uid: u32) -> Vec<(u32, String)> {
    let weapon_dir = state.state_dir.join(format!("player/{uid}/weapon"));
    let weapon_templates = load_weapon_templates(&state.asset_dir);
    let hakushin = load_hakushin_data(state);
    let mut result = Vec::new();

    if let Ok(entries) = fs::read_dir(&weapon_dir) {
        for entry in entries.flatten() {
            let Some(file_name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            let uid = match file_name
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
            let name = hakushin
                .weapons
                .get(&weapon_id)
                .map(|entry| entry.name.clone())
                .or_else(|| weapon_templates.get(&weapon_id).cloned())
                .unwrap_or_else(|| format!("Weapon {weapon_id}"));
            result.push((uid, format!("{} (UID {})", name, uid)));
        }
    }

    result.sort_by_key(|(uid, _)| *uid);
    result
}

fn render_weapon_select(current_uid: u32, options: &[(u32, String)]) -> String {
    let mut html = String::new();
    html.push_str("<select name=\"cur_weapon_uid\">");
    html.push_str(&format!(
        "<option value=\"0\"{}>None</option>",
        if current_uid == 0 { " selected" } else { "" }
    ));
    for (uid, label) in options {
        html.push_str(&format!(
            "<option value=\"{}\"{}>{}</option>",
            uid,
            if *uid == current_uid { " selected" } else { "" },
            label
        ));
    }
    html.push_str("</select>");
    html
}

fn load_equip_templates(asset_dir: &FsPath) -> HashMap<u32, String> {
    let equip_path = asset_dir.join("EquipmentTemplateTb.json");
    let suit_path = asset_dir.join("EquipmentSuitTemplateTb.json");

    let equip_data = fs::read_to_string(equip_path).unwrap_or_default();
    let suit_data = fs::read_to_string(suit_path).unwrap_or_default();

    let equip_to_suit = parse_json_map_u32(&equip_data, "item_id", "suit_type");
    let suit_names = parse_json_map(&suit_data, "id", "name");

    let mut result = HashMap::new();
    for (item_id, suit_id) in equip_to_suit {
        let name = suit_names
            .get(&suit_id)
            .cloned()
            .unwrap_or_else(|| format!("Suit {suit_id}"));
        result.insert(item_id, name);
    }

    result
}

fn load_player_equips(state: &AppState, uid: u32) -> Vec<(u32, u32, String)> {
    let equip_dir = state.state_dir.join(format!("player/{uid}/equip"));
    let equip_templates = load_equip_templates(&state.asset_dir);
    let hakushin = load_hakushin_data(state);
    let equip_index = load_equip_template_index(&state.asset_dir);
    let mut result = Vec::new();

    if let Ok(entries) = fs::read_dir(&equip_dir) {
        for entry in entries.flatten() {
            let Some(file_name) = entry.file_name().to_str().map(|s| s.to_string()) else {
                continue;
            };
            let uid = match file_name
                .strip_suffix(".zon")
                .unwrap_or(&file_name)
                .parse::<u32>()
            {
                Ok(value) if value > 0 => value,
                _ => continue,
            };
            let equip = read_zon(&entry.path());
            let equip_item_id = equip
                .as_ref()
                .and_then(|v| zon_get_number(v, "id"))
                .unwrap_or(0) as u32;
            let set_id = equip_set_id(equip_item_id, equip_index);
            let name = hakushin
                .discs
                .get(&set_id)
                .map(|entry| entry.name.clone())
                .or_else(|| equip_templates.get(&equip_item_id).cloned())
                .unwrap_or_else(|| format!("Disc {equip_item_id}"));
            let slot = equip_slot(equip_item_id, equip_index);
            result.push((uid, slot, format!("{} (UID {})", name, uid)));
        }
    }

    result.sort_by_key(|(uid, _, _)| *uid);
    result
}

fn render_equip_selects(options: &[(u32, u32, String)], equipped: &[u32]) -> String {
    let mut html = String::new();
    for slot in 0..6 {
        let current = *equipped.get(slot).unwrap_or(&0);
        html.push_str("<div><label>Slot ");
        html.push_str(&(slot + 1).to_string());
        html.push_str("</label><select name=\"equip_slot_");
        html.push_str(&(slot + 1).to_string());
        html.push_str("\">");
        html.push_str(&format!(
            "<option value=\"0\"{}>Empty</option>",
            if current == 0 { " selected" } else { "" }
        ));
        for (uid, opt_slot, label) in options {
            if *opt_slot != (slot as u32 + 1) {
                continue;
            }
            html.push_str(&format!(
                "<option value=\"{}\"{}>{}</option>",
                uid,
                if *uid == current { " selected" } else { "" },
                label
            ));
        }
        html.push_str("</select></div>");
    }

    html
}

fn parse_json_map(data: &str, key: &str, value: &str) -> HashMap<u32, String> {
    let mut result = HashMap::new();
    let Ok(json) = serde_json::from_str::<JsonValue>(data) else {
        return result;
    };

    let Some(items) = json.get("data").and_then(|v| v.as_array()) else {
        return result;
    };

    for item in items {
        let Some(id) = item.get(key).and_then(|v| v.as_u64()) else {
            continue;
        };
        if let Some(name) = item.get(value).and_then(|v| v.as_str()) {
            result.insert(id as u32, name.to_string());
        }
    }

    result
}

fn parse_json_map_u32(data: &str, key: &str, value: &str) -> HashMap<u32, u32> {
    let mut result = HashMap::new();
    let Ok(json) = serde_json::from_str::<JsonValue>(data) else {
        return result;
    };

    let Some(items) = json.get("data").and_then(|v| v.as_array()) else {
        return result;
    };

    for item in items {
        let Some(id) = item.get(key).and_then(|v| v.as_u64()) else {
            continue;
        };
        if let Some(value) = item.get(value).and_then(|v| v.as_u64()) {
            result.insert(id as u32, value as u32);
        }
    }

    result
}

fn svg_data_uri(label: &str) -> String {
    let mut safe = label
        .replace('%', "%25")
        .replace('#', "%23")
        .replace('<', "%3C")
        .replace('>', "%3E")
        .replace('"', "%22")
        .replace(' ', "%20");

    if safe.len() > 32 {
        safe.truncate(32);
    }

    format!(
        "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='320' height='180'><rect width='100%25' height='100%25' fill='%23131a24'/><text x='50%25' y='50%25' dominant-baseline='middle' text-anchor='middle' fill='%239aa4b2' font-size='14' font-family='sans-serif'>{}</text></svg>",
        safe
    )
}

#[derive(Debug, Clone)]
enum ZValue {
    Null,
    Bool(bool),
    Number(i64),
    String(String),
    Enum(String),
    Array(Vec<ZValue>),
    Object(Vec<(String, ZValue)>),
}

fn read_zon(path: &FsPath) -> Option<ZValue> {
    let data = fs::read_to_string(path).ok()?;
    parse_zon(&data).ok()
}

fn read_zon_verbose(path: &FsPath) -> Option<ZValue> {
    let data = fs::read_to_string(path).ok()?;
    match parse_zon(&data) {
        Ok(value) => Some(value),
        Err(err) => {
            eprintln!("[zon] parse failed path={} err={}", path.display(), err);
            None
        }
    }
}

fn parse_zon(data: &str) -> Result<ZValue, String> {
    let mut parser = ZonParser::new(data);
    parser.parse_value()
}

fn zon_serialize(value: &ZValue) -> String {
    let mut out = String::new();
    serialize_zon_pretty(value, &mut out, 0);
    out
}

fn format_zon_pretty(content: &str) -> String {
    let mut out = match parse_zon(content) {
        Ok(value) => zon_serialize(&value),
        Err(_) => content.to_string(),
    };
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn serialize_zon_pretty(value: &ZValue, out: &mut String, level: usize) {
    match value {
        ZValue::Null => out.push_str("null"),
        ZValue::Bool(v) => out.push_str(if *v { "true" } else { "false" }),
        ZValue::Number(v) => out.push_str(&v.to_string()),
        ZValue::String(v) => {
            out.push('"');
            out.push_str(&v.replace('\\', "\\\\").replace('"', "\\\""));
            out.push('"');
        }
        ZValue::Enum(v) => {
            out.push('.');
            out.push_str(v);
        }
        ZValue::Array(items) => {
            out.push_str(".{");
            if !items.is_empty() {
                out.push('\n');
                for item in items {
                    write_indent(out, level + 1);
                    serialize_zon_pretty(item, out, level + 1);
                    out.push_str(",\n");
                }
                write_indent(out, level);
            }
            out.push('}');
        }
        ZValue::Object(fields) => {
            out.push_str(".{");
            if !fields.is_empty() {
                out.push('\n');
                for (key, value) in fields {
                    write_indent(out, level + 1);
                    out.push('.');
                    out.push_str(key);
                    out.push_str(" = ");
                    serialize_zon_pretty(value, out, level + 1);
                    out.push_str(",\n");
                }
                write_indent(out, level);
            }
            out.push('}');
        }
    }
}

fn write_indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str("    ");
    }
}

struct ZonParser<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ZonParser<'a> {
    fn new(data: &'a str) -> Self {
        Self {
            data: data.as_bytes(),
            pos: 0,
        }
    }

    fn parse_value(&mut self) -> Result<ZValue, String> {
        self.skip_ws();
        match self.peek_char() {
            Some(b'.') => {
                if self.peek_next_is(b".{") {
                    self.parse_container()
                } else {
                    self.consume_char();
                    let ident = self.parse_ident()?;
                    Ok(ZValue::Enum(ident))
                }
            }
            Some(b'"') => Ok(ZValue::String(self.parse_string()?)),
            Some(b't') | Some(b'f') => Ok(ZValue::Bool(self.parse_bool()?)),
            Some(b'n') => {
                self.expect_keyword("null")?;
                Ok(ZValue::Null)
            }
            Some(b'-') | Some(b'0'..=b'9') => Ok(ZValue::Number(self.parse_number()?)),
            other => Err(format!(
                "unexpected token at {} ({:?})",
                self.pos, other
            )),
        }
    }

    fn parse_container(&mut self) -> Result<ZValue, String> {
        self.expect_keyword(".{")?;
        self.skip_ws();
        let mut entries: Vec<ContainerEntry> = Vec::new();

        while !self.peek_char_is(b'}') {
            self.skip_ws();
            if self.peek_char_is(b'}') {
                break;
            }
            if self.peek_char_is(b'.') {
                if self.peek_next_is(b".{") {
                    let value = self.parse_value()?;
                    entries.push(ContainerEntry::Value(value));
                    self.skip_ws();
                    if self.peek_char_is(b',') {
                        self.consume_char();
                    }
                    continue;
                }
                let save_pos = self.pos;
                self.consume_char();
                let ident = self.parse_ident()?;
                self.skip_ws();
                if self.peek_char_is(b'=') {
                    self.consume_char();
                    let value = self.parse_value()?;
                    entries.push(ContainerEntry::Field(ident, value));
                } else {
                    self.pos = save_pos;
                    let value = self.parse_value()?;
                    entries.push(ContainerEntry::Value(value));
                }
            } else {
                let value = self.parse_value()?;
                entries.push(ContainerEntry::Value(value));
            }

            self.skip_ws();
            if self.peek_char_is(b',') {
                self.consume_char();
                self.skip_ws();
            } else {
                break;
            }
        }

        if self.peek_char_is(b'}') {
            self.consume_char();
        }

        let is_object = entries.iter().any(|e| matches!(e, ContainerEntry::Field(_, _)));
        if is_object {
            let mut fields = Vec::new();
            for entry in entries {
                if let ContainerEntry::Field(key, value) = entry {
                    fields.push((key, value));
                }
            }
            Ok(ZValue::Object(fields))
        } else {
            let mut values = Vec::new();
            for entry in entries {
                if let ContainerEntry::Value(value) = entry {
                    values.push(value);
                }
            }
            Ok(ZValue::Array(values))
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.expect_char(b'"')?;
        let mut out = String::new();
        while let Some(ch) = self.peek_char() {
            self.consume_char();
            match ch {
                b'"' => break,
                b'\\' => {
                    if let Some(next) = self.peek_char() {
                        self.consume_char();
                        out.push(next as char);
                    }
                }
                _ => out.push(ch as char),
            }
        }
        Ok(out)
    }

    fn parse_bool(&mut self) -> Result<bool, String> {
        if self.consume_if_keyword("true") {
            Ok(true)
        } else if self.consume_if_keyword("false") {
            Ok(false)
        } else {
            Err("invalid bool".into())
        }
    }

    fn parse_number(&mut self) -> Result<i64, String> {
        let start = self.pos;
        if self.peek_char_is(b'-') {
            self.consume_char();
        }
        while matches!(self.peek_char(), Some(b'0'..=b'9')) {
            self.consume_char();
        }
        let slice = std::str::from_utf8(&self.data[start..self.pos]).map_err(|_| "bad number")?;
        slice.parse::<i64>().map_err(|_| "bad number".into())
    }

    fn parse_ident(&mut self) -> Result<String, String> {
        let start = self.pos;
        while matches!(self.peek_char(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'0'..=b'9')) {
            self.consume_char();
        }
        if self.pos == start {
            return Err("expected identifier".into());
        }
        let slice = std::str::from_utf8(&self.data[start..self.pos]).map_err(|_| "bad ident")?;
        Ok(slice.to_string())
    }

    fn skip_ws(&mut self) {
        loop {
            while matches!(self.peek_char(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
                self.consume_char();
            }

            if self.peek_next_is(b"//") {
                while let Some(ch) = self.peek_char() {
                    self.consume_char();
                    if ch == b'\n' {
                        break;
                    }
                }
                continue;
            }

            if self.peek_next_is(b"/*") {
                self.consume_char();
                self.consume_char();
                while let Some(_) = self.peek_char() {
                    if self.peek_next_is(b"*/") {
                        self.consume_char();
                        self.consume_char();
                        break;
                    }
                    self.consume_char();
                }
                continue;
            }

            break;
        }
    }

    fn peek_char(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn peek_char_is(&self, ch: u8) -> bool {
        self.peek_char() == Some(ch)
    }

    fn consume_char(&mut self) {
        self.pos = self.pos.saturating_add(1);
    }

    fn expect_char(&mut self, ch: u8) -> Result<(), String> {
        if self.peek_char_is(ch) {
            self.consume_char();
            Ok(())
        } else {
            Err(format!("expected '{}'", ch as char))
        }
    }

    fn expect_keyword(&mut self, keyword: &str) -> Result<(), String> {
        if self.consume_if_keyword(keyword) {
            Ok(())
        } else {
            Err(format!("expected {keyword}"))
        }
    }

    fn consume_if_keyword(&mut self, keyword: &str) -> bool {
        if self.data[self.pos..].starts_with(keyword.as_bytes()) {
            self.pos += keyword.len();
            true
        } else {
            false
        }
    }

    fn peek_next_is(&self, bytes: &[u8]) -> bool {
        self.data[self.pos..].starts_with(bytes)
    }
}

enum ContainerEntry {
    Field(String, ZValue),
    Value(ZValue),
}

fn zon_get_number(value: &ZValue, field: &str) -> Option<i64> {
    match value {
        ZValue::Object(fields) => fields.iter().find_map(|(k, v)| {
            if k == field {
                if let ZValue::Number(num) = v {
                    Some(*num)
                } else {
                    None
                }
            } else {
                None
            }
        }),
        _ => None,
    }
}

fn zon_set_number(value: &mut ZValue, field: &str, num: i64) {
    if let ZValue::Object(fields) = value {
        if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == field) {
            *v = ZValue::Number(num);
            return;
        }
        fields.push((field.to_string(), ZValue::Number(num)));
    }
}

fn zon_get_array_numbers(value: &ZValue, field: &str) -> Vec<u32> {
    match value {
        ZValue::Object(fields) => fields
            .iter()
            .find(|(k, _)| k == field)
            .and_then(|(_, v)| match v {
                ZValue::Array(items) => Some(
                    items
                        .iter()
                        .filter_map(|item| match item {
                            ZValue::Number(n) => Some(*n as u32),
                            _ => None,
                        })
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .unwrap_or_default(),
        _ => vec![],
    }
}

fn zon_set_dressed_equip(value: &mut ZValue, field: &str, items: &[u32], slots: usize) {
    let mut array_items = Vec::with_capacity(slots);
    for idx in 0..slots {
        if let Some(uid) = items.get(idx) {
            if *uid == 0 {
                array_items.push(ZValue::Null);
            } else {
                array_items.push(ZValue::Number(*uid as i64));
            }
        } else {
            array_items.push(ZValue::Null);
        }
    }

    let array = ZValue::Array(array_items);
    if let ZValue::Object(fields) = value {
        if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == field) {
            *v = array;
            return;
        }
        fields.push((field.to_string(), array));
    }
}

fn zon_get_skill_levels(value: &ZValue) -> HashMap<String, u32> {
    let mut result = HashMap::new();
    if let ZValue::Object(fields) = value {
        if let Some((_, ZValue::Array(skills))) = fields.iter().find(|(k, _)| k == "skill_type_level") {
            for skill in skills {
                if let ZValue::Object(skill_fields) = skill {
                    let mut key = None;
                    let mut level = None;
                    for (k, v) in skill_fields {
                        if k == "type" {
                            if let ZValue::Enum(name) = v {
                                key = Some(name.clone());
                            }
                        }
                        if k == "level" {
                            if let ZValue::Number(num) = v {
                                level = Some(*num as u32);
                            }
                        }
                    }
                    if let (Some(key), Some(level)) = (key, level) {
                        result.insert(key, level);
                    }
                }
            }
        }
    }

    result
}

fn zon_set_skill_levels(value: &mut ZValue, levels: &mut Vec<(&str, u32)>) {
    if let ZValue::Object(fields) = value {
        let mut existing: HashMap<String, u32> = HashMap::new();
        if let Some((_, ZValue::Array(skills))) = fields.iter().find(|(k, _)| k == "skill_type_level") {
            for skill in skills {
                if let ZValue::Object(skill_fields) = skill {
                    let mut key = None;
                    let mut level = None;
                    for (k, v) in skill_fields {
                        if k == "type" {
                            if let ZValue::Enum(name) = v {
                                key = Some(name.clone());
                            }
                        }
                        if k == "level" {
                            if let ZValue::Number(num) = v {
                                level = Some(*num as u32);
                            }
                        }
                    }
                    if let (Some(key), Some(level)) = (key, level) {
                        existing.insert(key, level);
                    }
                }
            }
        }

        for (name, lvl) in levels.iter() {
            existing.insert((*name).to_string(), *lvl);
        }

        let mut array = Vec::new();
        for (name, lvl) in levels.iter() {
            array.push(ZValue::Object(vec![
                ("type".to_string(), ZValue::Enum((*name).to_string())),
                ("level".to_string(), ZValue::Number(*lvl as i64)),
            ]));
        }

        for (name, lvl) in existing {
            if levels.iter().any(|(known, _)| *known == name) {
                continue;
            }
            array.push(ZValue::Object(vec![
                ("type".to_string(), ZValue::Enum(name)),
                ("level".to_string(), ZValue::Number(lvl as i64)),
            ]));
        }

        if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == "skill_type_level") {
            *v = ZValue::Array(array);
            return;
        }
        fields.push(("skill_type_level".to_string(), ZValue::Array(array)));
    }
}

fn zon_get_entrance_zone_id(value: &ZValue, entrance_id: u32) -> Option<u32> {
    let ZValue::Object(fields) = value else { return None; };
    let Some((_, ZValue::Array(entrances))) = fields.iter().find(|(k, _)| k == "entrances") else {
        return None;
    };
    for entry in entrances {
        let ZValue::Object(items) = entry else { continue; };
        let mut id = None;
        let mut zone_id = None;
        for (k, v) in items {
            if k == "id" {
                if let ZValue::Number(num) = v {
                    id = Some(*num as u32);
                }
            }
            if k == "zone_id" {
                if let ZValue::Number(num) = v {
                    zone_id = Some(*num as u32);
                }
            }
        }
        if id == Some(entrance_id) {
            return zone_id;
        }
    }
    None
}

fn zon_set_entrance_zone_id(value: &mut ZValue, entrance_id: u32, zone_id: u32) {
    let ZValue::Object(fields) = value else { return; };

    let entrances_index = fields.iter().position(|(k, _)| k == "entrances");
    if entrances_index.is_none() {
        fields.push(("entrances".to_string(), ZValue::Array(Vec::new())));
    }
    let entrances_index = entrances_index.unwrap_or(fields.len().saturating_sub(1));

    let items = match &mut fields[entrances_index].1 {
        ZValue::Array(items) => items,
        _ => {
            fields[entrances_index].1 = ZValue::Array(Vec::new());
            match &mut fields[entrances_index].1 {
                ZValue::Array(items) => items,
                _ => return,
            }
        }
    };

    for entry in items.iter_mut() {
        let ZValue::Object(entry_fields) = entry else { continue; };
        let mut id = None;
        for (k, v) in entry_fields.iter() {
            if k == "id" {
                if let ZValue::Number(num) = v {
                    id = Some(*num as u32);
                }
            }
        }
        if id == Some(entrance_id) {
            if let Some((_, v)) = entry_fields.iter_mut().find(|(k, _)| k == "zone_id") {
                *v = ZValue::Number(zone_id as i64);
            } else {
                entry_fields.push(("zone_id".to_string(), ZValue::Number(zone_id as i64)));
            }
            return;
        }
    }

    items.push(ZValue::Object(vec![
        ("id".to_string(), ZValue::Number(entrance_id as i64)),
        ("zone_id".to_string(), ZValue::Number(zone_id as i64)),
    ]));
}

fn zon_get_main_property(value: &ZValue) -> (u32, u32, u32) {
    if let ZValue::Object(fields) = value {
        if let Some((_, ZValue::Array(properties))) = fields.iter().find(|(k, _)| k == "properties") {
            if let Some(ZValue::Object(prop_fields)) = properties.first() {
                let key = prop_fields
                    .iter()
                    .find(|(k, _)| k == "key")
                    .and_then(|(_, v)| match v {
                        ZValue::Number(num) => Some(*num as u32),
                        _ => None,
                    })
                    .unwrap_or(0);
                let base = prop_fields
                    .iter()
                    .find(|(k, _)| k == "base_value")
                    .and_then(|(_, v)| match v {
                        ZValue::Number(num) => Some(*num as u32),
                        _ => None,
                    })
                    .unwrap_or(0);
                let add = prop_fields
                    .iter()
                    .find(|(k, _)| k == "add_value")
                    .and_then(|(_, v)| match v {
                        ZValue::Number(num) => Some(*num as u32),
                        _ => None,
                    })
                    .unwrap_or(0);
                return (key, base, add);
            }
        }
    }

    (0, 0, 0)
}

fn zon_set_main_property(value: &mut ZValue, key: u32, base: u32, add: u32) {
    let prop = ZValue::Object(vec![
        ("key".to_string(), ZValue::Number(key as i64)),
        ("base_value".to_string(), ZValue::Number(base as i64)),
        ("add_value".to_string(), ZValue::Number(add as i64)),
    ]);

    if let ZValue::Object(fields) = value {
        if let Some((_, ZValue::Array(properties))) = fields.iter_mut().find(|(k, _)| k == "properties") {
            if properties.is_empty() {
                properties.push(prop);
            } else {
                properties[0] = prop;
            }
            return;
        }
        fields.push(("properties".to_string(), ZValue::Array(vec![prop])));
    }
}

fn zon_get_sub_properties_list(value: &ZValue) -> Vec<(u32, u32, u32)> {
    let mut list = Vec::new();
    if let ZValue::Object(fields) = value {
        if let Some((_, ZValue::Array(properties))) = fields.iter().find(|(k, _)| k == "sub_properties") {
            for prop in properties {
                if let ZValue::Object(prop_fields) = prop {
                    let key = prop_fields
                        .iter()
                        .find(|(k, _)| k == "key")
                        .and_then(|(_, v)| match v {
                            ZValue::Number(num) => Some(*num as u32),
                            _ => None,
                        })
                        .unwrap_or(0);
                    let base = prop_fields
                        .iter()
                        .find(|(k, _)| k == "base_value")
                        .and_then(|(_, v)| match v {
                            ZValue::Number(num) => Some(*num as u32),
                            _ => None,
                        })
                        .unwrap_or(0);
                    let add = prop_fields
                        .iter()
                        .find(|(k, _)| k == "add_value")
                        .and_then(|(_, v)| match v {
                            ZValue::Number(num) => Some(*num as u32),
                            _ => None,
                        })
                        .unwrap_or(0);
                    list.push((key, base, add));
                }
            }
        }
    }

    list
}

fn zon_set_sub_properties(value: &mut ZValue, keys: &[u32], base: &[u32], add: &[u32]) {
    let mut list = Vec::new();
    for idx in 0..keys.len() {
        let base_val = base.get(idx).copied().unwrap_or(0);
        let add_val = add.get(idx).copied().unwrap_or(0);
        list.push(ZValue::Object(vec![
            ("key".to_string(), ZValue::Number(keys[idx] as i64)),
            ("base_value".to_string(), ZValue::Number(base_val as i64)),
            ("add_value".to_string(), ZValue::Number(add_val as i64)),
        ]));
    }

    if let ZValue::Object(fields) = value {
        if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == "sub_properties") {
            *v = ZValue::Array(list);
            return;
        }
        fields.push(("sub_properties".to_string(), ZValue::Array(list)));
    }
}
