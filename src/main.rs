use axum::{
    body::Body,
    extract::{Form, OriginalUri, Path, Query, RawForm, State},
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
    routing::{get, post},
    Router,
};
use password_hash::PasswordHash;
use pbkdf2::Pbkdf2;
use rand::{distributions::Alphanumeric, seq::SliceRandom, Rng};
use rusqlite::{params, Connection};
use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::{
    collections::HashMap,
    env,
    fs,
    hash::{Hash, Hasher},
    path::{Path as FsPath, PathBuf},
    sync::{Mutex, OnceLock},
};

static SESSION_STORE: OnceLock<Mutex<HashMap<String, Session>>> = OnceLock::new();
static HAKUSHIN_DATA: OnceLock<Mutex<Option<(u64, HakushinData)>>> = OnceLock::new();
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

#[derive(Default, Clone)]
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
    next: Option<String>,
}

#[derive(Deserialize)]
struct LoginQuery {
    next: Option<String>,
}

#[derive(Deserialize)]
struct SdkConfig {
    db_file: String,
}

#[derive(Deserialize)]
struct TabQuery {
    tab: Option<String>,
    delete: Option<u8>,
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
struct GenerateEquipForm {
    equip_set_id: u32,
    slot: Option<String>,
    count: u32,
}

#[derive(Deserialize)]
struct DaShiyuForm {
    shiyu_zone_id: u32,
    deadly_assault_zone_id: u32,
}

#[derive(Deserialize)]
struct ShiyuDetailQuery {
    floor: Option<u32>,
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
        .route("/equip/generate", get(equip_generate).post(equip_generate_submit))
        .route("/equip/delete", post(equip_delete_submit))
        .route("/bangboo/:uid", get(bangboo_edit).post(bangboo_update))
        .route("/da/:id", get(da_detail))
        .route("/da/:id/select", post(da_select))
        .route("/shiyu/:id", get(shiyu_detail))
        .route("/shiyu/:id/select", post(shiyu_select))
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

async fn login_page(Query(query): Query<LoginQuery>) -> Html<String> {
    let next = query
        .next
        .as_deref()
        .and_then(sanitize_next_path)
        .unwrap_or_else(|| "/dashboard".to_string());
    let next_attr = html_escape_attr(&next);

        let body = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Gear Editor - Login</title>
  <style>
    body { font-family: system-ui, sans-serif; background: #0f1115; color: #e6e6e6; display: grid; place-items: center; height: 100vh; margin: 0; }
    form { background: #1b1f2a; padding: 24px; border-radius: 12px; width: 320px; box-shadow: 0 10px 30px rgba(0,0,0,.4); display: flex; flex-direction: column; gap: 12px; }
    h1 { font-size: 18px; margin: 0; }
    .field { display: flex; flex-direction: column; gap: 6px; }
    label { display: block; margin: 0; font-size: 12px; color: #9aa4b2; }
    input { width: 100%; box-sizing: border-box; padding: 10px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }
    button { width: 100%; padding: 10px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }
  </style>
</head>
<body>
  <form method="post" action="/login">
    <h1>Gear Editor</h1>
        <input type="hidden" name="next" value="{next_attr}" />
        <div class="field">
            <label for="username">Username</label>
            <input id="username" name="username" autocomplete="username" required />
        </div>
        <div class="field">
            <label for="password">Password</label>
            <input id="password" name="password" type="password" autocomplete="current-password" required />
        </div>
    <button type="submit">Sign in</button>
  </form>
</body>
</html>"#
    .replace("{next_attr}", &next_attr);

    Html(body)
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

            let next = payload
                .next
                .as_deref()
                .and_then(sanitize_next_path)
                .unwrap_or_else(|| "/dashboard".to_string());
            (headers, Redirect::to(&next)).into_response()
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
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let tab = query.tab.unwrap_or_else(|| "avatars".to_string());
    let delete_mode = query.delete.unwrap_or(0) == 1;
    let uid = resolve_player_uid(&state, session.uid);

    let avatar_cards = render_avatar_cards(&state, uid);
    let weapon_cards = render_weapon_cards(&state, uid);
    let equip_cards = render_equip_cards(&state, uid, delete_mode);
    let bangboo_cards = render_bangboo_cards(&state, uid);
    let da_panel = render_da_panel(&state, uid);
    let shiyu_panel = render_shiyu_panel(&state, uid);

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
        .panel form {{ width: 100%; }}
        .panel label {{ display: block; margin: 12px 0 6px; font-size: 12px; color: #9aa4b2; }}
        .panel input, .panel select {{ width: 100%; box-sizing: border-box; padding: 8px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
        .panel button {{ margin-top: 16px; padding: 10px 14px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }}
        .row {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 12px; }}
    .apply {{ background: #22c55e; color: #0b1220; border: 0; padding: 8px 14px; border-radius: 8px; font-weight: 600; cursor: pointer; }}
    .pill {{ display: inline-block; padding: 4px 8px; background: #2a3140; border-radius: 999px; font-size: 12px; color: #9aa4b2; }}
        .danger {{ background: #ef4444; color: #fff; border: 0; padding: 8px 14px; border-radius: 8px; font-weight: 600; cursor: pointer; }}
        .select-card {{ cursor: pointer; }}
        .select-card input[type="checkbox"] {{ width: auto; margin-bottom: 10px; }}
  </style>
</head>
<body>
<header>
  <div class="tabs">
    <a class="{tab_avatar}" href="/dashboard?tab=avatars">Characters</a>
    <a class="{tab_weapon}" href="/dashboard?tab=weapons">Weapons</a>
    <a class="{tab_equip}" href="/dashboard?tab=discs">Discs</a>
    <a class="{tab_bangboo}" href="/dashboard?tab=bangboos">Bangboos</a>
    <a class="{tab_da}" href="/dashboard?tab=da">Deadly Assault</a>
    <a class="{tab_shiyu}" href="/dashboard?tab=shiyu">Shiyu</a>
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
        tab_da = if tab == "da" { "active" } else { "" },
        tab_shiyu = if tab == "shiyu" { "active" } else { "" },
        content = match tab.as_str() {
            "weapons" => weapon_cards,
            "discs" => equip_cards,
            "bangboos" => bangboo_cards,
            "da" => da_panel,
            "shiyu" => shiyu_panel,
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
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
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
    original_uri: OriginalUri,
    Form(payload): Form<AvatarUpdateForm>,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
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
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
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
    original_uri: OriginalUri,
    Form(payload): Form<WeaponUpdateForm>,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
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
    original_uri: OriginalUri,
    Form(payload): Form<AddWeaponForm>,
) -> impl IntoResponse {
    let Some((session_id, session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
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
    original_uri: OriginalUri,
) -> impl IntoResponse {
        let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
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
    original_uri: OriginalUri,
        Form(payload): Form<BangbooUpdateForm>,
) -> impl IntoResponse {
        let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
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
    original_uri: OriginalUri,
    Form(payload): Form<DaShiyuForm>,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
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
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
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
    let mut sub_options_by_main = HashMap::new();
    let mut label_map = HashMap::new();
    for slot_id in 1..=6 {
        let options = disk_main_stat_options(slot_id);
        for key in options {
            label_map.entry(key).or_insert_with(|| stat_label(&state, key));
            let sub_opts = disk_sub_stat_options(key);
            for sub_key in &sub_opts {
                label_map
                    .entry(*sub_key)
                    .or_insert_with(|| stat_label(&state, *sub_key));
            }
            sub_options_by_main.insert(key, sub_opts);
        }
    }
    let sub_options_by_main_json = serde_json::to_string(&sub_options_by_main).unwrap_or_default();
    let label_map_json = serde_json::to_string(&label_map).unwrap_or_default();
    let script = render_equip_substat_script("{}", &sub_options_by_main_json, &label_map_json);
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
    input, select {{ width: 100%; box-sizing: border-box; padding: 8px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
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
        script = script,
        warning = warning,
    );

    Html(body).into_response()
}

async fn equip_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(equip_uid): Path<u32>,
    original_uri: OriginalUri,
    Form(payload): Form<EquipUpdateForm>,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
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
    original_uri: OriginalUri,
) -> impl IntoResponse {
        let Some((_session_id, _session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
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
        let script = render_equip_substat_script(
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
    original_uri: OriginalUri,
    Form(payload): Form<AddEquipForm>,
) -> impl IntoResponse {
    let Some((session_id, session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
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
    Redirect::to("/dashboard?tab=discs").into_response()
}

async fn equip_generate(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, _session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let options = render_disc_select_options(&state, 0);
    let slot_options = render_generate_slot_options(None);
    let body = format!(
        r#"<!doctype html>
<html lang="en">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Generate Discs</title>
    <style>
        body {{ font-family: system-ui, sans-serif; margin: 0; background: #0f1115; color: #e6e6e6; }}
        .container {{ padding: 24px; max-width: 900px; margin: 0 auto; }}
        input[type="number"], select {{ width: 100%; padding: 8px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
        label {{ display: block; margin: 12px 0 6px; font-size: 12px; color: #9aa4b2; }}
        button {{ margin-top: 16px; padding: 10px 14px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }}
        .row {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 12px; }}
        .meta {{ color: #9aa4b2; font-size: 12px; }}
    </style>
</head>
<body>
    <div class="container">
        <h1>Generate Discs</h1>
        <div class="meta">Each generated disc uses valid slot/main stat combinations, 4 unique substats, and total procs in range 8-9.</div>
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
                    <select name="slot">
                        {slot_options}
                    </select>
                </div>
                <div>
                    <label>Count</label>
                    <input name="count" type="number" min="1" max="200" value="10" required />
                </div>
            </div>
            <button type="submit">Generate</button>
        </form>
    </div>
</body>
</html>"#,
        options = options,
    slot_options = slot_options,
    );

    Html(body).into_response()
}

async fn equip_generate_submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
    Form(payload): Form<GenerateEquipForm>,
) -> impl IntoResponse {
    let Some((session_id, session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    if payload.count == 0 || payload.count > 200 {
        return (
            StatusCode::BAD_REQUEST,
            Html("Count must be between 1 and 200"),
        )
            .into_response();
    }
    let selected_slot = payload
        .slot
        .as_deref()
        .map(parse_slot_value)
        .unwrap_or(0);
    if selected_slot > 6 {
        return (StatusCode::BAD_REQUEST, Html("Slot must be 0..6")).into_response();
    }

    let uid = resolve_player_uid(&state, session.uid);
    let equip_dir = state.state_dir.join(format!("player/{uid}/equip"));
    let equip_index = load_equip_template_index(&state.asset_dir);
    let mut next_uid = read_next_uid(&equip_dir).unwrap_or(1).max(1);
    let mut rng = rand::thread_rng();

    for _ in 0..payload.count {
        let equip = match generate_random_disc(payload.equip_set_id, selected_slot, equip_index, &mut rng) {
            Ok(value) => value,
            Err(message) => return (StatusCode::BAD_REQUEST, Html(message)).into_response(),
        };

        let equip_path = equip_dir.join(next_uid.to_string());
        let serialized = format_zon_pretty(&zon_serialize(&equip));
        if let Some(parent) = equip_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(err) = fs::write(&equip_path, serialized) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html(format!("Failed to create generated disc: {}", err)),
            )
                .into_response();
        }

        next_uid += 1;
    }

    let next_path = equip_dir.join("next");
    if let Err(err) = fs::write(&next_path, format!("{}\n", next_uid)) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("Failed to update disc counter: {}", err)),
        )
            .into_response();
    }

    set_session(session_id, session);
    Redirect::to("/dashboard?tab=discs").into_response()
}

async fn equip_delete_submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
    RawForm(raw_form): RawForm,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let uid = resolve_player_uid(&state, session.uid);
    let raw_form_text = String::from_utf8_lossy(&raw_form).into_owned();
    let selected = parse_selected_equip_uids(&raw_form_text);

    for equip_uid in selected {
        let primary_path = state
            .state_dir
            .join(format!("player/{uid}/equip/{equip_uid}"));
        let zon_path = state
            .state_dir
            .join(format!("player/{uid}/equip/{equip_uid}.zon"));

        if primary_path.exists() {
            let _ = fs::remove_file(&primary_path);
        }
        if zon_path.exists() {
            let _ = fs::remove_file(&zon_path);
        }

        session.pending_writes.remove(&primary_path);
        session.pending_writes.remove(&zon_path);
    }

    set_session(session_id, session);
    Redirect::to("/dashboard?tab=discs").into_response()
}

fn parse_selected_equip_uids(raw_form: &str) -> Vec<u32> {
    let mut ids = Vec::new();
    for pair in raw_form.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or("");
        let value = parts.next().unwrap_or("");
        if key == "equip_uids" || key == "equip_uids[]" || key == "equip_uids%5B%5D" {
            if let Ok(id) = value.parse::<u32>() {
                ids.push(id);
            }
        }
    }
    ids.sort_unstable();
    ids.dedup();
    ids
}

fn generate_random_disc(
    set_id: u32,
    selected_slot: u32,
    equip_index: &EquipTemplateIndex,
    rng: &mut impl Rng,
) -> Result<ZValue, String> {
    let mut slots = Vec::new();
    for slot in 1..=6 {
        if resolve_equip_item_id(set_id, slot, equip_index).is_some() {
            slots.push(slot);
        }
    }
    if slots.is_empty() {
        return Err("Invalid disc set".to_string());
    }

    let slot = if selected_slot == 0 {
        *slots.choose(rng).ok_or_else(|| "No slots for disc set".to_string())?
    } else {
        if !(1..=6).contains(&selected_slot) {
            return Err("Invalid slot".to_string());
        }
        if !slots.contains(&selected_slot) {
            return Err("Selected slot is not available for this disc set".to_string());
        }
        selected_slot
    };
    let item_id = resolve_equip_item_id(set_id, slot, equip_index)
        .map(force_disc_fourth_digit)
        .ok_or_else(|| "Invalid disc set/slot combination".to_string())?;

    let main_options = disk_main_stat_options(slot);
    let main_key = *main_options
        .choose(rng)
        .ok_or_else(|| "No valid main stats for selected slot".to_string())?;
    let main_base = disk_main_base_value(main_key).unwrap_or(0);

    let mut allowed_subs = disk_sub_stat_options(main_key);
    if allowed_subs.len() < 4 {
        return Err("Not enough valid substats for selected main stat".to_string());
    }
    allowed_subs.shuffle(rng);
    let keys: Vec<u32> = allowed_subs.into_iter().take(4).collect();

    let base: Vec<u32> = keys
        .iter()
        .map(|key| disk_sub_base_value(*key).unwrap_or(0))
        .collect();
    let mut add = vec![1u32; 4];
    let target_total = rng.gen_range(8..=9);
    while add.iter().sum::<u32>() < target_total {
        let candidates: Vec<usize> = add
            .iter()
            .enumerate()
            .filter_map(|(idx, value)| if *value < 6 { Some(idx) } else { None })
            .collect();
        if candidates.is_empty() {
            break;
        }
        let idx = *candidates
            .choose(rng)
            .ok_or_else(|| "Failed to choose substat proc target".to_string())?;
        add[idx] += 1;
    }

    let mut equip = ZValue::Object(vec![
        ("id".to_string(), ZValue::Number(item_id as i64)),
        ("level".to_string(), ZValue::Number(15)),
        ("exp".to_string(), ZValue::Number(0)),
        ("lock".to_string(), ZValue::Bool(false)),
        ("star".to_string(), ZValue::Number(1)),
        (
            "properties".to_string(),
            ZValue::Array(vec![ZValue::Object(vec![
                ("add_value".to_string(), ZValue::Number(0)),
                ("base_value".to_string(), ZValue::Number(main_base as i64)),
                ("key".to_string(), ZValue::Number(main_key as i64)),
            ])]),
        ),
        ("sub_properties".to_string(), ZValue::Array(Vec::new())),
    ]);

    zon_set_sub_properties(&mut equip, &keys, &base, &add);
    Ok(equip)
}

async fn apply_changes(headers: HeaderMap, original_uri: OriginalUri) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
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
    .to_string()
}

fn element_icon_path(element: &str) -> &'static str {
    match element.to_lowercase().as_str() {
        "ice" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconIce.webp",
        "fire" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconFire.webp",
        "electric" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconElectric.webp",
        "ether" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconEther.webp",
        "physical" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconPhysical.webp",
        "wind" => "/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/IconWind.webp",
        _ => "",
    }
}

fn element_label(element: &str) -> &'static str {
    match element.to_lowercase().as_str() {
        "ice" => "Ice",
        "fire" => "Fire",
        "electric" => "Electric",
        "ether" => "Ether",
        "physical" => "Physical",
        "wind" => "Wind",
        _ => "",
    }
}

fn boss_image_base_name(image_path: &str) -> String {
    let file_name = image_path
        .rsplit('/')
        .next()
        .unwrap_or(image_path)
        .trim();
    file_name
        .strip_suffix(".png")
        .or_else(|| file_name.strip_suffix(".webp"))
        .unwrap_or(file_name)
        .to_string()
}

fn format_with_commas(value: i64) -> String {
    let negative = value < 0;
    let digits = value.abs().to_string();
    let mut out = String::new();
    for (idx, ch) in digits.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    let mut formatted: String = out.chars().rev().collect();
    if negative {
        formatted.insert(0, '-');
    }
    formatted
}

fn format_stat_value(value: Option<f64>) -> String {
    value
        .map(|v| format_with_commas(v.round() as i64))
        .unwrap_or_else(|| "N/A".to_string())
}

fn clean_rich_text(text: &str) -> String {
    text.replace("<color=#FFAF2C>", "")
        .replace("<color=#FFFFFF>", "")
        .replace("<color=#2BAD00>", "")
        .replace("<color=#98EFF0>", "")
        .replace("<color=#2EB6FF>", "")
        .replace("</color>", "")
        .trim()
        .trim_start_matches('.')
        .trim()
        .to_string()
}

fn html_escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn render_rich_text(text: &str) -> String {
    let mut source = text.trim().trim_start_matches('.').trim().to_string();

    // Drop icon placeholders that have no renderer in this UI.
    while let Some(start) = source.find("<IconMap:") {
        if let Some(end_rel) = source[start..].find('>') {
            let end = start + end_rel + 1;
            source.replace_range(start..end, "");
        } else {
            break;
        }
    }

    let bytes = source.as_bytes();
    let mut out = String::new();
    let mut idx = 0usize;
    let mut open_spans = 0usize;

    while idx < bytes.len() {
        if source[idx..].starts_with("<color=#") {
            let tag_start = idx + "<color=#".len();
            if let Some(end_rel) = source[tag_start..].find('>') {
                let color = &source[tag_start..tag_start + end_rel];
                let valid = color.len() == 6 && color.chars().all(|c| c.is_ascii_hexdigit());
                if valid {
                    out.push_str(&format!("<span style=\"color: #{};\">", color));
                    open_spans += 1;
                    idx = tag_start + end_rel + 1;
                    continue;
                }
            }
        }

        if source[idx..].starts_with("</color>") {
            if open_spans > 0 {
                out.push_str("</span>");
                open_spans -= 1;
            }
            idx += "</color>".len();
            continue;
        }

        if let Some(ch) = source[idx..].chars().next() {
            if ch == '\n' {
                out.push_str("<br>");
            } else {
                out.push_str(&html_escape_text(&ch.to_string()));
            }
            idx += ch.len_utf8();
        } else {
            break;
        }
    }

    for _ in 0..open_spans {
        out.push_str("</span>");
    }

    out
}

fn da_total_hp_from_base(base_hp: f64) -> i64 {
    // Observed from DA data: total HP is a stable 8.74x of base HP.
    (base_hp * 8.74).round() as i64
}

fn shiyu_max_stage(shiyu_data: &JsonValue) -> u32 {
    shiyu_data
        .get("zone")
        .and_then(|z| z.as_object())
        .map(|zones| {
            zones
                .values()
                .filter_map(|zone| zone.get("stage_num").and_then(|v| v.as_u64()).map(|v| v as u32))
                .max()
                .unwrap_or(1)
        })
        .unwrap_or(1)
}

fn shiyu_stage_zones(shiyu_data: &JsonValue, floor: u32) -> Vec<(u32, JsonValue)> {
    let mut zones: Vec<(u32, JsonValue)> = shiyu_data
        .get("zone")
        .and_then(|z| z.as_object())
        .map(|zone_map| {
            zone_map
                .iter()
                .filter_map(|(zone_id, zone)| {
                    let stage_num = zone.get("stage_num").and_then(|v| v.as_u64()).map(|v| v as u32)?;
                    if stage_num != floor {
                        return None;
                    }
                    let zone_id = zone_id.parse::<u32>().ok()?;
                    Some((zone_id, zone.clone()))
                })
                .collect()
        })
        .unwrap_or_default();
    zones.sort_by_key(|(zone_id, _)| *zone_id);
    zones
}

fn shiyu_floor_boss_names(zones: &[(u32, JsonValue)]) -> Vec<String> {
    let mut names = Vec::new();
    for (_, zone) in zones {
        let Some(rooms) = zone.get("layer_room").and_then(|r| r.as_object()) else {
            continue;
        };
        for room in rooms.values() {
            let Some(monsters) = room.get("monster_list").and_then(|m| m.as_object()) else {
                continue;
            };
            let mut top_boss: Option<(&str, f64)> = None;
            for monster in monsters.values() {
                let name = monster.get("name").and_then(|v| v.as_str()).unwrap_or("Unknown");
                let hp = monster
                    .get("stats")
                    .and_then(|s| s.get("hp"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                if top_boss.map(|(_, best_hp)| hp > best_hp).unwrap_or(true) {
                    top_boss = Some((name, hp));
                }
            }
            if let Some((name, _)) = top_boss {
                names.push(name.to_string());
            }
        }
    }
    names
}

fn shiyu_render_monster_card(monster: &JsonValue, weakness: Option<&serde_json::Map<String, JsonValue>>) -> String {
    let empty_map = serde_json::Map::new();
    let boss_name = monster
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("Unknown");
    let boss_image = monster
        .get("image")
        .and_then(|img| img.as_str())
        .unwrap_or("");
    let image_html = if !boss_image.is_empty() {
        let base = boss_image_base_name(boss_image);
        let local_src = format!("/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/{}.webp", base);
        format!(
            r#"<img src="{}" alt="{}" style="width: 180px; height: 100%; object-fit: cover; background: #10141d; border-radius: 8px; flex-shrink: 0;" />"#,
            local_src, boss_name
        )
    } else {
        String::new()
    };

    let stats = monster.get("stats").and_then(|s| s.as_object()).unwrap_or(&empty_map);
    let element = monster.get("element").and_then(|e| e.as_object()).unwrap_or(&empty_map);
    let weakness = weakness.unwrap_or(&empty_map);

    let hp = format_stat_value(stats.get("hp").and_then(|h| h.as_f64()));
    let atk = format_stat_value(stats.get("attack").and_then(|a| a.as_f64()));
    let def = format_stat_value(stats.get("defence").and_then(|d| d.as_f64()));
    let stun = format_stat_value(stats.get("stun").and_then(|st| st.as_f64()));

    let weakness_badges: Vec<String> = weakness
        .iter()
        .filter_map(|(_, v)| {
            if let Some(elem) = v.as_str() {
                let icon_path = element_icon_path(elem);
                let label = element_label(elem);
                if icon_path.is_empty() || label.is_empty() {
                    None
                } else {
                    Some(format!(
                        r#"<span style="display:inline-flex; align-items:center; gap:6px; margin-right:10px; vertical-align:middle;"><img src="{}" alt="{}" title="{}" style="width: 18px; height: 18px; display:block;" />{}</span>"#,
                        icon_path, label, label, label
                    ))
                }
            } else {
                None
            }
        })
        .collect();
    let weakness_str = weakness_badges.join("");

    let resistance_badges: Vec<String> = element
        .iter()
        .filter_map(|(e, v)| {
            if v.as_i64() == Some(-1) {
                let icon_path = element_icon_path(e);
                let label = element_label(e);
                if icon_path.is_empty() || label.is_empty() {
                    None
                } else {
                    Some(format!(
                        r#"<span style="display:inline-flex; align-items:center; gap:6px; margin-right:10px; vertical-align:middle;"><img src="{}" alt="{}" title="{}" style="width: 18px; height: 18px; display:block;" />{}</span>"#,
                        icon_path, label, label, label
                    ))
                }
            } else {
                None
            }
        })
        .collect();
    let resistance_str = resistance_badges.join("");

    format!(
        r#"<div class="card" style="background: #1b1f2a; padding: 12px; border-radius: 12px; border: 1px solid #232a38; display: flex; gap: 14px; align-items: stretch; justify-content: space-between; flex-wrap: nowrap; min-height: 170px;">
            <div style="flex: 1 1 260px; min-width: 220px;">
                <h4 style="margin: 0 0 10px 0; font-size: 16px;">{boss_name}</h4>
                <div style="font-size: 13px; color: #c7d1e0; line-height: 1.45;">
                    <div style="margin-bottom: 5px;"><strong>HP:</strong> {hp}</div>
                    <div style="margin-bottom: 5px;"><strong>ATK:</strong> {atk}</div>
                    <div style="margin-bottom: 5px;"><strong>DEF:</strong> {def}</div>
                    <div style="margin-bottom: 5px;"><strong>Stun:</strong> {stun}</div>
                    {weakness_html}
                    {resistance_html}
                </div>
            </div>
            {image_html}
        </div>"#,
        boss_name = boss_name,
        hp = hp,
        atk = atk,
        def = def,
        stun = stun,
        weakness_html = if !weakness_str.is_empty() {
            format!("<div style=\"display:flex; align-items:center; gap:8px; margin-top:8px; margin-bottom:6px;\"><strong>Weakness:</strong> <span style=\"display:inline-flex; align-items:center; flex-wrap:wrap; gap:6px;\">{}</span></div>", weakness_str)
        } else {
            String::new()
        },
        resistance_html = if !resistance_str.is_empty() {
            format!("<div style=\"display:flex; align-items:center; gap:8px; margin-top:8px;\"><strong>Resistance:</strong> <span style=\"display:inline-flex; align-items:center; flex-wrap:wrap; gap:6px;\">{}</span></div>", resistance_str)
        } else {
            String::new()
        },
        image_html = image_html,
    )
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

fn render_equip_cards(state: &AppState, uid: u32, delete_mode: bool) -> String {
    let equip_dir = state.state_dir.join(format!("player/{uid}/equip"));
    let equip_templates = load_equip_templates(&state.asset_dir);
    let hakushin = load_hakushin_data(state);
    let equip_index = load_equip_template_index(&state.asset_dir);

    let mut cards_data = Vec::new();
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
            let sub_stats = equip
                .as_ref()
                .map(zon_get_sub_properties_list)
                .unwrap_or_default();
            let sub_stats_text = if sub_stats.is_empty() {
                "None".to_string()
            } else {
                sub_stats
                    .iter()
                    .map(|(key, _, procs)| format!("{} x{}", stat_label(state, *key), procs))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let card_html = if delete_mode {
                format!(
                    "<label class=\"card select-card\"><input type=\"checkbox\" name=\"equip_uids[]\" value=\"{uid}\" /><img class=\"thumb\" src=\"{img}\" alt=\"{name}\" /><span class=\"pill\">UID {uid}</span><h3>{name}</h3><div class=\"meta\">Set: {name}</div><div class=\"meta\">Slot: {slot}</div><div class=\"meta\">Level: {level}</div><div class=\"meta\">Main stat: {main_label}</div><div class=\"meta\">Sub stats: {sub_stats_text}</div></label>",
                    uid = equip_uid,
                    name = name,
                    level = level,
                    slot = slot,
                    main_label = main_label,
                    sub_stats_text = sub_stats_text,
                    img = img
                )
            } else {
                format!(
                    "<a class=\"card\" href=\"/equip/{uid}\"><img class=\"thumb\" src=\"{img}\" alt=\"{name}\" /><span class=\"pill\">UID {uid}</span><h3>{name}</h3><div class=\"meta\">Set: {name}</div><div class=\"meta\">Slot: {slot}</div><div class=\"meta\">Level: {level}</div><div class=\"meta\">Main stat: {main_label}</div><div class=\"meta\">Sub stats: {sub_stats_text}</div></a>",
                    uid = equip_uid,
                    name = name,
                    level = level,
                    slot = slot,
                    main_label = main_label,
                    sub_stats_text = sub_stats_text,
                    img = img
                )
            };
            cards_data.push((equip_item_id, equip_uid, card_html));
        }
    }

    cards_data.sort_by_key(|(equip_item_id, equip_uid, _)| (*equip_item_id, *equip_uid));
    let mut cards = String::new();
    for (_, _, card_html) in cards_data {
        cards.push_str(&card_html);
    }

    if cards.is_empty() {
        cards.push_str("<p class=\"meta\">No discs found for this account.</p>");
    }

    let add_panel = render_add_equip_panel(state, delete_mode);
    if delete_mode {
        let delete_panel = "<div class=\"panel\"><h3>Delete Mode</h3><div style=\"display:flex; gap:8px;\"><button class=\"danger\" type=\"submit\">Delete selected discs</button><a href=\"/dashboard?tab=discs\">Cancel</a></div></div>";
        format!(
            "{add_panel}<form method=\"post\" action=\"/equip/delete\" onsubmit=\"return confirm('Delete selected discs?');\">{delete_panel}<div class=\"cards\">{cards}</div></form>",
        )
    } else {
        format!("{add_panel}<div class=\"cards\">{cards}</div>")
    }
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

fn render_da_panel(state: &AppState, uid: u32) -> String {
    let dump_dir = &state.dump_dir;
    let boss_details_path = dump_dir.join("boss_details.json");
    let zone_info_path = state.asset_dir.join("ZoneInfoTemplateTb.json");
    
    // Load available DA zone IDs (starting with 69) from ZoneInfoTemplateTb.json
    // Keep full zone_id to support DA variants (e.g., 69038 and 690381 are separate)
    let mut available_zones = std::collections::HashSet::new();
    if let Ok(zone_content) = fs::read_to_string(&zone_info_path) {
        if let Ok(zone_data) = serde_json::from_str::<serde_json::Value>(&zone_content) {
            if let Some(data_array) = zone_data.get("data").and_then(|d| d.as_array()) {
                for entry in data_array {
                    if let Some(zone_id) = entry.get("zone_id").and_then(|z| z.as_u64()) {
                        let zone_id = zone_id as u32;
                        // Include all DA zones (base and variants like 69038 and 690381)
                        if zone_id >= 69000 && zone_id < 70000 {
                            available_zones.insert(zone_id);
                        }
                    }
                }
            }
        }
    }
    
    // Get currently selected DA zone (exact zone_id, supports variants like 69038 and 690381)
    let hadal_path = state.state_dir.join(format!("player/{uid}/hadal_zone/info"));
    let selected_da = if let Some(hadal_zon) = read_zon(&hadal_path) {
        if let Some(zone_id) = zon_get_entrance_zone_id(&hadal_zon, 9) {
            // Use exact zone_id to support DA variants (69038, 690381, etc.)
            zone_id
        } else {
            0
        }
    } else {
        0
    };
    
    let mut cards = String::new();
    
    if let Ok(content) = fs::read_to_string(&boss_details_path) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
            let mut da_entries: Vec<_> = data
                .as_object()
                .unwrap_or(&serde_json::Map::new())
                .iter()
                .filter_map(|(id_str, details)| {
                    let id = id_str.parse::<u32>().ok()?;
                    // Show DA entries when exact zone exists OR when a 6-digit hotfix variant
                    // maps to an available 5-digit base zone (e.g., 690381 -> 69038).
                    let id_str = id.to_string();
                    let base_id = if id_str.len() == 6 {
                        id / 10
                    } else {
                        id
                    };
                    if !available_zones.contains(&id) && !available_zones.contains(&base_id) {
                        return None;
                    }
                    
                    let name = details
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("Unknown");
                    
                    let mut boss_names = Vec::new();
                    if let Some(zones) = details.get("zone").and_then(|z| z.as_object()) {
                        for zone in zones.values() {
                            let zone_name = zone
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("")
                                .trim();
                            if !zone_name.is_empty() {
                                boss_names.push(zone_name.to_string());
                            } else if let Some(layer_room) = zone.get("layer_room").and_then(|r| r.as_object()) {
                                // Some DA entries have empty zone.name but still include full boss data in layer_room.
                                for room in layer_room.values() {
                                    if let Some(monster_list) = room.get("monster_list").and_then(|m| m.as_object()) {
                                        for monster in monster_list.values() {
                                            if let Some(monster_name) = monster.get("name").and_then(|n| n.as_str()) {
                                                if !monster_name.trim().is_empty() {
                                                    boss_names.push(monster_name.to_string());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Some((id, name.to_string(), boss_names))
                })
                .collect();
            
            // Sort: selected first, then by 5-digit base ID desc, then full ID desc.
            // Example: 69027 comes before 690271.
            da_entries.sort_by(|a, b| {
                let a_selected = a.0 == selected_da;
                let b_selected = b.0 == selected_da;
                if a_selected != b_selected {
                    return if a_selected { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
                }

                let a_base = if a.0 >= 100000 { a.0 / 10 } else { a.0 };
                let b_base = if b.0 >= 100000 { b.0 / 10 } else { b.0 };

                b_base
                    .cmp(&a_base)
                    .then_with(|| b.0.cmp(&a.0))
            });
            
            for (id, name, boss_names) in da_entries {
                let is_selected = id == selected_da;
                let style = if is_selected {
                    r#"style="text-decoration: none; color: inherit; border: 2px solid #4c7dff; background: rgba(76, 125, 255, 0.1);""#
                } else {
                    r#"style="text-decoration: none; color: inherit;""#
                };
                let boss_list = boss_names.join("<br>");
                let selected_mark = if is_selected { " ✓" } else { "" };
                cards.push_str(&format!(
                    r#"<a href="/da/{id}" class="card" {style}>
                        <h3>{name}{selected_mark}</h3>
                        <div class="meta">ID: {id}<br>{boss_list}</div>
                    </a>"#
                ));
            }
        }
    }
    
    if cards.is_empty() {
        cards.push_str("<p class=\"meta\">No Deadly Assault data available.</p>");
    }
    
    format!("<div class=\"cards\">{cards}</div>")
}

fn render_shiyu_panel(state: &AppState, uid: u32) -> String {
    let dump_dir = &state.dump_dir;
    let shiyu_details_path = dump_dir.join("shiyu_details.json");
    let zone_info_path = state.asset_dir.join("ZoneInfoTemplateTb.json");
    
    // Load available Shiyu zone IDs (starting with 62) from ZoneInfoTemplateTb.json
    let mut available_zones = std::collections::HashSet::new();
    if let Ok(zone_content) = fs::read_to_string(&zone_info_path) {
        if let Ok(zone_data) = serde_json::from_str::<serde_json::Value>(&zone_content) {
            if let Some(data_array) = zone_data.get("data").and_then(|d| d.as_array()) {
                for entry in data_array {
                    if let Some(zone_id) = entry.get("zone_id").and_then(|z| z.as_u64()) {
                        let zone_id = zone_id as u32;
                        let zone_id_str = zone_id.to_string();
                        let is_shiyu_5_digit = zone_id_str.len() == 5 && zone_id_str.starts_with("62");
                        let is_shiyu_6_digit = zone_id_str.len() == 6 && zone_id_str.starts_with("62");
                        if is_shiyu_5_digit || is_shiyu_6_digit {
                            available_zones.insert(zone_id);
                        }
                    }
                }
            }
        }
    }
    
    // Get currently selected Shiyu zone
    let hadal_path = state.state_dir.join(format!("player/{uid}/hadal_zone/info"));
    let selected_shiyu = if let Some(hadal_zon) = read_zon(&hadal_path) {
        zon_get_entrance_zone_id(&hadal_zon, 1).unwrap_or(0)
    } else {
        0
    };
    
    let mut cards = String::new();
    
    if let Ok(content) = fs::read_to_string(&shiyu_details_path) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
            let mut shiyu_entries: Vec<_> = data
                .as_object()
                .unwrap_or(&serde_json::Map::new())
                .iter()
                .filter_map(|(id_str, details)| {
                    let id = id_str.parse::<u32>().ok()?;
                    // Filter: only show Shiyu nodes that start with 62 and have a matching zone entry.
                    if !id_str.starts_with("62") || !available_zones.contains(&id) {
                        return None;
                    }
                    
                    let name = details
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("Unknown");

                    let max_stage = shiyu_max_stage(details);
                    let last_floor_zones = shiyu_stage_zones(details, max_stage);
                    let boss_names = shiyu_floor_boss_names(&last_floor_zones);
                    
                    Some((id, name.to_string(), boss_names))
                })
                .collect();
            
            // Sort: selected first, then by 5-digit base ID desc, then full ID desc.
            shiyu_entries.sort_by(|a, b| {
                let a_selected = a.0 == selected_shiyu;
                let b_selected = b.0 == selected_shiyu;
                if a_selected != b_selected {
                    return if a_selected { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
                }

                let a_base = if a.0 >= 100000 { a.0 / 10 } else { a.0 };
                let b_base = if b.0 >= 100000 { b.0 / 10 } else { b.0 };

                b_base
                    .cmp(&a_base)
                    .then_with(|| b.0.cmp(&a.0))
            });
            
            for (id, name, boss_names) in shiyu_entries {
                let is_selected = id == selected_shiyu;
                let style = if is_selected {
                    r#"style="text-decoration: none; color: inherit; border: 2px solid #4c7dff; background: rgba(76, 125, 255, 0.1);""#
                } else {
                    r#"style="text-decoration: none; color: inherit;""#
                };
                let selected_mark = if is_selected { " ✓" } else { "" };
                let boss_list = boss_names.join("<br>");
                cards.push_str(&format!(
                    r#"<a href="/shiyu/{id}" class="card" {style}>
                        <h3>{name}{selected_mark}</h3>
                        <div class="meta">ID: {id}<br>{boss_list}</div>
                    </a>"#
                ));
            }
        }
    }
    
    if cards.is_empty() {
        cards.push_str("<p class=\"meta\">No Shiyu data available.</p>");
    }
    
    format!("<div class=\"cards\">{cards}</div>")
}

async fn da_detail(
    State(state): State<AppState>,
    Path(id): Path<u32>,
) -> impl IntoResponse {
    let dump_dir = &state.dump_dir;
    let boss_details_path = dump_dir.join("boss_details.json");
    
    if let Ok(content) = fs::read_to_string(&boss_details_path) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(da_data) = data.get(id.to_string()) {
                let da_name = da_data
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("Unknown");
                
                let mut buff_cards = String::new();
                let mut boss_cards = String::new();
                let empty_map = serde_json::Map::new();
                
                if let Some(zones) = da_data.get("zone").and_then(|z| z.as_object()) {
                    for zone in zones.values() {
                        let selectable_buffs = zone
                            .get("selectable_buff")
                            .and_then(|b| b.as_object())
                            .unwrap_or(&empty_map);
                        
                        let layer_room = zone
                            .get("layer_room")
                            .and_then(|r| r.as_object())
                            .unwrap_or(&empty_map);

                        let layer_buffs = zone
                            .get("layer_buff")
                            .and_then(|b| b.as_object())
                            .unwrap_or(&empty_map);

                        let mut layer_buffs_html = String::new();
                        for buff in layer_buffs.values() {
                            let buff_desc = buff
                                .get("desc")
                                .and_then(|d| d.as_str())
                                .unwrap_or("");
                            if clean_rich_text(buff_desc).is_empty() {
                                continue;
                            }
                            layer_buffs_html.push_str(&format!(
                                r#"<div style="padding: 8px 10px; border-radius: 8px; background: #10141d; margin-top: 8px; border-left: 3px solid #4c7dff; font-size: 12px; color: #9aa4b2; line-height: 1.4;">{}</div>"#,
                                render_rich_text(buff_desc)
                            ));
                        }

                        let layer_buffs_section = if layer_buffs_html.is_empty() {
                            String::new()
                        } else {
                            format!(
                                r#"<div style="margin-top: 10px;">
                                    <div style="font-size: 12px; font-weight: 700; color: #8fb0ff; margin-bottom: 2px;">Layer Buffs</div>
                                    {}
                                </div>"#,
                                layer_buffs_html
                            )
                        };
                        
                        for room in layer_room.values() {
                            let monster_list = room
                                .get("monster_list")
                                .and_then(|m| m.as_object())
                                .unwrap_or(&empty_map);
                            
                            for monster in monster_list.values() {
                                let boss_name = monster
                                    .get("name")
                                    .and_then(|n| n.as_str())
                                    .unwrap_or("Unknown");
                                
                                let boss_image = monster
                                    .get("image")
                                    .and_then(|img| img.as_str())
                                    .unwrap_or("");
                                
                                let image_html = if !boss_image.is_empty() {
                                    let base = boss_image_base_name(boss_image);
                                    let local_src = format!("/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/{}.webp", base);
                                    format!(
                                        r#"<img src="{}" alt="{}" style="width: 220px; height: 100%; object-fit: cover; background: #10141d; border-radius: 8px; flex-shrink: 0;" />"#,
                                        local_src, boss_name
                                    )
                                } else {
                                    String::new()
                                };
                                
                                let stats = monster.get("stats").and_then(|s| s.as_object()).unwrap_or(&empty_map);
                                let element = monster.get("element").and_then(|e| e.as_object()).unwrap_or(&empty_map);
                                let _weakness = room.get("monster_weakness").and_then(|w| w.as_object()).unwrap_or(&empty_map);
                                
                                let base_hp_value = stats.get("hp").and_then(|h| h.as_f64()).unwrap_or(0.0);
                                let hp = format_with_commas(da_total_hp_from_base(base_hp_value));
                                let base_hp = format_with_commas(base_hp_value.round() as i64);
                                let atk = format_stat_value(stats.get("attack").and_then(|a| a.as_f64()));
                                let def = format_stat_value(stats.get("defence").and_then(|d| d.as_f64()));
                                let stun = format_stat_value(stats.get("stun").and_then(|st| st.as_f64()));
                                
                                // Correct field mapping: element == 1 means weakness, element == -1 means resistance.
                                let weakness_badges: Vec<String> = element
                                    .iter()
                                    .filter_map(|(e, v)| {
                                        if v.as_i64() == Some(1) {
                                            let icon_path = element_icon_path(e);
                                            let label = element_label(e);
                                            if icon_path.is_empty() || label.is_empty() {
                                                None
                                            } else {
                                                Some(format!(
                                                    r#"<span style="display:inline-flex; align-items:center; gap:6px; margin-right:10px; vertical-align:middle;"><img src="{}" alt="{}" title="{}" style="width: 18px; height: 18px; display:block;" />{}</span>"#,
                                                    icon_path, label, label, label
                                                ))
                                            }
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                let weakness_str = weakness_badges.join("");

                                let resistance_badges: Vec<String> = element
                                    .iter()
                                    .filter_map(|(e, v)| {
                                        if v.as_i64() == Some(-1) {
                                            let icon_path = element_icon_path(e);
                                            let label = element_label(e);
                                            if icon_path.is_empty() || label.is_empty() {
                                                None
                                            } else {
                                                Some(format!(
                                                    r#"<span style="display:inline-flex; align-items:center; gap:6px; margin-right:10px; vertical-align:middle;"><img src="{}" alt="{}" title="{}" style="width: 18px; height: 18px; display:block;" />{}</span>"#,
                                                    icon_path, label, label, label
                                                ))
                                            }
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                let resistance = resistance_badges.join("");
                                
                                boss_cards.push_str(&format!(
                                    r#"<div class="card" style="background: #1b1f2a; padding: 14px; border-radius: 12px; border: 1px solid #232a38; display: flex; gap: 16px; align-items: stretch; justify-content: space-between; flex-wrap: nowrap; min-height: 220px;">
                                        <div style="flex: 1 1 260px; min-width: 240px;">
                                            <h3>{boss_name}</h3>
                                            <div style="margin-top: 12px; font-size: 13px; color: #c7d1e0; line-height: 1.45;">
                                                <div style="margin-bottom: 6px;"><strong>HP:</strong> {hp}</div>
                                                <div style="margin-bottom: 6px;"><strong>Base HP:</strong> {base_hp}</div>
                                                <div style="margin-bottom: 6px;"><strong>ATK:</strong> {atk}</div>
                                                <div style="margin-bottom: 6px;"><strong>DEF:</strong> {def}</div>
                                                <div style="margin-bottom: 6px;"><strong>Stun:</strong> {stun}</div>
                                                {weakness_html}
                                                {resistance_html}
                                                {layer_buffs_section}
                                            </div>
                                        </div>
                                        {image_html}
                                    </div>"#,
                                    weakness_html = if !weakness_str.is_empty() {
                                        format!("<div style=\"display:flex; align-items:center; gap:8px; margin-top:8px; margin-bottom:6px;\"><strong>Weakness:</strong> <span style=\"display:inline-flex; align-items:center; flex-wrap:wrap; gap:6px;\">{}</span></div>", weakness_str)
                                    } else {
                                        String::new()
                                    },
                                    resistance_html = if !resistance.is_empty() {
                                        format!("<div style=\"display:flex; align-items:center; gap:8px; margin-top:8px;\"><strong>Resistance:</strong> <span style=\"display:inline-flex; align-items:center; flex-wrap:wrap; gap:6px;\">{}</span></div>", resistance)
                                    } else {
                                        String::new()
                                    },
                                    layer_buffs_section = layer_buffs_section
                                ));
                            }
                        }
                        
                        // Render selectable buffs (only once per DA, from first zone)
                        if buff_cards.is_empty() && !selectable_buffs.is_empty() {
                            let mut buffs_html = String::new();
                            for (_, buff) in selectable_buffs.iter().take(3) {
                                let buff_title = buff
                                    .get("title")
                                    .and_then(|t| t.as_str())
                                    .unwrap_or("Buff");
                                
                                let buff_desc = buff
                                    .get("desc")
                                    .and_then(|d| d.as_str())
                                    .unwrap_or("No description");
                                
                                // Remove color tags from description for better readability
                                let clean_desc = clean_rich_text(buff_desc);
                                let rich_desc = render_rich_text(buff_desc);
                                if buff_title.trim().is_empty() && clean_desc.is_empty() {
                                    continue;
                                }
                                let display_title = if buff_title.trim().is_empty() {
                                    "Buff"
                                } else {
                                    buff_title
                                };
                                
                                buffs_html.push_str(&format!(
                                    r#"<div style="margin-bottom: 12px; padding: 10px; background: #121620; border-radius: 8px; border-left: 3px solid #4c7dff;">
                                        <h4 style="margin: 0 0 6px 0; color: #4c7dff;">{}</h4>
                                        <p style="margin: 0; font-size: 12px; color: #9aa4b2; line-height: 1.4;">{}</p>
                                    </div>"#,
                                    display_title, rich_desc
                                ));
                            }

                            if !buffs_html.is_empty() {
                                buff_cards.push_str(&format!(
                                    r#"<div class="card" style="background: #1b1f2a; padding: 14px; border-radius: 12px; border: 1px solid #232a38; grid-column: span 1;">
                                        <h3>Selectable Buffs</h3>
                                        {}
                                    </div>"#,
                                    buffs_html
                                ));
                            }
                        }
                    }
                }
                
                let html = format!(
                    r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{} - Gear Editor</title>
  <style>
    body {{ font-family: system-ui, sans-serif; margin: 0; background: #0f1115; color: #e6e6e6; }}
    header {{ padding: 16px 24px; display: flex; justify-content: space-between; align-items: center; background: #151a24; }}
    .back {{ padding: 8px 12px; border-radius: 8px; background: #4c7dff; color: #fff; text-decoration: none; font-weight: 600; }}
    .container {{ padding: 20px 24px 40px; }}
    h1 {{ margin: 0 0 20px 0; font-size: 28px; }}
    .cards {{ display: grid; grid-template-columns: 1fr; gap: 16px; }}
    .card {{ background: #1b1f2a; padding: 16px; border-radius: 12px; border: 1px solid #232a38; }}
    .card h3 {{ margin: 0 0 12px 0; font-size: 18px; }}
    .meta {{ color: #9aa4b2; font-size: 12px; }}
  </style>
</head>
<body>
<header>
  <a href="/dashboard?tab=da" class="back">← Back to DA</a>
    <form method="post" action="/da/{}/select" style="margin: 0;">
        <button type="submit" style="padding: 10px 18px; background: #4c7dff; color: #fff; border: none; border-radius: 8px; font-weight: 600; cursor: pointer; font-size: 14px;">
            Select this Deadly Assault
        </button>
    </form>
</header>
<div class="container">
  <h1>{} #{}</h1>
  <div class="cards">
    {}
    {}
  </div>
</div>
</body>
</html>"#,
                                        da_name, id, da_name, id, buff_cards, boss_cards
                );
                
                return Html(html).into_response();
            }
        }
    }
    
    Html("<html><body><h1>DA not found</h1></body></html>".to_string()).into_response()
}

async fn shiyu_detail(
    State(state): State<AppState>,
    Path(id): Path<u32>,
    Query(query): Query<ShiyuDetailQuery>,
) -> impl IntoResponse {
    let dump_dir = &state.dump_dir;
    let shiyu_details_path = dump_dir.join("shiyu_details.json");
    
    if let Ok(content) = fs::read_to_string(&shiyu_details_path) {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(shiyu_data) = data.get(id.to_string()) {
                let shiyu_name = shiyu_data
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("Unknown");
                let max_stage = shiyu_max_stage(shiyu_data);
                let selected_floor = query.floor.unwrap_or(max_stage).clamp(1, max_stage);
                let floor_zones = shiyu_stage_zones(shiyu_data, selected_floor);
                let is_new_style = max_stage == 5;

                let tab_html = (1..=max_stage)
                    .map(|floor| {
                        let active = floor == selected_floor;
                        let href = format!("/shiyu/{id}?floor={floor}");
                        let style = if active {
                            r#"display:inline-flex; align-items:center; justify-content:center; min-width: 48px; padding: 8px 12px; border-radius: 10px; border: 2px solid #8fb0ff; text-decoration: none; font-weight: 800; letter-spacing: 0.2px; color: #ffffff; background: linear-gradient(180deg, #4c7dff 0%, #365fcc 100%); box-shadow: 0 0 0 2px rgba(143,176,255,0.25), 0 6px 16px rgba(0,0,0,0.35);"#
                        } else {
                            r#"display:inline-flex; align-items:center; justify-content:center; min-width: 48px; padding: 8px 12px; border-radius: 10px; border: 1px solid #2a3140; text-decoration: none; font-weight: 600; color: #d5dbea; background: #121620;"#
                        };
                        format!(
                            r#"<a href="{}" style="{}">Floor {}</a>"#,
                            href, style, floor
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("");

                let buff_zone = floor_zones
                    .iter()
                    .find(|(_, zone)| zone.get("layer_room").and_then(|r| r.as_object()).map(|rooms| rooms.is_empty()).unwrap_or(false))
                    .map(|(_, zone)| zone.clone())
                    .or_else(|| floor_zones.first().map(|(_, zone)| zone.clone()));

                let mut buff_cards = String::new();
                if !(is_new_style && selected_floor == 5) {
                    if let Some(zone) = buff_zone {
                    let selectable_buffs = zone
                        .get("layer_buff")
                        .and_then(|b| b.as_object())
                        .cloned()
                        .unwrap_or_default();
                    let mut buffs_html = String::new();
                    for (_, buff) in selectable_buffs.iter() {
                        let buff_title = buff
                            .get("title")
                            .and_then(|t| t.as_str())
                            .unwrap_or("Buff");
                        let buff_desc = buff
                            .get("desc")
                            .and_then(|d| d.as_str())
                            .unwrap_or("No description");
                        let clean_desc = clean_rich_text(buff_desc);
                        let rich_desc = render_rich_text(buff_desc);
                        if buff_title.trim().is_empty() && clean_desc.is_empty() {
                            continue;
                        }
                        let display_title = if buff_title.trim().is_empty() {
                            "Buff"
                        } else {
                            buff_title
                        };

                        buffs_html.push_str(&format!(
                            r#"<div style="margin-bottom: 12px; padding: 10px; background: #121620; border-radius: 8px; border-left: 3px solid #4c7dff;">
                                <h4 style="margin: 0 0 6px 0; color: #4c7dff;">{}</h4>
                                <p style="margin: 0; font-size: 12px; color: #9aa4b2; line-height: 1.4;">{}</p>
                            </div>"#,
                            display_title, rich_desc
                        ));
                    }

                    if !buffs_html.is_empty() {
                        buff_cards = format!(
                            r#"<div class="card" style="background: #1b1f2a; padding: 14px; border-radius: 12px; border: 1px solid #232a38;">
                                <h3>Buffs</h3>
                                {}
                            </div>"#,
                            buffs_html
                        );
                    }
                    }
                }

                let mut fight_cards = String::new();
                let mut fight_index = 1u32;
                let mut ordered_rooms: Vec<(u32, String, JsonValue)> = Vec::new();
                for (zone_id, zone) in &floor_zones {
                    if let Some(rooms) = zone.get("layer_room").and_then(|r| r.as_object()) {
                        let mut room_items: Vec<_> = rooms.iter().collect();
                        room_items.sort_by_key(|(rid, _)| rid.parse::<u32>().unwrap_or(0));
                        for (room_id, room) in room_items {
                            ordered_rooms.push((*zone_id, room_id.clone(), room.clone()));
                        }
                    }
                }

                for (zone_id, _room_id, room) in ordered_rooms {
                    let zone = floor_zones
                        .iter()
                        .find(|(id, _)| *id == zone_id)
                        .map(|(_, zone)| zone.clone())
                        .unwrap_or_else(|| JsonValue::Null);
                    let room_title = zone
                        .get("name")
                        .and_then(|n| n.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .or_else(|| room.get("name").and_then(|n| n.as_str()).filter(|s| !s.is_empty()).map(|s| s.to_string()))
                        .unwrap_or_else(|| format!("Fight {fight_index}"));
                    let waves_num = room.get("waves_num").and_then(|v| v.as_u64()).unwrap_or(0);
                    let room_weakness = room.get("monster_weakness").and_then(|w| w.as_object());

                    let mut monsters: Vec<_> = room
                        .get("monster_list")
                        .and_then(|m| m.as_object())
                        .map(|obj| obj.values().collect::<Vec<_>>())
                        .unwrap_or_default();
                    monsters.sort_by(|a, b| {
                        let a_hp = a.get("stats").and_then(|s| s.get("hp")).and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let b_hp = b.get("stats").and_then(|s| s.get("hp")).and_then(|v| v.as_f64()).unwrap_or(0.0);
                        b_hp.partial_cmp(&a_hp).unwrap_or(std::cmp::Ordering::Equal)
                    });

                    let mut monster_cards = String::new();
                    for monster in monsters {
                        monster_cards.push_str(&shiyu_render_monster_card(monster, room_weakness));
                    }

                    let room_buff_html = if is_new_style && selected_floor == 5 {
                        let buffs = zone
                            .get("layer_buff")
                            .and_then(|b| b.as_object())
                            .cloned()
                            .unwrap_or_default();
                        let mut html = String::new();
                        for (_, buff) in buffs.iter() {
                            let buff_title = buff.get("title").and_then(|v| v.as_str()).unwrap_or("Buff");
                            let buff_desc = buff.get("desc").and_then(|v| v.as_str()).unwrap_or("");
                            let clean_desc = clean_rich_text(buff_desc);
                            let rich_desc = render_rich_text(buff_desc);
                            if buff_title.trim().is_empty() && clean_desc.is_empty() {
                                continue;
                            }
                            let display_title = if buff_title.trim().is_empty() {
                                "Buff"
                            } else {
                                buff_title
                            };
                            html.push_str(&format!(
                                r#"<div style="padding: 8px 10px; border-radius: 8px; background: #10141d; margin-top: 8px; border-left: 3px solid #4c7dff;">
                                    <strong style="color: #4c7dff;">{}</strong>
                                    <div style="font-size: 12px; color: #9aa4b2; line-height: 1.4; margin-top: 4px;">{}</div>
                                </div>"#,
                                display_title, rich_desc
                            ));
                        }
                        html
                    } else {
                        String::new()
                    };

                    fight_cards.push_str(&format!(
                        r#"<div class="card" style="background: #1b1f2a; padding: 14px; border-radius: 12px; border: 1px solid #232a38;">
                            <h3 style="margin: 0 0 8px 0;">{room_title}</h3>
                            <div class="meta" style="margin-bottom: 12px;">Waves: {waves_num}</div>
                            {room_buff_html}
                            <div style="display: grid; gap: 12px; margin-top: 12px;">{monster_cards}</div>
                        </div>"#,
                    ));
                    fight_index += 1;
                }

                let html = format!(
                    r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{} - Gear Editor</title>
  <style>
    body {{ font-family: system-ui, sans-serif; margin: 0; background: #0f1115; color: #e6e6e6; }}
    header {{ padding: 16px 24px; display: flex; justify-content: space-between; align-items: center; background: #151a24; }}
    .back {{ padding: 8px 12px; border-radius: 8px; background: #4c7dff; color: #fff; text-decoration: none; font-weight: 600; }}
    .container {{ padding: 20px 24px 40px; }}
    h1 {{ margin: 0 0 20px 0; font-size: 28px; }}
    .floor-tabs {{ display: flex; flex-wrap: wrap; gap: 8px; margin: 0 0 20px 0; }}
    .section-title {{ margin: 0 0 12px 0; font-size: 18px; }}
  </style>
</head>
<body>
<header>
  <a href="/dashboard?tab=shiyu" class="back">← Back to Shiyu</a>
    <form method="post" action="/shiyu/{}/select" style="margin: 0;">
        <button type="submit" style="padding: 10px 18px; background: #4c7dff; color: #fff; border: none; border-radius: 8px; font-weight: 600; cursor: pointer; font-size: 14px;">
            Select this Shiyu
        </button>
    </form>
</header>
<div class="container">
  <h1>{} #{}</h1>
    <div class="floor-tabs">{}</div>
  <div class="cards">
        {}
        {}
  </div>
</div>
</body>
</html>"#,
                                                                                shiyu_name, id, shiyu_name, id, tab_html, buff_cards, fight_cards
                );
                
                return Html(html).into_response();
            }
        }
    }
    
    Html("<html><body><h1>Shiyu not found</h1></body></html>".to_string()).into_response()
}

async fn shiyu_select(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<u32>,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let uid = resolve_player_uid(&state, session.uid);
    let hadal_file = state.state_dir.join(format!("player/{uid}/hadal_zone/info"));

    let mut hadal_zon = match read_zon_verbose(&hadal_file) {
        Some(z) => z,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html("<h1>Failed to read hadal zone</h1>".to_string()),
            )
                .into_response()
        }
    };

    zon_set_entrance_zone_id(&mut hadal_zon, 1, id);
    let zon_content = format_zon_pretty(&zon_serialize(&hadal_zon));
    if let Err(err) = fs::write(&hadal_file, zon_content) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("Failed to save hadal zone: {}", err)),
        )
            .into_response();
    }

    Redirect::to("/dashboard?tab=shiyu").into_response()
}

async fn da_select(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<u32>,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    // Extract session and uid
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let uid = resolve_player_uid(&state, session.uid);

    // Read the hadal_zone/info file
    let hadal_file = state
        .state_dir
        .join(format!("player/{uid}/hadal_zone/info"));
    
    let mut hadal_zon = match read_zon_verbose(&hadal_file) {
        Some(z) => z,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html("<h1>Failed to read hadal zone</h1>".to_string()),
            )
                .into_response()
        }
    };

    // Use the zone_id directly (supports DA variants like 69038 and 690381)
    let zone_id_to_set = id;

    // Update the entrance (id=9) zone_id
    let mut modified = false;
    if let ZValue::Object(fields) = &mut hadal_zon {
        if let Some((_, ZValue::Array(entrances))) = fields.iter_mut().find(|(k, _)| k == "entrances") {
            for entry in entrances {
                if let ZValue::Object(items) = entry {
                    let mut is_target = false;
                    for (k, v) in items.iter() {
                        if k == "id" {
                            if let ZValue::Number(num) = v {
                                if *num as u32 == 9 {
                                    is_target = true;
                                }
                            }
                        }
                    }
                    if is_target {
                        // Update zone_id
                        if let Some((_, v)) = items.iter_mut().find(|(k, _)| k == "zone_id") {
                            *v = ZValue::Number(zone_id_to_set as i64);
                            modified = true;
                            break;
                        }
                    }
                }
            }
        }
    }

    if modified {
        // Apply immediately so DA selection does not require pressing "Apply Changes".
        let zon_content = format_zon_pretty(&zon_serialize(&hadal_zon));
        if let Err(err) = fs::write(&hadal_file, zon_content) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Html(format!("Failed to save hadal zone: {}", err)),
            )
                .into_response();
        }
    }

    Redirect::to("/dashboard?tab=da").into_response()
}

fn render_add_weapon_panel(state: &AppState) -> String {
        let _ = state;
        "<div class=\"panel\"><h3>Add Weapon</h3><a href=\"/weapon/new\">New weapon</a></div>"
                .to_string()
}

fn render_add_equip_panel(state: &AppState, delete_mode: bool) -> String {
        let _ = state;
    if delete_mode {
        "<div class=\"panel\"><h3>Discs</h3><div style=\"display:flex; gap:8px;\"><a href=\"/equip/new\">New disc</a><a href=\"/equip/generate\">Generate discs</a><a href=\"/dashboard?tab=discs\">Exit delete mode</a></div></div>"
            .to_string()
    } else {
        "<div class=\"panel\"><h3>Add Disc</h3><div style=\"display:flex; gap:8px;\"><a href=\"/equip/new\">New disc</a><a href=\"/equip/generate\">Generate discs</a><a href=\"/dashboard?tab=discs&delete=1\">Delete discs</a></div></div>"
            .to_string()
    }
}

fn render_generate_slot_options(selected: Option<u32>) -> String {
    let mut html = String::new();
    html.push_str(&format!(
        "<option value=\"\"{}>Not selected (random)</option>",
        if selected.is_none() { " selected" } else { "" }
    ));
    for slot in 1..=6 {
        html.push_str(&format!(
            "<option value=\"{}\"{}>Slot {}</option>",
            slot,
            if selected == Some(slot) { " selected" } else { "" },
            slot
        ));
    }
    html
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
        let file_name = path
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .split('?')
            .next()
            .unwrap_or(path)
            .trim();
        let stem = FsPath::new(file_name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(file_name);
        return format!("/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/{stem}.webp");
    }
    if !path.contains('/') && !path.contains('.') {
        return format!("/assets/zzz_dump/assets/static.nanoka.cc/zzz/UI/{path}.webp");
    }
    format!("/assets/{}", path.trim_start_matches('/'))
}

fn load_hakushin_data(state: &AppState) -> HakushinData {
    let fingerprint = hakushin_data_fingerprint(&state.dump_dir);
    let cache = HAKUSHIN_DATA.get_or_init(|| Mutex::new(None));
    let mut guard = cache.lock().unwrap();

    if let Some((cached_fingerprint, cached_data)) = guard.as_ref() {
        if *cached_fingerprint == fingerprint {
            return cached_data.clone();
        }
    }

    let data = HakushinData {
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
    };

    *guard = Some((fingerprint, data.clone()));
    data
}

fn hakushin_data_fingerprint(dump_dir: &FsPath) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for file_name in ["characters.json", "weapons.json", "drive_discs.json", "bangboos.json"] {
        let path = dump_dir.join(file_name);
        path.to_string_lossy().hash(&mut hasher);
        if let Ok(metadata) = fs::metadata(&path) {
            metadata.len().hash(&mut hasher);
            if let Ok(modified) = metadata.modified() {
                if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                    duration.as_secs().hash(&mut hasher);
                    duration.subsec_nanos().hash(&mut hasher);
                }
            }
        }
    }
    hasher.finish()
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
                if let Some(local_path) = normalize_image_reference(root_dir, value) {
                    image_local = Some(local_path);
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

fn normalize_image_reference(root_dir: &FsPath, value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.starts_with("/assets/") {
        return Some(trimmed.trim_start_matches('/').to_string());
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let file_name = trimmed
            .rsplit('/')
            .next()
            .unwrap_or(trimmed)
            .split('?')
            .next()
            .unwrap_or(trimmed)
            .trim();
        let stem = FsPath::new(file_name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or(file_name)
            .trim();
        if stem.is_empty() {
            return None;
        }
        let candidate = format!("zzz_dump/assets/static.nanoka.cc/zzz/UI/{stem}.webp");
        return root_dir.join(&candidate).exists().then_some(candidate);
    }

    if root_dir.join(trimmed).exists() {
        return Some(trimmed.to_string());
    }

    let file_name = FsPath::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(trimmed);
    let stem = FsPath::new(file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(file_name)
        .trim();
    if stem.is_empty() {
        return None;
    }

    let candidate = format!("zzz_dump/assets/static.nanoka.cc/zzz/UI/{stem}.webp");
    root_dir.join(&candidate).exists().then_some(candidate)
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

fn sanitize_next_path(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if !trimmed.starts_with('/') || trimmed.starts_with("//") {
        return None;
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return None;
    }
    Some(trimmed.to_string())
}

fn url_encode_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

fn html_escape_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn redirect_to_login(original_uri: &axum::http::Uri) -> Response {
    let attempted = original_uri
        .path_and_query()
        .map(|pq| pq.as_str())
        .unwrap_or("/dashboard");
    let next = sanitize_next_path(attempted).unwrap_or_else(|| "/dashboard".to_string());
    let location = format!("/?next={}", url_encode_component(&next));
    Redirect::to(&location).into_response()
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
        STAT_HP => {
            options.retain(|key| *key != STAT_HP);
        }
        STAT_ATK => {
            options.retain(|key| *key != STAT_ATK);
        }
        STAT_DEF => {
            options.retain(|key| *key != STAT_DEF);
        }
        STAT_HP_PCT => {
            options.retain(|key| *key != STAT_HP_PCT);
        }
        STAT_ATK_PCT => {
            options.retain(|key| *key != STAT_ATK_PCT);
        }
        STAT_DEF_PCT => {
            options.retain(|key| *key != STAT_DEF_PCT);
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

fn render_equip_substat_script(
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
    script.push_str("  const mainKey = Number(mainSelect?.value ?? 0);\n");
    script.push_str("  const subKeys = subOptionsByMain[mainKey] ?? [];\n");
    script.push_str("  const nextValues = [];\n");
    script.push_str("  for (const select of subSelects) {\n");
    script.push_str("    const current = Number(select.value);\n");
    script.push_str("    let chosen = current;\n");
    script.push_str("    if (!subKeys.includes(chosen) || nextValues.includes(chosen)) {\n");
    script.push_str("      chosen = subKeys.find((key) => !nextValues.includes(key));\n");
    script.push_str("      if (chosen === undefined) {\n");
    script.push_str("        chosen = subKeys[0] ?? 0;\n");
    script.push_str("      }\n");
    script.push_str("    }\n");
    script.push_str("    nextValues.push(chosen);\n");
    script.push_str("  }\n");
    script.push_str("  subSelects.forEach((select, idx) => {\n");
    script.push_str("    renderOptions(select, subKeys, nextValues[idx]);\n");
    script.push_str("  });\n");
    script.push_str("};\n\n");

    script.push_str("const updateMainOptions = () => {\n");
    script.push_str("  if (!mainSelect) {\n");
    script.push_str("    return;\n");
    script.push_str("  }\n");
    script.push_str("  if (slotSelect) {\n");
    script.push_str("    const slot = Number(slotSelect.value);\n");
    script.push_str("    const keys = mainOptionsBySlot[slot] ?? [];\n");
    script.push_str("    const current = Number(mainSelect.value);\n");
    script.push_str("    const selected = keys.includes(current) ? current : (keys[0] ?? 0);\n");
    script.push_str("    renderOptions(mainSelect, keys, selected);\n");
    script.push_str("  }\n");
    script.push_str("  updateSubOptions();\n");
    script.push_str("};\n\n");

    script.push_str("if (slotSelect) {\n");
    script.push_str("  slotSelect.addEventListener(\"change\", updateMainOptions);\n");
    script.push_str("}\n");
    script.push_str("if (mainSelect) {\n");
    script.push_str("  mainSelect.addEventListener(\"change\", updateSubOptions);\n");
    script.push_str("}\n");
    script.push_str("for (const select of subSelects) {\n");
    script.push_str("  select.addEventListener(\"change\", updateSubOptions);\n");
    script.push_str("}\n");
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
