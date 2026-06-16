use crate::{app_state::AppState, auth::html_escape_attr, i18n::Locale, i18n::t};
use std::{
    fs,
    path::{Path as FsPath, PathBuf},
};

struct UpdateFileInfo {
    file_name: String,
    relative_path: String,
    size_bytes: u64,
    modified: Option<std::time::SystemTime>,
}

pub(crate) fn render_client_updates_panel(
    state: &AppState,
    server_host: &str,
    locale: Locale,
    is_admin: bool,
) -> String {
    let beta_patch = find_update_file(
        &state.root_dir.join("client_updates/Beta/Patch"),
        "vortex_patch_beta_",
    );
    let beta_updates = list_update_files(&state.root_dir.join("client_updates/Beta/Update"));
    let prod_patch = find_update_file(
        &state.root_dir.join("client_updates/Prod/Patch"),
        "vortex_patch_prod_",
    );

    let beta_items = vec![
        (
            t(locale, "updates.patch").to_string(),
            beta_patch.into_iter().collect::<Vec<_>>(),
        ),
        (t(locale, "updates.update").to_string(), beta_updates),
    ];
    let prod_items = vec![(
        t(locale, "updates.patch").to_string(),
        prod_patch.into_iter().collect::<Vec<_>>(),
    )];

    let mut admin_upload = String::new();
    if is_admin {
        admin_upload = format!(
            r#"<div style="margin-bottom:12px;">
                <form id="upload-form" method="post" action="/admin/upload-update" enctype="multipart/form-data" style="display:flex; gap:8px; align-items:end;">
                    <div style="flex:1;">
                        <label style="margin:0 0 4px; font-size:11px; color:#9aa4b2;">{upload_label}</label>
                        <input type="file" name="file" id="upload-file" required style="width:100%; box-sizing:border-box; padding:6px; border-radius:8px; border:1px solid #2a3140; background:#121620; color:#e6e6e6; font-size:13px;" />
                    </div>
                    <button type="submit" class="apply" id="upload-btn">{upload_btn}</button>
                </form>
                <div id="upload-progress-wrap" style="display:none; margin-top:8px;">
                    <div style="display:flex; justify-content:space-between; font-size:11px; color:#9aa4b2; margin-bottom:4px;">
                        <span id="upload-progress-text">0%</span>
                        <span id="upload-speed-text"></span>
                    </div>
                    <div style="height:8px; background:#232a38; border-radius:4px; overflow:hidden;">
                        <div id="upload-progress-bar" style="height:100%; width:0%; background:#4c7dff; border-radius:4px; transition: width 0.3s;"></div>
                    </div>
                </div>
                <div id="upload-error" style="color:#ef4444; font-size:12px; margin-top:6px; display:none;"></div>
                <script>
                (function() {{
                    var form = document.getElementById("upload-form");
                    var fileInput = document.getElementById("upload-file");
                    var btn = document.getElementById("upload-btn");
                    var wrap = document.getElementById("upload-progress-wrap");
                    var bar = document.getElementById("upload-progress-bar");
                    var text = document.getElementById("upload-progress-text");
                    var speedEl = document.getElementById("upload-speed-text");
                    var errorEl = document.getElementById("upload-error");
                    form.addEventListener("submit", function(e) {{
                        e.preventDefault();
                        var file = fileInput.files[0];
                        if (!file) return;
                        errorEl.style.display = "none";
                        btn.disabled = true;
                        btn.textContent = "{uploading}...";
                        wrap.style.display = "block";
                        bar.style.width = "0%";
                        text.textContent = "0%";
                        var formData = new FormData();
                        formData.append("file", file);
                    var xhr = new XMLHttpRequest();
                    var startTime = Date.now();
                    var uploading = true;
                    var warnOnLeave = function(e) {{ if (uploading) {{ e.preventDefault(); e.returnValue = ""; }} }};
                    window.addEventListener("beforeunload", warnOnLeave);
                    xhr.upload.addEventListener("progress", function(ev) {{
                            if (ev.lengthComputable) {{
                                var pct = Math.round(ev.loaded / ev.total * 100);
                                bar.style.width = pct + "%";
                                text.textContent = pct + "%";
                                var elapsed = (Date.now() - startTime) / 1000;
                                if (elapsed > 0) {{
                                    var mbps = (ev.loaded / 1024 / 1024) / elapsed;
                                    speedEl.textContent = mbps.toFixed(1) + " MB/s";
                                }}
                            }}
                        }});
                        var cleanup = function() {{
                            uploading = false;
                            window.removeEventListener("beforeunload", warnOnLeave);
                        }};
                        xhr.addEventListener("load", function() {{
                            cleanup();
                            if (xhr.status >= 200 && xhr.status < 300) {{
                                window.location.href = "/dashboard?tab=updates";
                            }} else {{
                                errorEl.textContent = xhr.responseText || "Upload failed";
                                errorEl.style.display = "block";
                                btn.disabled = false;
                                btn.textContent = "{upload_btn}";
                                wrap.style.display = "none";
                            }}
                        }});
                        xhr.addEventListener("error", function() {{
                            cleanup();
                            errorEl.textContent = "Network error during upload";
                            errorEl.style.display = "block";
                            btn.disabled = false;
                            btn.textContent = "{upload_btn}";
                            wrap.style.display = "none";
                        }});
                        xhr.addEventListener("abort", function() {{
                            cleanup();
                            errorEl.textContent = "Upload cancelled";
                            errorEl.style.display = "block";
                            btn.disabled = false;
                            btn.textContent = "{upload_btn}";
                            wrap.style.display = "none";
                        }});
                        xhr.open("POST", "/admin/upload-update", true);
                        xhr.send(formData);
                    }});
                }})();
                </script>
            </div>"#,
            upload_label = t(locale, "updates.upload_beta_update"),
            upload_btn = t(locale, "updates.upload"),
            uploading = t(locale, "updates.uploading"),
        );
    }

    let beta_cards = render_update_group("Beta", &beta_items, true, server_host, locale, is_admin);
    let prod_cards = render_update_group("Prod", &prod_items, false, server_host, locale, false);

    format!(
        r#"<div style="display:grid; gap:16px;">
            <div class="card" style="background: #1b1f2a; padding: 16px; border-radius: 12px; border: 1px solid #232a38;">
                <h3 style="margin-top: 0;">Beta</h3>
                {admin_upload}
                <div style="display:grid; gap:12px; min-width:0;">{beta_cards}</div>
            </div>
            <div class="card" style="background: #1b1f2a; padding: 16px; border-radius: 12px; border: 1px solid #232a38;">
                <h3 style="margin-top: 0;">Prod</h3>
                <div style="display:grid; gap:12px; min-width:0;">{prod_cards}</div>
            </div>
        </div>"#
    )
}

fn render_update_group(
    title: &str,
    items: &[(String, Vec<UpdateFileInfo>)],
    show_note: bool,
    server_host: &str,
    locale: Locale,
    is_admin: bool,
) -> String {
    let no_file = t(locale, "updates.no_file");
    let unknown = t(locale, "updates.unknown");
    let download_prefix = t(locale, "updates.download");
    let updated_prefix = t(locale, "updates.updated");
    let aria2c_desc = t(locale, "updates.aria2c_desc");
    let delete_label = t(locale, "updates.delete");
    let mut inner = String::new();
    for (label, files) in items {
        if files.is_empty() {
            inner.push_str(&format!(
                r#"<div style="padding: 12px; border-radius: 10px; background: #121620; border: 1px solid #232a38; min-width: 0;">
                    <div style="font-size: 13px; font-weight: 700; color: #e6e6e6; margin-bottom: 6px;">{label}</div>
                    <div class="meta">{no_file}</div>
                </div>"#,
                label = label,
                no_file = no_file,
            ));
        } else {
            for file in files {
                let updated = file
                    .modified
                    .map(format_system_time)
                    .unwrap_or_else(|| unknown.to_string());
                let download_url = format!("/assets/{}", file.relative_path);
                let mut card_html = format!(
                    r#"<div style="padding: 12px; border-radius: 10px; background: #121620; border: 1px solid #232a38; display: grid; gap: 6px; min-width: 0;">
                        <div style="font-size: 13px; font-weight: 700; color: #e6e6e6;">{label}</div>
                        <div class="meta" style="overflow-wrap:anywhere; word-break: break-word;">{name}</div>
                        <div style="display:flex; gap:8px;">
                            <a href="{download_url}" download style="flex:1; display:flex; align-items:center; justify-content:center; text-align:center; white-space:normal; overflow-wrap:anywhere; word-break:break-word; padding: 10px 12px; border-radius: 8px; background: #4c7dff; color: #fff; text-decoration: none; font-weight: 700;">
                                {download_prefix} {name} {size}
                            </a>"#,
                    label = label,
                    name = file.file_name,
                    download_url = download_url,
                    download_prefix = download_prefix,
                    size = format_file_size(file.size_bytes),
                );

                if is_admin && title == "Beta" && label == t(locale, "updates.update") {
                    card_html.push_str(&format!(
                        r#"<form method="post" action="/admin/delete-update" style="display:flex; flex:0 0 auto;">
                            <input type="hidden" name="filename" value="{}" />
                            <button type="submit" class="danger" style="padding:6px 10px; font-size:11px; white-space:nowrap;" onclick="return confirm('Delete {}?')">{}</button>
                        </form>"#,
                        html_escape_attr(&file.file_name),
                        html_escape_attr(&file.file_name),
                        delete_label,
                    ));
                }

                card_html.push_str("</div>");

                if label == t(locale, "updates.update") {
                    let aria2c_command = format!(
                        "aria2c -x 16 -s 16 -k 1M -c \"http://{}{}\"",
                        server_host, download_url
                    );
                    card_html.push_str(&format!(
                        r#"<div style="padding: 8px; border-radius: 6px; background: #0f1115; border: 1px solid #1f2635; font-size: 12px; color: #9aa4b2; line-height: 1.4; min-width: 0;">
                            <div style="margin-bottom: 6px; color: #b8c0cc;">{aria2c_desc}</div>
                            <code style="display: block; background: #0a0d11; padding: 6px; border-radius: 4px; font-family: monospace; font-size: 11px; overflow-x: auto; white-space: pre-wrap; word-break: break-all; overflow-wrap:anywhere; color: #6c9cff;">{}</code>
                        </div>"#,
                        aria2c_command
                    ));
                }

                card_html.push_str(&format!(
                    r#"<div class="meta">{updated_prefix}: {updated}</div>"#,
                    updated = updated,
                    updated_prefix = updated_prefix,
                ));

                card_html.push_str("</div>");
                inner.push_str(&card_html);
            }
        }
    }

    let note = if show_note {
        format!(
            "<div class=\"meta\" style=\"margin-top: 8px;\">{}</div>",
            t(locale, "updates.beta_note")
        )
    } else {
        format!(
            "<div class=\"meta\" style=\"margin-top: 8px;\">{}</div>",
            t(locale, "updates.prod_note")
        )
    };

    format!(
        r#"<div style="display:grid; gap:12px; min-width:0;">{inner}</div>{note}"#,
        inner = inner,
        note = note,
    )
}

fn list_update_files(dir: &FsPath) -> Vec<UpdateFileInfo> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut files = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("zip") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Some(relative_path) = path_to_asset_relative(&path) else {
            continue;
        };

        files.push(UpdateFileInfo {
            file_name: path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown.zip".to_string()),
            relative_path,
            size_bytes: metadata.len(),
            modified: metadata.modified().ok(),
        });
    }

    files.sort_by(|a, b| {
        let a_time = a.modified.unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let b_time = b.modified.unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        b_time.cmp(&a_time)
    });

    files
}

fn find_update_file(dir: &FsPath, prefix: &str) -> Option<UpdateFileInfo> {
    let mut latest: Option<(PathBuf, fs::Metadata)> = None;
    let Ok(entries) = fs::read_dir(dir) else {
        return None;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("zip") {
            continue;
        }
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => continue,
        };
        if !file_name.starts_with(prefix) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let modified = metadata.modified().ok();
        let is_newer = match (&latest, modified) {
            (None, _) => true,
            (Some((_, existing)), Some(candidate)) => {
                candidate
                    > existing
                        .modified()
                        .ok()
                        .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
            }
            _ => false,
        };
        if is_newer {
            latest = Some((path, metadata));
        }
    }

    latest.and_then(|(path, metadata)| {
        Some(UpdateFileInfo {
            file_name: path.file_name()?.to_string_lossy().to_string(),
            relative_path: path_to_asset_relative(&path)?,
            size_bytes: metadata.len(),
            modified: metadata.modified().ok(),
        })
    })
}

fn path_to_asset_relative(path: &FsPath) -> Option<String> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    path.strip_prefix(&root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
}

fn format_file_size(bytes: u64) -> String {
    let bytes_f = bytes as f64;
    let gib = 1024.0 * 1024.0 * 1024.0;
    let mib = 1024.0 * 1024.0;
    if bytes_f >= gib {
        format!("{:.2} GB", bytes_f / gib)
    } else {
        format!("{:.2} MB", bytes_f / mib)
    }
}

fn format_system_time(time: std::time::SystemTime) -> String {
    use chrono::{DateTime, Utc};
    let datetime: DateTime<Utc> = time.into();
    datetime.format("%Y-%m-%d %H:%M UTC").to_string()
}
