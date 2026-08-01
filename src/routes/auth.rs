use crate::{
    AppState,
    app_state::parse_server_selection,
    auth::{
        get_session, html_escape_attr, html_escape_text, insert_session, redirect_to_login,
        remove_session, sanitize_next_path, url_encode_component, validate_login,
    },
    i18n::{Locale, locale_from_headers, t},
    sdk,
    utils::audit_log,
};
use axum::{
    extract::{Form, OriginalUri, Query, State},
    http::{HeaderMap, HeaderValue, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct LoginForm {
    username: String,
    password: String,
    next: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct LoginQuery {
    next: Option<String>,
    error: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SwitchServerQuery {
    target: Option<String>,
    next: Option<String>,
}

pub(crate) async fn login_page(
    Query(query): Query<LoginQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if get_session(&headers).is_some() {
        return Redirect::to("/dashboard").into_response();
    }

    let locale = locale_from_headers(&headers);
    let next = query
        .next
        .as_deref()
        .and_then(sanitize_next_path)
        .unwrap_or_else(|| "/dashboard".to_string());
    let error = query.error.as_deref().filter(|e| !e.is_empty());
    Html(render_login_form(locale, &next, error)).into_response()
}

fn render_login_form(locale: Locale, next: &str, error: Option<&str>) -> String {
    let next_attr = html_escape_attr(next);
    let error_html = match error {
        Some(msg) => format!(
            "<div style=\"background:#3d1420;color:#fca5a5;border:1px solid #6b2136;padding:10px 12px;border-radius:8px;font-size:13px;margin-bottom:0;\">{}</div>",
            html_escape_text(t(locale, msg))
        ),
        None => String::new(),
    };

    format!(
        r#"<!doctype html>
<html lang="{lang}">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{title}</title>
  <style>
        body {{ font-family: system-ui, sans-serif; background: #0f1115; color: #e6e6e6; display: grid; place-items: center; min-height: 100vh; min-height: 100dvh; margin: 0; }}
        form {{ background: #1b1f2a; padding: 24px; border-radius: 12px; width: 320px; box-sizing: border-box; box-shadow: 0 10px 30px rgba(0,0,0,.4); display: flex; flex-direction: column; gap: 12px; }}
    h1 {{ font-size: 18px; margin: 0; }}
    .field {{ display: flex; flex-direction: column; gap: 6px; }}
    label {{ display: block; margin: 0; font-size: 12px; color: #9aa4b2; }}
    input {{ width: 100%; box-sizing: border-box; padding: 10px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
    button {{ width: 100%; padding: 10px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }}
        @media (max-width: 768px) {{
            body {{ display: flex; align-items: center; justify-content: center; height: auto; min-height: 100vh; min-height: 100dvh; padding: 16px; box-sizing: border-box; }}
            form {{ width: 100%; max-width: 420px; margin: 0; box-sizing: border-box; }}
        }}
  </style>
</head>
<body>
  <form method="post" action="/login">
    <h1>{title}</h1>
        {error}
        <input type="hidden" name="next" value="{next_attr}" />
        <div class="field">
            <label for="username">{username_label}</label>
            <input id="username" name="username" autocomplete="username" required />
        </div>
        <div class="field">
            <label for="password">{password_label}</label>
            <input id="password" name="password" type="password" autocomplete="current-password" required />
        </div>
    <button type="submit">{sign_in}</button>
    <div style="text-align:center;">
      <a href="/register?next={next_attr}" style="color:#9aa4b2; font-size:12px; text-decoration:none;">{register_link}</a>
    </div>
  </form>
</body>
</html>"#,
        error = error_html,
        next_attr = next_attr,
        title = t(locale, "login.title"),
        username_label = t(locale, "login.username"),
        password_label = t(locale, "login.password"),
        sign_in = t(locale, "login.sign_in"),
        register_link = t(locale, "login.register"),
        lang = locale.lang_attr(),
    )
}

pub(crate) async fn register_page(
    Query(query): Query<LoginQuery>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if get_session(&headers).is_some() {
        return Redirect::to("/dashboard").into_response();
    }

    let locale = locale_from_headers(&headers);
    let next = query
        .next
        .as_deref()
        .and_then(sanitize_next_path)
        .unwrap_or_else(|| "/dashboard".to_string());
    let error = query.error.as_deref().filter(|e| !e.is_empty());
    Html(render_register_form(locale, &next, error)).into_response()
}

fn render_register_form(locale: Locale, next: &str, error: Option<&str>) -> String {
    let next_attr = html_escape_attr(next);
    let error_html = match error {
        Some(msg) => format!(
            "<div style=\"background:#3d1420;color:#fca5a5;border:1px solid #6b2136;padding:10px 12px;border-radius:8px;font-size:13px;margin-bottom:0;\">{}</div>",
            html_escape_text(t(locale, msg))
        ),
        None => String::new(),
    };

    format!(
        r#"<!doctype html>
<html lang="{lang}">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{title}</title>
  <style>
        body {{ font-family: system-ui, sans-serif; background: #0f1115; color: #e6e6e6; display: grid; place-items: center; min-height: 100vh; min-height: 100dvh; margin: 0; }}
        form {{ background: #1b1f2a; padding: 24px; border-radius: 12px; width: 320px; box-sizing: border-box; box-shadow: 0 10px 30px rgba(0,0,0,.4); display: flex; flex-direction: column; gap: 12px; }}
    h1 {{ font-size: 18px; margin: 0; }}
    .field {{ display: flex; flex-direction: column; gap: 6px; }}
    label {{ display: block; margin: 0; font-size: 12px; color: #9aa4b2; }}
    input {{ width: 100%; box-sizing: border-box; padding: 10px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
    button {{ width: 100%; padding: 10px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }}
        @media (max-width: 768px) {{
            body {{ display: flex; align-items: center; justify-content: center; height: auto; min-height: 100vh; min-height: 100dvh; padding: 16px; box-sizing: border-box; }}
            form {{ width: 100%; max-width: 420px; margin: 0; box-sizing: border-box; }}
        }}
  </style>
</head>
<body>
  <form method="post" action="/register">
    <h1>{title}</h1>
        {error}
        <input type="hidden" name="next" value="{next_attr}" />
        <div class="field">
            <label for="reg-username">{username_label}</label>
            <input id="reg-username" name="username" autocomplete="username" required />
        </div>
        <div class="field">
            <label for="reg-password">{password_label}</label>
            <input id="reg-password" name="password" type="password" autocomplete="new-password" required />
        </div>
    <button type="submit">{register_submit}</button>
    <div style="text-align:center;">
      <a href="/?next={next_attr}" style="color:#9aa4b2; font-size:12px; text-decoration:none;">{back_to_login}</a>
    </div>
  </form>
</body>
</html>"#,
        error = error_html,
        next_attr = next_attr,
        title = t(locale, "login.register_title"),
        username_label = t(locale, "login.username"),
        password_label = t(locale, "login.password"),
        register_submit = t(locale, "login.register_submit"),
        back_to_login = t(locale, "login.back_to_login"),
        lang = locale.lang_attr(),
    )
}

pub(crate) async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<LoginForm>,
) -> impl IntoResponse {
    let _locale = locale_from_headers(&headers);
    let username = payload.username.trim().to_string();
    let response: Response = match validate_login(&state, &username, &payload.password) {
        Ok(Some((session, _is_admin))) => {
            let uid = session.uid;
            let session_id = insert_session(session);

            audit_log(&state.root_dir, &username, uid, "login", "successful login");

            let mut headers = HeaderMap::new();
            headers.insert(
                header::SET_COOKIE,
                format!(
                    "ge_session={}; HttpOnly; SameSite=Lax; Path=/; Max-Age=2592000",
                    session_id
                )
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
        Ok(None) => {
            let next = payload
                .next
                .as_deref()
                .and_then(sanitize_next_path)
                .unwrap_or_else(|| "/dashboard".to_string());
            let location = format!(
                "/?next={}&error={}",
                url_encode_component(&next),
                url_encode_component("login.invalid_credentials")
            );
            Redirect::to(&location).into_response()
        }
        Err(_) => {
            let next = payload
                .next
                .as_deref()
                .and_then(sanitize_next_path)
                .unwrap_or_else(|| "/dashboard".to_string());
            let location = format!(
                "/?next={}&error={}",
                url_encode_component(&next),
                url_encode_component("login.failed")
            );
            Redirect::to(&location).into_response()
        }
    };

    response
}

pub(crate) async fn register(
    State(state): State<AppState>,
    _headers: HeaderMap,
    Form(payload): Form<LoginForm>,
) -> impl IntoResponse {
    let username = payload.username.trim().to_string();
    let next = payload
        .next
        .as_deref()
        .and_then(sanitize_next_path)
        .unwrap_or_else(|| "/dashboard".to_string());

    if username.is_empty() || payload.password.is_empty() {
        return redirect_with_error(&next, "login.register_invalid");
    }

    match sdk::register_account(&username, &payload.password).await {
        Ok(true) => {
            audit_log(&state.root_dir, &username, 0, "register", "account created");
            Redirect::to(&next).into_response()
        }
        Ok(false) => redirect_with_error(&next, "login.invalid_credentials"),
        Err(_) => redirect_with_error(&next, "login.failed"),
    }
}

fn redirect_with_error(next: &str, error_key: &str) -> Response {
    let location = format!(
        "/?next={}&error={}",
        url_encode_component(next),
        url_encode_component(error_key)
    );
    Redirect::to(&location).into_response()
}

pub(crate) async fn switch_server(
    headers: HeaderMap,
    original_uri: OriginalUri,
    Query(query): Query<SwitchServerQuery>,
) -> impl IntoResponse {
    let Some((_session_id, _session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let sel = parse_server_selection(query.target.as_deref().unwrap_or("beta:1"));
    let next = query
        .next
        .as_deref()
        .and_then(sanitize_next_path)
        .unwrap_or_else(|| "/dashboard".to_string());

    let mut response = Redirect::to(&next).into_response();
    let value = format!("gear_server={}; Path=/; SameSite=Lax", sel.cookie_value());
    if let Ok(header_value) = HeaderValue::from_str(&value) {
        response
            .headers_mut()
            .insert(header::SET_COOKIE, header_value);
    }

    response
}

pub(crate) async fn logout(State(state): State<AppState>, headers: HeaderMap) -> impl IntoResponse {
    let mut response = Redirect::to("/").into_response();

    if let Some((session_id, session)) = get_session(&headers) {
        audit_log(
            &state.root_dir,
            &session.username,
            session.uid,
            "logout",
            "session ended",
        );
        remove_session(&session_id);
    }

    response.headers_mut().insert(
        header::SET_COOKIE,
        "ge_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0"
            .parse()
            .unwrap(),
    );
    response
}
