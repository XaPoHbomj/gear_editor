# Gear Editor — AGENTS.md

## Project Overview

Web admin panel for the remielle game server. Reads/writes protobuf `PlayerSave` files, reads server config ZON files, and serves game data from `zzz_dump/latest/`.

**Tech stack:** Rust + Axum 0.7, inline `format!()` HTML, vanilla JS.

**Branch:** `remielle-support` — single-server, no beta/prod switching.

---

## Project Structure

```
gear_editor/
└── src/
    ├── main.rs         # Router, dashboard HTML with inline CSS
    ├── app_state.rs    # AppState, cookie parsing, version from state_dir/version/
    ├── auth.rs         # Session store, login (pbkdf2 against hoyo-sdk DB)
    ├── assets.rs       # Static file serving from zzz_dump/assets/
    ├── i18n.rs         # 5-locale translation table (EN, RU, CN, KR, JP)
    ├── player_state.rs # UID resolution from GENERAL_DATA.bin, PlayerSave load/save
    ├── remielle_save.rs# Manual protobuf parser/serializer (~660 lines)
    ├── updates.rs      # Client updates panel (upload/delete/browse)
    ├── utils.rs        # apply_changes, shared_page_css, svg_data_uri
    ├── zon.rs          # ZON format parser: read_zon, zon_parse_entries
    ├── data/
    │   ├── hakushin.rs # Hakushin.gg dump: char/weapon/disc/bangboo names+images
    │   └── templates.rs# Template JSON via zon_parse_entries (ZON format)
    └── routes/
        ├── auth.rs     # Login page
        ├── avatar.rs   # Character edit/update/cards/add-all (protobuf PlayerSave)
        ├── weapon.rs   # Weapon edit/new/update/add (protobuf PlayerSave)
        ├── equip.rs    # Disc edit/new/generate/delete/lock/filter (protobuf PlayerSave)
        ├── bangboo.rs  # Bangboo edit/update/cards/add-all (protobuf PlayerSave)
        ├── challenges.rs # DA/Shiyu detail pages + status tab
        └── admin.rs    # Client update upload/delete + hadal zone editing
```

---

## State Directory Layout (Remielle)

```
bin_remielle/Persistent/LocalStorage/
    GENERAL_DATA.bin          # LE u64 array: index i -> player_uid = 666 + i
    USD_{uid}.bin             # PlayerSave protobuf (fields 1-8)
configs_remielle/server{1,2,3}/
    config.zon                # Server config with hadal_zone_entrances
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

## Protobuf PlayerSave (Most Important)

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

**No hadal_zone field.** DA/Shiyu state is runtime-only in remielle (not persisted).

Key functions in `remielle_save.rs`:
- `parse_player_save(data: &[u8]) -> PlayerSave`
- `serialize_player_save(save: &PlayerSave) -> Vec<u8>`

---

## DA/Shiyu Features (Status Tab + Detail Pages)

### Status tab (`/dashboard?tab=status`)
- Reads `configs_remielle/server{1,2,3}/config.zon` via `extract_entrance_zone()`
- Shows 3 card panels per server: Shiyu, Deadly Assault, Deadly Assault Hardcore
- Cards use labels (not zone names) as titles
- Admin users see inline zone ID edit forms that call `scripts/update_hadal_zone.sh`

### Detail pages (`/da/:id`, `/shiyu/:id`)
- Read-only, no selection/writing
- DA: shows boss cards with HP/BaseHP/ATK/DEF/Stun, weakness/resistance from `monster.element` (lowercase keys), layer buffs
- Shiyu: floor tabs, ordered rooms with monster cards sorted by HP desc, buffs from `layer_buff`
- Element icons: `IconFire.webp` etc. (not `Sprite/Element_Fire.webp`)

### Admin hadal zone editing
- POST `/admin/update-hadal-zone` — runs `scripts/update_hadal_zone.sh <server> <hadal_id> <new_zone>`
- Script stops the server, updates config.zon, rebuilds and relaunches
- Only visible to admin users (checked via `is_admin()`)

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
