use crate::{
    app_state::{AppState, state_with_active_server},
    auth::{get_session, get_session_mut, html_escape_attr, redirect_to_login, set_session},
    data::{
        hakushin::{load_hakushin_data, to_asset_url},
        templates::{
            EquipTemplateIndex, equip_set_id, equip_slot, force_disc_fourth_digit,
            load_equip_template_index, load_equip_templates, resolve_equip_item_id,
        },
    },
    domain::discs::{
        disk_main_base_value, disk_main_stat_options, disk_sub_base_value, disk_sub_stat_options,
        normalize_disk_main_stat, stat_label,
    },
    i18n::{Locale, locale_from_headers, t},
    player_state::{
        parse_slot_value, read_next_uid, render_equip_substat_script, render_slot_options,
        render_stat_select_options, render_sub_stat_rows, resolve_player_uid,
    },
    utils::{audit_log, shared_page_css, svg_data_uri},
    zon::{
        ZValue, format_zon_pretty, read_zon, zon_get_bool, zon_get_main_property, zon_get_number,
        zon_get_sub_properties_list, zon_serialize, zon_set_bool, zon_set_main_property,
        zon_set_number, zon_set_sub_properties,
    },
};
use axum::{
    extract::{Form, OriginalUri, Path, RawForm, State},
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Redirect, Response},
};
use rand::{Rng, seq::SliceRandom};
use serde::Deserialize;
use std::{collections::HashMap, fs};

#[derive(Deserialize)]
pub(crate) struct EquipUpdateForm {
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
    #[serde(default)]
    lock: Option<u8>,
}

#[derive(Deserialize)]
pub(crate) struct AddEquipForm {
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
pub(crate) struct GenerateEquipForm {
    equip_set_id: u32,
    slot: Option<String>,
    count: u32,
}

pub(crate) async fn equip_edit(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(equip_uid): Path<u32>,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&state, session.uid);
    let equip_path = state
        .state_dir
        .join(format!("player/{uid}/equip/{equip_uid}"));

    let locale = locale_from_headers(&headers);

    let Some(equip_zon) = read_zon(&equip_path) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "disc.not_found"))).into_response();
    };

    let level = zon_get_number(&equip_zon, "level").unwrap_or(0) as u32;
    let locked = zon_get_bool(&equip_zon, "lock").unwrap_or(false);
    let lock_checked = if locked { "checked" } else { "" };
    let equip_item_id = zon_get_number(&equip_zon, "id").unwrap_or(0) as u32;
    let hakushin = load_hakushin_data(&state, locale);
    let equip_index = load_equip_template_index(&state.asset_dir);
    let set_id = equip_set_id(equip_item_id, equip_index);
    let num_slot = equip_slot(equip_item_id, equip_index);
    let equip_name = hakushin
        .discs
        .get(&set_id)
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| format!("{} {equip_item_id}", t(locale, "fallback.disc")));
    let equip_img = hakushin
        .discs
        .get(&set_id)
        .and_then(|entry| entry.image_local.as_deref())
        .map(to_asset_url)
        .unwrap_or_else(|| svg_data_uri(&equip_name));
    let (main_key, _, _) = zon_get_main_property(&equip_zon);
    let sub_props = zon_get_sub_properties_list(&equip_zon);
    let main_options = disk_main_stat_options(num_slot);
    let normalized_main_key = normalize_disk_main_stat(num_slot, main_key)
        .unwrap_or_else(|| main_options.first().copied().unwrap_or(0));
    let sub_options = disk_sub_stat_options(normalized_main_key);
    let mut sub_options_by_main = HashMap::new();
    let mut label_map = HashMap::new();
    for slot_id in 1..=6 {
        let options = disk_main_stat_options(slot_id);
        for key in options {
            label_map
                .entry(key)
                .or_insert_with(|| stat_label(&state, locale, key));
            let sub_opts = disk_sub_stat_options(key);
            for sub_key in &sub_opts {
                label_map
                    .entry(*sub_key)
                    .or_insert_with(|| stat_label(&state, locale, *sub_key));
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
            <img src="{equip_img}" alt="{equip_name}" />
            <div>
                <h1>{edit_title} {equip_name}</h1>
                <div class="meta">{uid_label} {equip_uid} · {item_label} {equip_item_id} · {slot_label} {slot_num}</div>
            </div>
        </div>
    <form method="post">
      <label>{level_label}</label>
      <input name="level" type="number" min="0" value="{level}" />
      <label style="display:flex; align-items:center; gap:8px;"><input type="checkbox" name="lock" value="1" {lock_checked} style="width:auto;" /> {lock_toggle}</label>

            <h3>{main_stat_heading}</h3>
            <div class="row">
                <div>
                    <label>{stat_label_str}</label>
                    <select name="main_key" id="main_key">
                        {main_options}
                    </select>
                </div>
            </div>

            <h3>{sub_stats_heading}</h3>
            <div class="row">
                {sub_stat_rows}
            </div>
        {script}
      {warning}

      <button type="submit">{save_label}</button>
    </form>
  </div>
</body>
</html>"#,
        equip_uid = equip_uid,
        equip_item_id = equip_item_id,
        equip_name = html_escape_attr(&equip_name),
        equip_img = html_escape_attr(&equip_img),
        slot_num = num_slot,
        level = level,
        lock_checked = lock_checked,
        main_options =
            render_stat_select_options(&state, &main_options, normalized_main_key, locale),
        sub_stat_rows = render_sub_stat_rows(
            &state,
            &sub_props,
            &sub_options,
            normalized_main_key,
            locale
        ),
        script = script,
        warning = warning,
        edit_title = t(locale, "disc.edit"),
        uid_label = t(locale, "disc.uid"),
        item_label = t(locale, "disc.item"),
        slot_label = t(locale, "disc.slot"),
        level_label = t(locale, "disc.level"),
        lock_toggle = t(locale, "disc.lock_toggle"),
        main_stat_heading = t(locale, "disc.main_stat"),
        stat_label_str = t(locale, "disc.stat"),
        sub_stats_heading = t(locale, "disc.sub_stats"),
        save_label = t(locale, "disc.save"),
        shared_css = shared_page_css(),
        lang = locale.lang_attr(),
    );

    Html(body).into_response()
}

pub(crate) async fn equip_update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(equip_uid): Path<u32>,
    original_uri: OriginalUri,
    Form(payload): Form<EquipUpdateForm>,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&state, session.uid);
    let equip_path = state
        .state_dir
        .join(format!("player/{uid}/equip/{equip_uid}"));

    let locale = locale_from_headers(&headers);

    let Some(mut equip_zon) = read_zon(&equip_path) else {
        return (StatusCode::NOT_FOUND, Html(t(locale, "disc.not_found"))).into_response();
    };

    if zon_get_number(&equip_zon, "star").is_none() {
        zon_set_number(&mut equip_zon, "star", 1);
    }
    zon_set_number(&mut equip_zon, "level", payload.level as i64);
    let locked = payload.lock.unwrap_or(0) == 1;
    zon_set_bool(&mut equip_zon, "lock", locked);
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

pub(crate) async fn equip_new(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, _session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state_with_active_server(&state, &headers);
    let locale = locale_from_headers(&headers);

    let options = render_disc_select_options(&state, 0, locale);
    let disc_images: HashMap<u32, String> = {
        let h = load_hakushin_data(&state, locale);
        h.discs
            .iter()
            .map(|(id, entry)| {
                let url = entry
                    .image_local
                    .as_deref()
                    .map(to_asset_url)
                    .unwrap_or_else(|| svg_data_uri(&entry.name));
                (*id, url)
            })
            .collect()
    };
    let disc_images_json = serde_json::to_string(&disc_images).unwrap_or_else(|_| "{}".to_string());
    let slot_options = render_slot_options(locale, 1);
    let main_options = render_stat_select_options(&state, &disk_main_stat_options(1), 0, locale);
    let sub_options = disk_sub_stat_options(0);
    let sub_stat_rows = render_sub_stat_rows(&state, &[], &sub_options, 0, locale);

    let mut main_options_by_slot = HashMap::new();
    let mut sub_options_by_main = HashMap::new();
    let mut label_map = HashMap::new();
    for slot in 1..=6 {
        let options = disk_main_stat_options(slot);
        for key in &options {
            label_map
                .entry(*key)
                .or_insert_with(|| stat_label(&state, locale, *key));
            let sub_opts = disk_sub_stat_options(*key);
            for sub_key in &sub_opts {
                label_map
                    .entry(*sub_key)
                    .or_insert_with(|| stat_label(&state, locale, *sub_key));
            }
            sub_options_by_main.insert(*key, sub_opts);
        }
        main_options_by_slot.insert(slot, options);
    }
    let main_options_by_slot_json =
        serde_json::to_string(&main_options_by_slot).unwrap_or_default();
    let sub_options_by_main_json = serde_json::to_string(&sub_options_by_main).unwrap_or_default();
    let label_map_json = serde_json::to_string(&label_map).unwrap_or_default();
    let script = render_equip_substat_script(
        &main_options_by_slot_json,
        &sub_options_by_main_json,
        &label_map_json,
    );

    let new_title = t(locale, "disc.new");
    let disc_set_label = t(locale, "disc.set");
    let slot_label = t(locale, "disc.slot");
    let stat_label_str = t(locale, "disc.stat");
    let main_stat_heading = t(locale, "disc.main_stat");
    let sub_stats_heading = t(locale, "disc.sub_stats");
    let create_label = t(locale, "disc.create");
    let lang = locale.lang_attr();

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
        <form method="post">
            <div>
                <img id="disc_preview" class="preview-img" />
                <label>{disc_set_label}</label>
                <select name="equip_set_id" id="equip_set_id" required>
                    {options}
                </select>
            </div>
            <div class="row">
                <div>
                    <label>{slot_label}</label>
                    <select name="equip_slot" id="equip_slot">
                        {slot_options}
                    </select>
                </div>
            </div>

            <h3>{main_stat_heading}</h3>
            <div class="row">
                <div>
                    <label>{stat_label_str}</label>
                    <select name="main_key" id="main_key">
                        {main_options}
                    </select>
                </div>
            </div>

            <h3>{sub_stats_heading}</h3>
            <div class="row">
                {sub_stat_rows}
            </div>
            {script}
            <button>{create_label}</button>
        </form>
    </div>
    <script>
    var d = {disc_images_json};
    var p = document.getElementById("disc_preview");
    var s = document.getElementById("equip_set_id");
    s.addEventListener("change", function() {{
        var u = d[s.value];
        if (u) {{ p.src = u; p.style.display = "block"; }}
        else {{ p.style.display = "none"; }}
    }});
    </script>
</body>
</html>"#,
        options = options,
        slot_options = slot_options,
        main_options = main_options,
        sub_stat_rows = sub_stat_rows,
        script = script,
        disc_images_json = disc_images_json,
        new_title = new_title,
        disc_set_label = disc_set_label,
        slot_label = slot_label,
        stat_label_str = stat_label_str,
        main_stat_heading = main_stat_heading,
        sub_stats_heading = sub_stats_heading,
        create_label = create_label,
        shared_css = shared_page_css(),
        lang = lang,
    );

    Html(body).into_response()
}

pub(crate) async fn equip_add(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
    Form(payload): Form<AddEquipForm>,
) -> impl IntoResponse {
    let Some((session_id, session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state_with_active_server(&state, &headers);
    let locale = locale_from_headers(&headers);
    let uid = resolve_player_uid(&state, session.uid);
    let equip_dir = state.state_dir.join(format!("player/{uid}/equip"));
    let next_uid = read_next_uid(&equip_dir).unwrap_or(1);
    let new_uid = next_uid.max(1);
    let equip_index = load_equip_template_index(&state.asset_dir);
    let Some(item_id) =
        resolve_equip_item_id(payload.equip_set_id, payload.equip_slot, equip_index)
    else {
        return (
            StatusCode::BAD_REQUEST,
            Html(t(locale, "disc.invalid_set_slot")),
        )
            .into_response();
    };
    let item_id = force_disc_fourth_digit(item_id);

    let main_key =
        normalize_disk_main_stat(payload.equip_slot, payload.main_key).unwrap_or_else(|| {
            disk_main_stat_options(payload.equip_slot)
                .first()
                .copied()
                .unwrap_or(0)
        });
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
            Html(format!("{}: {}", t(locale, "disc.failed_create"), err)),
        )
            .into_response();
    }

    let next_path = equip_dir.join("next");
    if let Err(err) = fs::write(&next_path, format!("{}\n", new_uid + 1)) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("{}: {}", t(locale, "disc.failed_counter"), err)),
        )
            .into_response();
    }

    audit_log(&state.root_dir, &session.username, session.uid, "equip_add", &format!("created disc {}", new_uid));
    set_session(session_id, session);
    Redirect::to("/dashboard?tab=discs").into_response()
}

pub(crate) async fn equip_generate(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((_session_id, _session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state_with_active_server(&state, &headers);
    let locale = locale_from_headers(&headers);

    let options = render_disc_select_options(&state, 0, locale);
    let disc_images: HashMap<u32, String> = {
        let h = load_hakushin_data(&state, locale);
        h.discs
            .iter()
            .map(|(id, entry)| {
                let url = entry
                    .image_local
                    .as_deref()
                    .map(to_asset_url)
                    .unwrap_or_else(|| svg_data_uri(&entry.name));
                (*id, url)
            })
            .collect()
    };
    let disc_images_json = serde_json::to_string(&disc_images).unwrap_or_else(|_| "{}".to_string());
    let gen_title = t(locale, "disc.generate");
    let gen_desc = t(locale, "disc.generate_desc");
    let disc_set_label = t(locale, "disc.set");
    let slot_label = t(locale, "disc.slot");
    let count_label = t(locale, "disc.count");
    let gen_btn = t(locale, "disc.generate_btn");
    let lang = locale.lang_attr();
    let slot_options = render_generate_slot_options(None, locale);
    let body = format!(
        r#"<!doctype html>
<html lang="{lang}">
<head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{gen_title}</title>
    <style>{shared_css}</style>
</head>
<body>
    <div class="container">
        <h1>{gen_title}</h1>
        <div class="meta">{gen_desc}</div>
        <form method="post">
            <div>
                <img id="disc_preview" class="preview-img" />
                <label>{disc_set_label}</label>
                <select name="equip_set_id" id="equip_set_id" required>
                    {options}
                </select>
            </div>
            <div class="row">
                <div>
                    <label>{slot_label}</label>
                    <select name="slot">
                        {slot_options}
                    </select>
                </div>
                <div>
                    <label>{count_label}</label>
                    <input name="count" type="number" min="1" max="200" value="10" required />
                </div>
            </div>
            <button>{gen_btn}</button>
        </form>
    </div>
    <script>
    var d = {disc_images_json};
    var p = document.getElementById("disc_preview");
    var s = document.getElementById("equip_set_id");
    s.addEventListener("change", function() {{
        var u = d[s.value];
        if (u) {{ p.src = u; p.style.display = "block"; }}
        else {{ p.style.display = "none"; }}
    }});
    </script>
</body>
</html>"#,
        options = options,
        slot_options = slot_options,
        gen_title = gen_title,
        gen_desc = gen_desc,
        disc_images_json = disc_images_json,
        disc_set_label = disc_set_label,
        slot_label = slot_label,
        count_label = count_label,
        gen_btn = gen_btn,
        shared_css = shared_page_css(),
        lang = lang,
    );

    Html(body).into_response()
}

pub(crate) async fn equip_generate_submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
    Form(payload): Form<GenerateEquipForm>,
) -> impl IntoResponse {
    let Some((session_id, session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let locale = locale_from_headers(&headers);

    if payload.count == 0 || payload.count > 200 {
        return (StatusCode::BAD_REQUEST, Html(t(locale, "disc.count_range"))).into_response();
    }
    let selected_slot = payload.slot.as_deref().map(parse_slot_value).unwrap_or(0);
    if selected_slot > 6 {
        return (StatusCode::BAD_REQUEST, Html(t(locale, "disc.slot_range"))).into_response();
    }

    let state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&state, session.uid);
    let equip_dir = state.state_dir.join(format!("player/{uid}/equip"));
    let equip_index = load_equip_template_index(&state.asset_dir);
    let mut next_uid = read_next_uid(&equip_dir).unwrap_or(1).max(1);
    let mut rng = rand::thread_rng();

    for _ in 0..payload.count {
        let equip = match generate_random_disc(
            payload.equip_set_id,
            selected_slot,
            equip_index,
            &mut rng,
            locale,
        ) {
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
                Html(format!("{}: {}", t(locale, "disc.failed_create_gen"), err)),
            )
                .into_response();
        }

        next_uid += 1;
    }

    let next_path = equip_dir.join("next");
    if let Err(err) = fs::write(&next_path, format!("{}\n", next_uid)) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Html(format!("{}: {}", t(locale, "disc.failed_counter"), err)),
        )
            .into_response();
    }

    audit_log(&state.root_dir, &session.username, session.uid, "equip_generate", &format!("generated {} discs", payload.count));
    set_session(session_id, session);
    Redirect::to("/dashboard?tab=discs").into_response()
}

pub(crate) async fn equip_delete_submit(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
    RawForm(raw_form): RawForm,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&state, session.uid);
    let raw_form_text = String::from_utf8_lossy(&raw_form).into_owned();
    let selected = parse_selected_equip_uids(&raw_form_text);

    let mut deleted = Vec::new();

    for equip_uid in selected {
        let primary_path = state
            .state_dir
            .join(format!("player/{uid}/equip/{equip_uid}"));

        let equip = read_zon(&primary_path);
        let locked = equip
            .as_ref()
            .and_then(|v| zon_get_bool(v, "lock"))
            .unwrap_or(false);
        if locked {
            continue;
        }

        deleted.push(equip_uid);

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

    audit_log(&state.root_dir, &session.username, session.uid, "equip_delete", &format!("deleted {} discs", deleted.len()));
    set_session(session_id, session);
    Redirect::to("/dashboard?tab=discs").into_response()
}

pub(crate) async fn equip_delete_all_unlocked(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session_mut(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&state, session.uid);
    let equip_dir = state.state_dir.join(format!("player/{uid}/equip"));

    let mut deleted = 0u32;

    if let Ok(entries) = fs::read_dir(&equip_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let equip = read_zon(&path);
            let locked = equip
                .as_ref()
                .and_then(|v| zon_get_bool(v, "lock"))
                .unwrap_or(false);
            if !locked {
                if path.exists() {
                    let _ = fs::remove_file(&path);
                }
                let zon_path = path.with_extension("zon");
                if zon_path.exists() {
                    let _ = fs::remove_file(&zon_path);
                }
                session.pending_writes.remove(&path);
                session.pending_writes.remove(&zon_path);
                deleted += 1;
            }
        }
    }

    audit_log(&state.root_dir, &session.username, session.uid, "equip_delete_all_unlocked", &format!("deleted {} unlocked discs", deleted));
    set_session(session_id, session);
    Redirect::to("/dashboard?tab=discs").into_response()
}

pub(crate) async fn equip_lock_selected(
    State(state): State<AppState>,
    headers: HeaderMap,
    original_uri: OriginalUri,
    RawForm(raw_form): RawForm,
) -> impl IntoResponse {
    equip_set_lock(state, headers, original_uri, &raw_form)
}

fn equip_set_lock(
    state: AppState,
    headers: HeaderMap,
    original_uri: OriginalUri,
    raw_form: &[u8],
) -> Response {
    let Some((_session_id, session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let state = state_with_active_server(&state, &headers);
    let uid = resolve_player_uid(&state, session.uid);
    let raw_form_text = String::from_utf8_lossy(raw_form).into_owned();
    let selected: std::collections::HashSet<u32> = parse_selected_equip_uids(&raw_form_text)
        .into_iter()
        .collect();
    let equip_dir = state.state_dir.join(format!("player/{uid}/equip"));

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

            let should_lock = selected.contains(&equip_uid);

            if let Some(mut equip) = read_zon(&entry.path()) {
                zon_set_bool(&mut equip, "lock", should_lock);
                let content = format_zon_pretty(&zon_serialize(&equip));
                let _ = fs::write(&entry.path(), content);
            }
        }
    }

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
    locale: Locale,
) -> Result<ZValue, String> {
    let mut slots = Vec::new();
    for slot in 1..=6 {
        if resolve_equip_item_id(set_id, slot, equip_index).is_some() {
            slots.push(slot);
        }
    }
    if slots.is_empty() {
        return Err(t(locale, "disc.invalid_set").to_string());
    }

    let slot = if selected_slot == 0 {
        *slots
            .choose(rng)
            .ok_or_else(|| t(locale, "disc.no_slots").to_string())?
    } else {
        if !(1..=6).contains(&selected_slot) {
            return Err(t(locale, "disc.slot_range").to_string());
        }
        if !slots.contains(&selected_slot) {
            return Err(t(locale, "disc.slot_not_available").to_string());
        }
        selected_slot
    };
    let item_id = resolve_equip_item_id(set_id, slot, equip_index)
        .map(force_disc_fourth_digit)
        .ok_or_else(|| t(locale, "disc.invalid_set_slot").to_string())?;

    let main_options = disk_main_stat_options(slot);
    let main_key = *main_options
        .choose(rng)
        .ok_or_else(|| t(locale, "disc.no_valid_main").to_string())?;
    let main_base = disk_main_base_value(main_key).unwrap_or(0);

    let mut allowed_subs = disk_sub_stat_options(main_key);
    if allowed_subs.len() < 4 {
        return Err(t(locale, "disc.not_enough_substats").to_string());
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
            .ok_or_else(|| t(locale, "disc.choose_failed").to_string())?;
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

pub(crate) fn render_equip_cards(
    state: &AppState,
    uid: u32,
    delete_mode: bool,
    lock_mode: bool,
    locale: Locale,
    filter_set_id: Option<u32>,
    filter_slot: Option<u32>,
    filter_main_stat: Option<u32>,
    filter_status: Option<&str>,
    page: u32,
) -> String {
    let equip_dir = state.state_dir.join(format!("player/{uid}/equip"));
    let equip_templates = load_equip_templates(&state.asset_dir);
    let hakushin = load_hakushin_data(state, locale);
    let equip_index = load_equip_template_index(&state.asset_dir);

    let uid_label = t(locale, "disc.uid");
    let set_label = t(locale, "disc.set");
    let slot_label = t(locale, "disc.slot");
    let level_label = t(locale, "disc.level");
    let main_stat_label = t(locale, "disc.main_stat_label");
    let sub_stats_lbl = t(locale, "disc.sub_stats_label");
    let none_str = t(locale, "disc.none");
    let fallback_disc = t(locale, "fallback.disc");

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
                .unwrap_or_else(|| format!("{fallback_disc} {equip_item_id}"));

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
            let main_label = stat_label(state, locale, main_stat.0);
            let sub_stats = equip
                .as_ref()
                .map(zon_get_sub_properties_list)
                .unwrap_or_default();
            let locked = equip
                .as_ref()
                .and_then(|v| zon_get_bool(v, "lock"))
                .unwrap_or(false);
            let lock_icon = if locked { " \u{1f512}" } else { "" };

            let sub_stats_text = if sub_stats.is_empty() {
                none_str.to_string()
            } else {
                sub_stats
                    .iter()
                    .map(|(key, _, procs)| {
                        format!("{} x{}", stat_label(state, locale, *key), procs)
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let card_html = if delete_mode {
                if locked {
                    format!(
                        "<label class=\"card select-card locked\"><input type=\"checkbox\" disabled /><img class=\"thumb\" src=\"{img}\" alt=\"{name}\" /><span class=\"pill\">{uid_label} {uid} \u{1f512}</span><h3>{name}</h3><div class=\"meta\">{set_label}: {name}</div><div class=\"meta\">{slot_label}: {slot}</div><div class=\"meta\">{level_label}: {level}</div><div class=\"meta\">{main_stat_label}: {main_label}</div><div class=\"meta\">{sub_stats_lbl}: {sub_stats_text}</div></label>",
                        uid = equip_uid,
                        name = html_escape_attr(&name),
                        level = level,
                        slot = slot,
                        main_label = main_label,
                        sub_stats_text = sub_stats_text,
                        img = html_escape_attr(&img)
                    )
                } else {
                    format!(
                        "<label class=\"card select-card\"><input type=\"checkbox\" name=\"equip_uids[]\" value=\"{uid}\" /><img class=\"thumb\" src=\"{img}\" alt=\"{name}\" /><span class=\"pill\">{uid_label} {uid}</span><h3>{name}</h3><div class=\"meta\">{set_label}: {name}</div><div class=\"meta\">{slot_label}: {slot}</div><div class=\"meta\">{level_label}: {level}</div><div class=\"meta\">{main_stat_label}: {main_label}</div><div class=\"meta\">{sub_stats_lbl}: {sub_stats_text}</div></label>",
                        uid = equip_uid,
                        name = html_escape_attr(&name),
                        level = level,
                        slot = slot,
                        main_label = main_label,
                        sub_stats_text = sub_stats_text,
                        img = html_escape_attr(&img)
                    )
                }
            } else if lock_mode {
                if locked {
                    format!(
                        "<label class=\"card select-card\"><input type=\"checkbox\" name=\"equip_uids[]\" value=\"{uid}\" checked /><img class=\"thumb\" src=\"{img}\" alt=\"{name}\" /><span class=\"pill\">{uid_label} {uid} \u{1f512}</span><h3>{name}</h3><div class=\"meta\">{set_label}: {name}</div><div class=\"meta\">{slot_label}: {slot}</div><div class=\"meta\">{level_label}: {level}</div><div class=\"meta\">{main_stat_label}: {main_label}</div><div class=\"meta\">{sub_stats_lbl}: {sub_stats_text}</div></label>",
                        uid = equip_uid,
                        name = html_escape_attr(&name),
                        level = level,
                        slot = slot,
                        main_label = main_label,
                        sub_stats_text = sub_stats_text,
                        img = html_escape_attr(&img)
                    )
                } else {
                    format!(
                        "<label class=\"card select-card\"><input type=\"checkbox\" name=\"equip_uids[]\" value=\"{uid}\" /><img class=\"thumb\" src=\"{img}\" alt=\"{name}\" /><span class=\"pill\">{uid_label} {uid}</span><h3>{name}</h3><div class=\"meta\">{set_label}: {name}</div><div class=\"meta\">{slot_label}: {slot}</div><div class=\"meta\">{level_label}: {level}</div><div class=\"meta\">{main_stat_label}: {main_label}</div><div class=\"meta\">{sub_stats_lbl}: {sub_stats_text}</div></label>",
                        uid = equip_uid,
                        name = html_escape_attr(&name),
                        level = level,
                        slot = slot,
                        main_label = main_label,
                        sub_stats_text = sub_stats_text,
                        img = html_escape_attr(&img)
                    )
                }
            } else {
                format!(
                    "<a class=\"card\" href=\"/equip/{uid}\"><img class=\"thumb\" src=\"{img}\" alt=\"{name}\" /><span class=\"pill\">{uid_label} {uid}{lock_icon}</span><h3>{name}</h3><div class=\"meta\">{set_label}: {name}</div><div class=\"meta\">{slot_label}: {slot}</div><div class=\"meta\">{level_label}: {level}</div><div class=\"meta\">{main_stat_label}: {main_label}</div><div class=\"meta\">{sub_stats_lbl}: {sub_stats_text}</div></a>",
                    uid = equip_uid,
                    name = html_escape_attr(&name),
                    level = level,
                    slot = slot,
                    main_label = main_label,
                    sub_stats_text = sub_stats_text,
                    img = html_escape_attr(&img),
                    lock_icon = lock_icon
                )
            };
            cards_data.push((equip_item_id, equip_uid, card_html, set_id, slot, main_stat.0, locked));
        }
    }

    cards_data.retain(|(_, _, _, set_id, slot, main_stat_key, locked)| {
        if let Some(fid) = filter_set_id {
            if *set_id != fid {
                return false;
            }
        }
        if let Some(fs) = filter_slot {
            if *slot != fs {
                return false;
            }
        }
        if let Some(fm) = filter_main_stat {
            if *main_stat_key != fm {
                return false;
            }
        }
        match filter_status {
            Some("locked") if !locked => return false,
            Some("unlocked") if *locked => return false,
            _ => {}
        }
        true
    });

    cards_data.sort_by_key(|(equip_item_id, equip_uid, _, _, _, _, _)| (*equip_item_id, *equip_uid));
    let total = cards_data.len();

    let per_page: usize = 50;
    let total_pages = if total == 0 { 1 } else { (total + per_page - 1) / per_page };
    let page = page.clamp(1, total_pages as u32);
    let start = ((page - 1) as usize) * per_page;
    let end = total.min(start + per_page);
    let page_cards: Vec<_> = cards_data.drain(start..end).collect();

    let mut cards = String::new();
    for (_, _, card_html, _, _, _, _) in page_cards {
        cards.push_str(&card_html);
    }

    let filter_panel = if !delete_mode && !lock_mode { render_disc_filter_panel(state, locale, filter_set_id, filter_slot, filter_main_stat, filter_status) } else { String::new() };
    let pagination_html = if total_pages > 1 { render_pagination(locale, page, total_pages as u32, filter_set_id, filter_slot, filter_main_stat, filter_status, total) } else { String::new() };

    if cards.is_empty() && total == 0 {
        cards.push_str(&format!(
            "<p class=\"meta\">{}</p>",
            t(locale, "disc.no_discs")
        ));
    }

    let add_panel = render_add_equip_panel(state, delete_mode, lock_mode, locale);
    if delete_mode {
        let delete_panel = format!(
            "<div class=\"panel\"><h3>{}</h3><div style=\"display:flex; gap:8px; flex-wrap:wrap;\"><button class=\"danger\" type=\"submit\">{}</button><button class=\"danger\" type=\"submit\" formaction=\"/equip/delete-all-unlocked\" onclick=\"return confirm('{}');\">{}</button><a href=\"/dashboard?tab=discs\">{}</a></div></div>",
            t(locale, "disc.delete_mode"),
            t(locale, "disc.delete_selected"),
            t(locale, "disc.delete_all_unlocked"),
            t(locale, "disc.delete_all_unlocked"),
            t(locale, "disc.cancel"),
        );
        format!(
            "{add_panel}<form class=\"delete-form\" method=\"post\" action=\"/equip/delete\" onsubmit=\"return confirm('{}');\">{delete_panel}<div class=\"cards\">{cards}</div></form>{pagination_html}",
            t(locale, "disc.delete_selected"),
            pagination_html = pagination_html,
        )
    } else if lock_mode {
        let lock_panel = format!(
            "<div class=\"panel\"><h3>{}</h3><div style=\"display:flex; gap:8px; flex-wrap:wrap;\"><button type=\"submit\" formaction=\"/equip/lock-selected\">{}</button><a href=\"/dashboard?tab=discs\">{}</a></div></div>",
            t(locale, "disc.lock_mode"),
            t(locale, "disc.lock_selected"),
            t(locale, "disc.cancel"),
        );
        format!(
            "{add_panel}<form class=\"lock-form\" method=\"post\">{lock_panel}<div class=\"cards\">{cards}</div></form>{pagination_html}",
            pagination_html = pagination_html,
        )
    } else {
        format!("{add_panel}{filter_panel}<div class=\"cards\">{cards}</div>{pagination_html}")
    }
}

fn render_add_equip_panel(
    state: &AppState,
    delete_mode: bool,
    lock_mode: bool,
    locale: Locale,
) -> String {
    let _ = state;
    let title = t(locale, "disc.title");
    let add = t(locale, "disc.add");
    let new_disc = t(locale, "disc.new_disc");
    let generate_discs = t(locale, "disc.generate_discs");
    let delete_discs = t(locale, "disc.delete_discs");
    let lock_discs = t(locale, "disc.lock_discs");
    let exit_delete = t(locale, "disc.exit_delete");
    let exit_lock = t(locale, "disc.exit_lock");
    if delete_mode {
        format!(
            "<div class=\"panel\"><h3>{title}</h3><div style=\"display:flex; gap:8px;\"><a href=\"/equip/new\">{new_disc}</a><a href=\"/equip/generate\">{generate_discs}</a><a href=\"/dashboard?tab=discs\">{exit_delete}</a></div></div>"
        )
    } else if lock_mode {
        format!(
            "<div class=\"panel\"><h3>{title}</h3><div style=\"display:flex; gap:8px;\"><a href=\"/equip/new\">{new_disc}</a><a href=\"/equip/generate\">{generate_discs}</a><a href=\"/dashboard?tab=discs\">{exit_lock}</a></div></div>"
        )
    } else {
        format!(
            "<div class=\"panel\"><h3>{add}</h3><div style=\"display:flex; gap:8px;\"><a href=\"/equip/new\">{new_disc}</a><a href=\"/equip/generate\">{generate_discs}</a><a href=\"/dashboard?tab=discs&delete=1\">{delete_discs}</a><a href=\"/dashboard?tab=discs&lock=1\">{lock_discs}</a></div></div>"
        )
    }
}

fn render_generate_slot_options(selected: Option<u32>, locale: Locale) -> String {
    let mut html = String::new();
    html.push_str(&format!(
        "<option value=\"\"{}>{}</option>",
        if selected.is_none() { " selected" } else { "" },
        t(locale, "disc.not_selected"),
    ));
    for slot in 1..=6 {
        html.push_str(&format!(
            "<option value=\"{}\"{}>{} {}</option>",
            slot,
            if selected == Some(slot) {
                " selected"
            } else {
                ""
            },
            t(locale, "slot"),
            slot
        ));
    }
    html
}

fn render_disc_select_options(state: &AppState, selected_id: u32, locale: Locale) -> String {
    let hakushin = load_hakushin_data(state, locale);
    let equip_index = load_equip_template_index(&state.asset_dir);
    let known_sets: std::collections::HashSet<u32> = equip_index
        .by_suit_slot
        .keys()
        .map(|(set_id, _)| *set_id)
        .collect();
    let mut items: Vec<(u32, String)> = hakushin
        .discs
        .iter()
        .filter(|(id, _)| known_sets.contains(id))
        .map(|(id, entry)| (*id, entry.name.clone()))
        .collect();
    items.sort_by(|a, b| a.1.cmp(&b.1));

    let mut html = String::new();
    html.push_str(&format!(
        "<option value=\"\" disabled selected>{}</option>",
        t(locale, "disc.select")
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

fn render_disc_filter_panel(
    state: &AppState,
    locale: Locale,
    filter_set_id: Option<u32>,
    filter_slot: Option<u32>,
    filter_main_stat: Option<u32>,
    filter_status: Option<&str>,
) -> String {
    let set_opts = {
        let hakushin = load_hakushin_data(state, locale);
        let equip_index = load_equip_template_index(&state.asset_dir);
        let known_sets: std::collections::HashSet<u32> = equip_index
            .by_suit_slot
            .keys()
            .map(|(set_id, _)| *set_id)
            .collect();
        let mut items: Vec<(u32, String)> = hakushin
            .discs
            .iter()
            .filter(|(id, _)| known_sets.contains(id))
            .map(|(id, entry)| (*id, entry.name.clone()))
            .collect();
        items.sort_by(|a, b| a.1.cmp(&b.1));
        let mut html = format!("<option value=\"\">{}</option>", t(locale, "disc.filter_all"));
        for (id, name) in items {
            let sel = if filter_set_id == Some(id) { " selected" } else { "" };
            html.push_str(&format!("<option value=\"{}\"{}>{}</option>", id, sel, name));
        }
        html
    };

    let slot_opts = {
        let mut html = format!("<option value=\"\">{}</option>", t(locale, "disc.filter_all"));
        for s in 1..=6 {
            let sel = if filter_slot == Some(s) { " selected" } else { "" };
            html.push_str(&format!("<option value=\"{}\"{}>{} {}</option>", s, sel, t(locale, "slot"), s));
        }
        html
    };

    let main_stat_opts = {
        let mut html = format!("<option value=\"\">{}</option>", t(locale, "disc.filter_all"));
        let all_keys: &[u32] = &[
            11103, 12103, 13103,
            11102, 12102, 13102,
            20103, 21103,
            31203, 23202,
            31402, 12202,
            30502,
            31503, 31603,
            31703, 31803, 31903,
        ];
        for &key in all_keys {
            let label = stat_label(state, locale, key);
            let sel = if filter_main_stat == Some(key) { " selected" } else { "" };
            html.push_str(&format!("<option value=\"{}\"{}>{}</option>", key, sel, label));
        }
        html
    };

    let status_opts = {
        let all_sel = if filter_status.is_none() || filter_status == Some("") { " selected" } else { "" };
        let locked_sel = if filter_status == Some("locked") { " selected" } else { "" };
        let unlocked_sel = if filter_status == Some("unlocked") { " selected" } else { "" };
        format!(
            "<option value=\"\"{}>{}</option><option value=\"locked\"{}>{}</option><option value=\"unlocked\"{}>{}</option>",
            all_sel, t(locale, "disc.filter_all"),
            locked_sel, t(locale, "disc.filter_locked"),
            unlocked_sel, t(locale, "disc.filter_unlocked"),
        )
    };

    format!(
        r#"<form method="get" action="/dashboard" style="margin-bottom:12px;">
            <input type="hidden" name="tab" value="discs" />
            <div style="display:flex; gap:8px; flex-wrap:wrap; align-items:end;">
                <div style="display:flex; flex-direction:column; gap:4px;">
                    <span style="font-size:11px; color:#9aa4b2;">{set_label}</span>
                    <select name="set_id" onchange="this.form.submit()" style="width:auto; padding:5px 8px; border-radius:8px; border:1px solid #2a3140; background:#121620; color:#e6e6e6; font-size:12px;">{set_opts}</select>
                </div>
                <div style="display:flex; flex-direction:column; gap:4px;">
                    <span style="font-size:11px; color:#9aa4b2;">{slot_label}</span>
                    <select name="slot" onchange="this.form.submit()" style="width:auto; padding:5px 8px; border-radius:8px; border:1px solid #2a3140; background:#121620; color:#e6e6e6; font-size:12px;">{slot_opts}</select>
                </div>
                <div style="display:flex; flex-direction:column; gap:4px;">
                    <span style="font-size:11px; color:#9aa4b2;">{main_stat_label}</span>
                    <select name="main_stat" onchange="this.form.submit()" style="width:auto; padding:5px 8px; border-radius:8px; border:1px solid #2a3140; background:#121620; color:#e6e6e6; font-size:12px;">{main_stat_opts}</select>
                </div>
                <div style="display:flex; flex-direction:column; gap:4px;">
                    <span style="font-size:11px; color:#9aa4b2;">{status_label}</span>
                    <select name="status" onchange="this.form.submit()" style="width:auto; padding:5px 8px; border-radius:8px; border:1px solid #2a3140; background:#121620; color:#e6e6e6; font-size:12px;">{status_opts}</select>
                </div>
            </div>
        </form>"#,
        set_label = t(locale, "disc.filter_set"),
        slot_label = t(locale, "disc.filter_slot"),
        main_stat_label = t(locale, "disc.filter_main_stat"),
        status_label = t(locale, "disc.filter_status"),
    )
}

fn render_pagination(
    locale: Locale,
    page: u32,
    total_pages: u32,
    filter_set_id: Option<u32>,
    filter_slot: Option<u32>,
    filter_main_stat: Option<u32>,
    filter_status: Option<&str>,
    total: usize,
) -> String {
    let showing_label = t(locale, "disc.showing");
    let page_label = t(locale, "disc.page");
    let prev_label = t(locale, "disc.prev");
    let next_label = t(locale, "disc.next");

    let per_page: usize = 50;
    let start = ((page - 1) as usize) * per_page + 1;
    let end = total.min(start + per_page - 1);

    let mut filter_params = String::from("tab=discs");
    if let Some(s) = filter_set_id {
        filter_params.push_str(&format!("&set_id={}", s));
    }
    if let Some(s) = filter_slot {
        filter_params.push_str(&format!("&slot={}", s));
    }
    if let Some(m) = filter_main_stat {
        filter_params.push_str(&format!("&main_stat={}", m));
    }
    if let Some(st) = filter_status {
        if !st.is_empty() {
            filter_params.push_str(&format!("&status={}", st));
        }
    }

    let prev_link = if page > 1 {
        format!(
            "<a href=\"/dashboard?{}&page={}\" style=\"padding:6px 12px; border-radius:8px; background:#2a3140; color:#c7d1e0; text-decoration:none; font-size:12px; font-weight:600;\">{}</a>",
            filter_params, page - 1, prev_label,
        )
    } else {
        format!(
            "<span style=\"padding:6px 12px; border-radius:8px; background:#1b2230; color:#5a6474; font-size:12px; font-weight:600;\">{}</span>",
            prev_label,
        )
    };

    let next_link = if page < total_pages {
        format!(
            "<a href=\"/dashboard?{}&page={}\" style=\"padding:6px 12px; border-radius:8px; background:#2a3140; color:#c7d1e0; text-decoration:none; font-size:12px; font-weight:600;\">{}</a>",
            filter_params, page + 1, next_label,
        )
    } else {
        format!(
            "<span style=\"padding:6px 12px; border-radius:8px; background:#1b2230; color:#5a6474; font-size:12px; font-weight:600;\">{}</span>",
            next_label,
        )
    };

    format!(
        "<div style=\"display:flex; align-items:center; justify-content:space-between; gap:12px; margin-top:16px; flex-wrap:wrap;\">
            <span class=\"meta\">{showing} {start}&ndash;{end} / {total}</span>
            <div style=\"display:flex; gap:6px; align-items:center;\">
                {prev_link}
                <span style=\"font-size:12px; color:#9aa4b2;\">{page_label} {page}/{total_pages}</span>
                {next_link}
            </div>
        </div>",
        showing = showing_label,
        total = total,
        page_label = page_label,
        total_pages = total_pages,
    )
}
