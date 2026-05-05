use crate::app_state::AppState;
use std::{
    env, fs,
    path::{Path as FsPath, PathBuf},
};

struct UpdateFileInfo {
    file_name: String,
    relative_path: String,
    size_bytes: u64,
    modified: Option<std::time::SystemTime>,
}

pub(crate) fn render_client_updates_panel(state: &AppState, server_host: &str) -> String {
    let beta_patch = find_update_file(
        &state.root_dir.join("client_updates/Beta/Patch"),
        "tentacle_patch.zip",
    );
    let beta_updates = list_update_files(&state.root_dir.join("client_updates/Beta/Update"));
    let prod_patch = find_update_file(
        &state.root_dir.join("client_updates/Prod/Patch"),
        "tentacle_patch.zip",
    );

    let beta_items = vec![
        (
            "Patch".to_string(),
            beta_patch.into_iter().collect::<Vec<_>>(),
        ),
        ("Update".to_string(), beta_updates),
    ];
    let prod_items = vec![(
        "Patch".to_string(),
        prod_patch.into_iter().collect::<Vec<_>>(),
    )];

    let beta_cards = render_update_group("Beta", &beta_items, true, server_host);
    let prod_cards = render_update_group("Prod", &prod_items, false, server_host);

    format!(
        r#"<div style="display:grid; gap:16px;">
            {beta_cards}
            {prod_cards}
        </div>"#
    )
}

fn render_update_group(
    title: &str,
    items: &[(String, Vec<UpdateFileInfo>)],
    show_note: bool,
    server_host: &str,
) -> String {
    let mut inner = String::new();
    for (label, files) in items {
        if files.is_empty() {
            inner.push_str(&format!(
                r#"<div style="padding: 12px; border-radius: 10px; background: #121620; border: 1px solid #232a38; min-width: 0;">
                    <div style="font-size: 13px; font-weight: 700; color: #e6e6e6; margin-bottom: 6px;">{label}</div>
                    <div class="meta">No file available yet.</div>
                </div>"#,
                label = label,
            ));
        } else {
            for file in files {
                let updated = file
                    .modified
                    .map(format_system_time)
                    .unwrap_or_else(|| "Unknown".to_string());
                let download_url = format!("/assets/{}", file.relative_path);
                let mut card_html = format!(
                    r#"<div style="padding: 12px; border-radius: 10px; background: #121620; border: 1px solid #232a38; display: grid; gap: 6px; min-width: 0;">
                        <div style="font-size: 13px; font-weight: 700; color: #e6e6e6;">{label}</div>
                        <div class="meta" style="overflow-wrap:anywhere; word-break: break-word;">{name}</div>
                        <a href="{download_url}" download style="display:flex; width:100%; box-sizing:border-box; align-items:center; justify-content:center; text-align:center; white-space:normal; overflow-wrap:anywhere; word-break:break-word; padding: 10px 12px; border-radius: 8px; background: #4c7dff; color: #fff; text-decoration: none; font-weight: 700;">
                            Download {name} {size}
                        </a>
                        <div class="meta">Updated: {updated}</div>"#,
                    label = label,
                    name = file.file_name,
                    download_url = download_url,
                    size = format_file_size(file.size_bytes),
                    updated = updated,
                );

                if label == "Update" {
                    let aria2c_command = format!(
                        "aria2c -x 16 -s 16 -k 1M -c \"http://{}{}\"",
                        server_host, download_url
                    );
                    card_html.push_str(&format!(
                        r#"<div style="padding: 8px; border-radius: 6px; background: #0f1115; border: 1px solid #1f2635; font-size: 12px; color: #9aa4b2; line-height: 1.4; min-width: 0;">
                            <div style="margin-bottom: 6px; color: #b8c0cc;">For faster downloads, we recommend using aria2c with parallel connections:</div>
                            <code style="display: block; background: #0a0d11; padding: 6px; border-radius: 4px; font-family: monospace; font-size: 11px; overflow-x: auto; white-space: pre-wrap; word-break: break-all; overflow-wrap:anywhere; color: #6c9cff;">{}</code>
                        </div>"#,
                        aria2c_command
                    ));
                }

                card_html.push_str("</div>");
                inner.push_str(&card_html);
            }
        }
    }

    let note = if show_note {
        "<div class=\"meta\" style=\"margin-top: 8px;\">Beta includes Patch and Update. Prod includes Patch only.</div>"
    } else {
        "<div class=\"meta\" style=\"margin-top: 8px;\">Prod distribution currently ships Patch only.</div>"
    };

    format!(
        r#"<div class="card" style="background: #1b1f2a; padding: 16px; border-radius: 12px; border: 1px solid #232a38;">
            <h3 style="margin-top: 0;">{title}</h3>
            <div style="display:grid; gap:12px; min-width:0;">{inner}</div>
            {note}
        </div>"#,
        title = title,
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

fn find_update_file(dir: &FsPath, preferred_name: &str) -> Option<UpdateFileInfo> {
    let preferred_path = dir.join(preferred_name);
    if let Ok(metadata) = fs::metadata(&preferred_path) {
        return Some(UpdateFileInfo {
            file_name: preferred_name.to_string(),
            relative_path: path_to_asset_relative(&preferred_path)?,
            size_bytes: metadata.len(),
            modified: metadata.modified().ok(),
        });
    }

    let mut latest: Option<(PathBuf, fs::Metadata)> = None;
    let Ok(entries) = fs::read_dir(dir) else {
        return None;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("zip") {
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
