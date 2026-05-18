use crate::{
    AppState,
    app_state::{ServerMode, parse_server_mode},
    auth::{
        get_session, html_escape_attr, html_escape_text, insert_session, redirect_to_login,
        remove_session, sanitize_next_path, set_session, url_encode_component, validate_login,
    },
    i18n::{Locale, locale_from_headers, t},
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
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>{title}</title>
  <style>
        body {{ font-family: system-ui, sans-serif; background: #0f1115; color: #e6e6e6; display: grid; place-items: center; height: 100vh; margin: 0; }}
        form {{ background: #1b1f2a; padding: 24px; border-radius: 12px; width: 320px; box-sizing: border-box; box-shadow: 0 10px 30px rgba(0,0,0,.4); display: flex; flex-direction: column; gap: 12px; }}
    h1 {{ font-size: 18px; margin: 0; }}
    .field {{ display: flex; flex-direction: column; gap: 6px; }}
    label {{ display: block; margin: 0; font-size: 12px; color: #9aa4b2; }}
    input {{ width: 100%; box-sizing: border-box; padding: 10px; border-radius: 8px; border: 1px solid #2a3140; background: #121620; color: #e6e6e6; }}
    button {{ width: 100%; padding: 10px; border: 0; border-radius: 8px; background: #4c7dff; color: #fff; font-weight: 600; cursor: pointer; }}
        @media (max-width: 768px) {{
            body {{ display: flex; align-items: center; justify-content: center; height: auto; min-height: 100vh; padding: 16px; box-sizing: border-box; }}
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
  </form>
</body>
</html>"#,
        error = error_html,
        next_attr = next_attr,
        title = t(locale, "login.title"),
        username_label = t(locale, "login.username"),
        password_label = t(locale, "login.password"),
        sign_in = t(locale, "login.sign_in"),
    )
}

pub(crate) async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Form(payload): Form<LoginForm>,
) -> impl IntoResponse {
    let _locale = locale_from_headers(&headers);
    let username = payload.username.trim().to_string();
    let response: Response = match validate_login(&state.db_path, &username, &payload.password) {
        Ok(Some(session)) => {
            let uid = session.uid;
            let session_id = insert_session(session);

            audit_log(
                &state.root_dir,
                &username,
                uid,
                "login",
                "successful login",
            );

            let mut headers = HeaderMap::new();
            headers.insert(
                header::SET_COOKIE,
                format!("ge_session={}; HttpOnly; SameSite=Lax; Path=/; Max-Age=604800", session_id)
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

pub(crate) async fn switch_server(
    headers: HeaderMap,
    original_uri: OriginalUri,
    Query(query): Query<SwitchServerQuery>,
) -> impl IntoResponse {
    let Some((session_id, mut session)) = get_session(&headers) else {
        return redirect_to_login(&original_uri.0);
    };

    let mode = parse_server_mode(query.target.as_deref().unwrap_or("beta"));
    let next = query
        .next
        .as_deref()
        .and_then(sanitize_next_path)
        .unwrap_or_else(|| "/dashboard".to_string());

    // Pending writes are path-bound; clear them on server switch to avoid cross-server apply.
    session.pending_writes.clear();
    set_session(session_id, session);

    let mut response = Redirect::to(&next).into_response();
    let value = match mode {
        ServerMode::Beta => "gear_server=beta; Path=/; SameSite=Lax",
        ServerMode::Prod => "gear_server=prod; Path=/; SameSite=Lax",
    };
    if let Ok(header_value) = HeaderValue::from_str(value) {
        response
            .headers_mut()
            .insert(header::SET_COOKIE, header_value);
    }

    response
}

pub(crate) async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
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
