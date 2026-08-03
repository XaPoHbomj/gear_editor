# Gear Editor — AGENTS.md

## Project Overview

Web admin panel for the remielle game server. All mutations (avatar, weapon, equip edits/creates/deletes) are sent to a running server via UDP control protocol (ctl). The old file-writing approach has been replaced by ctl commands.

**Tech stack:** Rust + Axum 0.7, inline `format!()` HTML, vanilla JS.

**Selection model:** 6 independent game servers (Beta 1-3, Prod 1-3). Each has its own save directory and ctl port. Selected via the `gear_server` cookie (`beta:N`/`prod:N`) and the header pills. No shared saves.

**Data dumps:** two synchronized versions under `zzz_dump/`:
- `zzz_dump/latest/` — used for **Beta** (via `AppState.dump_dir`)
- `zzz_dump/live/` — used for **Prod** (via `AppState.live_dump_dir`, swapped in `state_for_selected_server`)

The `sync_zzz_dump_assets.py` script downloads both targets. `AppState.dump_lang_dir(locale)` resolves against the active env's dump dir.

---

## Project Structure

```
gear_editor/
└── src/
    ├── main.rs         # Router, dashboard HTML with inline CSS
    ├── app_state.rs    # AppState, ServerSelection (beta/prod + server_num), cookie parsing, per-server dirs/ports/base_uid
    ├── auth.rs         # Session store, login via bcrypt from remielle SDK/passwd
    ├── sdk.rs          # Account registration: RSA-1024 encrypt + POST to sdksv login endpoint
    ├── assets.rs       # Static file serving from zzz_dump/assets/
    ├── ctl.rs          # UDP control protocol client (modAvatarMeta, createWeapon, modEquip, etc.)
    ├── i18n.rs         # 5-locale translation table (EN, RU, CN, KR, JP)
    ├── player_state.rs # UID resolution from GENERAL_DATA.bin, PlayerSave load
    ├── remielle_save.rs# Manual protobuf parser/serializer (~660 lines)
    ├── updates.rs      # Client updates panel (upload/delete/browse)
    ├── utils.rs        # apply_changes, shared_page_css, svg_data_uri
    ├── zon.rs          # ZON format parser: read_zon, zon_parse_entries
    ├── data/
    │   ├── hakushin.rs # Hakushin.gg dump: char/weapon/disc/bangboo names+images
    │   └── templates.rs# Template JSON via zon_parse_entries (ZON format)
    └── routes/
        ├── auth.rs     # Login page + register + switch-server (cookie beta:N/prod:N)
        ├── avatar.rs   # Character edit/update/cards (ctl modAvatarMeta)
        ├── weapon.rs   # Weapon edit/new/update (ctl create/modWeapon)
        ├── equip.rs    # Disc edit/new/generate/delete (ctl create/modEquip)
        ├── challenges.rs # DA/Shiyu detail pages + status tab (per selected server)
        └── admin.rs    # Client update upload/delete + hadal zone editing (ctl modHadalEntrance)
```

---

## Per-Server Save Layout

Each of the 6 game servers runs from its own CWD with its own save:

```
bin_remielle/server{1,2,3}/Persistent/LocalStorage/
    GENERAL_DATA.bin          # LE u64 array: account_uid -> index into player_uid space
    USD_{uid}.bin             # PlayerSave protobuf (fields 1-6)
    CALENDAR.bin              # Hadal zone IDs (written on graceful shutdown)
    version/                  # Version marker for dashboard
bin_remielle/server{1,2,3}/Persistent/SDK/
    passwd -> symlink to shared remielle/Persistent/SDK/passwd
bin_remielle_prod/server{1,2,3}/Persistent/LocalStorage/
    ...                       # same layout for prod
configs_remielle/server{1,2,3}/config.zon
    config.zon                # per-server game/ctl bind + base_player_uid
zzz_dump/latest/{en,zh,ko,ja}/   # Beta dump data (latest version)
zzz_dump/live/{en,zh,ko,ja}/     # Prod dump data (live version)
    avatar_details.json       # Character data
    weapon_details.json       # Weapon data
    equip_details.json        # Disc data
    buddy_details.json        # Bangboo data
    boss_details.json         # DA boss data
    shiyu_details.json        # Shiyu data
    .../zzz/UI/               # Referenced game UI assets (shared across versions)
```

**Important:** `base_player_uid` in each server's config.zon determines the UID offset. The editor reads it via `AppState::server_base_uid()` and uses it in `resolve_player_uid` — it must match the server's `base_player_uid` or UID resolution will fail.

---

## Protobuf PlayerSave

`remielle_save.rs` implements a manual protobuf parser (no `prost`/`protoc`). PlayerSave fields:

| Field | Content |
|-------|---------|
| 1 | basic info (optional) |
| 2 | avatar list |
| 3 | weapon list |
| 4 | equip list |
| 5 | buddy list |
| 6 | hall (last city location) |
| 7 | main_city_time |
| 8 | unknown (optional) |

**No hadal_zone field.** DA/Shiyu state is runtime-only in remielle (not persisted). Zone changes go via ctl `modHadalEntrance`.

Functions in `remielle_save.rs` are read-only now — only used for card views. All mutations go through `ctl.rs`.

---

## DA/Shiyu Features (Status Tab + Detail Pages)

### Status tab (`/dashboard?tab=status`)
- Shows 3 cards (Shiyu, Deadly Assault, Deadly Assault Hardcore) for the **currently selected** server only
- Admin users see inline zone ID edit forms that send ctl commands to the selected server's ctl port
- Zone IDs are read from the per-server `CALENDAR.bin` and cached in memory (`ZONE_CACHE`), updated immediately on ctl update

### Admin hadal zone editing
- POST `/admin/update-hadal-zone` — calls `ctl::mod_hadal_entrance()` over UDP to the selected server's ctl address
- No rebuild/restart needed — changes take effect immediately on the running server

---

## Key Conventions

- **The `t()` function** requires locale from `locale_from_headers(&headers)`. Never hardcode labels.
- **All CSS is inline** in main.rs or handler format!() blocks. No .css files.
- **Don't add emojis** unless asked.
- **Don't add comments** unless asked.
- **Build/test**: `cargo build` in `gear_editor/`. Run with `cargo run -r -j1`.
- **i18n keys**: Add to all 5 locale functions in `i18n.rs`.
- **New routes**: Register in `main.rs` Router, add tab link in both `.desktop-tabs` and `.mobile-drawer.tabs`.
- **Data access**: Use `state.dump_lang_dir(locale)` for language-specific dump data. RU files are populated by `translate_dump_to_ru.py` (translated names; DA/Shiyu/stat-name files copied from EN). No code-level EN fallback.
- **Server selection**: Always resolve via `state_with_active_server(&state, &headers)` (or `state_for_selected_server`) so saves, assets, ctl addr, and base_uid are consistent with the selected server.
- **Commit messages**: Short description, blank line, bullet points. **Do not commit/push without asking.**

---

## UI Rules for Status Cards (`.panel .card`)

`.panel a` in main.rs styles ALL `<a>` inside panels as blue buttons. To override for cards:
- Cards use `<div class="card">` with inner `<a>` for the clickable area
- CSS: `.panel .card { background: #1b1f2a; ... }` and `.panel .card a { background: none; ... }`
- Grid: `.panel .cards { grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); }`

