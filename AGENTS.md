# Gear Editor — AGENTS.md

## Project Overview

Web admin panel for the remielle game server. All mutations (avatar, weapon, equip edits/creates/deletes) are sent to a running server via UDP control protocol (ctl). The old file-writing approach has been replaced by ctl commands.

**Tech stack:** Rust + Axum 0.7, inline `format!()` HTML, vanilla JS.

**Branch:** `remielle-support` — single-server, no beta/prod switching.

---

## Project Structure

```
gear_editor/
└── src/
    ├── main.rs         # Router, dashboard HTML with inline CSS
    ├── app_state.rs    # AppState, cookie parsing, version from state_dir/version/
    ├── auth.rs         # Session store, login via bcrypt from remielle SDK/passwd
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
        ├── auth.rs     # Login page
        ├── avatar.rs   # Character edit/update/cards/add-all (ctl modAvatarMeta)
        ├── weapon.rs   # Weapon edit/new/update (ctl create/mod/deleteWeapon)
        ├── equip.rs    # Disc edit/new/generate/delete (ctl create/mod/deleteEquip)
        ├── bangboo.rs  # Bangboo edit/update/cards/add-all (file writes)
        ├── challenges.rs # DA/Shiyu detail pages + status tab
        └── admin.rs    # Client update upload/delete + hadal zone editing (ctl modHadalEntrance)
```

---

## State Directory Layout (Remielle)

```
bin_remielle/Persistent/LocalStorage/
    GENERAL_DATA.bin          # LE u64 array: index i -> player_uid found by file scan or 1+i
    USD_{uid}.bin             # PlayerSave protobuf (fields 1-6)
    version/                  # Version marker for dashboard
bin_remielle/Persistent/SDK/
    passwd                    # Account DB: count(u64) + names(32B each) + tokens(64B) + bcrypt hashes(257B)
configs_remielle/server{1,2,3}/
    config.zon                # Server config (game_bind_address, ctl_bind_address — no hadal zones)
zzz_dump/latest/{en,zh,ko,ja}/
    avatar_details.json       # Character data
    weapon_details.json       # Weapon data
    equip_details.json        # Disc data
    buddy_details.json        # Bangboo data
    boss_details.json         # DA boss data
    shiyu_details.json        # Shiyu data
    .../zzz/UI/               # Referenced game UI assets
```

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
- Shows 3 card panels per server: Shiyu, Deadly Assault, Deadly Assault Hardcore
- Admin users see inline zone ID edit forms that send ctl commands
- Zone IDs are runtime variables on the server (not persisted in config) — set via ctl `ModHadalZoneSchedule`

### Admin hadal zone editing
- POST `/admin/update-hadal-zone` — calls `ctl::mod_hadal_entrance()` directly over UDP
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
- **Data access**: Use `state.dump_lang_dir(locale)` for language-specific dump data; RU falls back to EN.
- **Commit messages**: Short description, blank line, bullet points. **Do not commit/push without asking.**

---

## UI Rules for Status Cards (`.panel .card`)

`.panel a` in main.rs styles ALL `<a>` inside panels as blue buttons. To override for cards:
- Cards use `<div class="card">` with inner `<a>` for the clickable area
- CSS: `.panel .card { background: #1b1f2a; ... }` and `.panel .card a { background: none; ... }`
- Grid: `.panel .cards { grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); }`
