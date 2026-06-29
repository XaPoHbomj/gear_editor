# gear_editor

Web admin panel for the remielle game server. Reads/writes protobuf `PlayerSave` files directly on disk — no sync layer, no API.

## Features

| Panel | Edit | Create | Delete | Card view |
|-------|------|--------|--------|-----------|
| Agents (avatars) | Level, rank, core skill, talents | No | No | Yes |
| W-Engines (weapons) | Level, rank, stars | Yes | No | Yes |
| Drive Discs | Main/sub stats, level, slot | Yes, seeded single | Yes | Yes |
| Bangboo | Level, rank, stars | No | No | Yes |
| DA/Shiyu Status | Zone ID (admin only) | No | No | Detail view |
| Client Updates | No | Upload patch .zip | Remove | File listing |

## Architecture

```
┌──────────────┐     ┌──────────────┐     ┌────────────────────────────┐
│   Browser    │────▶│ gear_editor  │────▶│ bin_remielle/Persistent/   │
│  (all HTML   │     │ (Rust+Axum)  │     │ LocalStorage/              │
│   inline)    │◀────│ localhost:   │◀────│ USD_{uid}.bin (protobuf)   │
│              │     │   3001       │     └────────────────────────────┘
└──────────────┘     └──────┬───────┘
                            │ (auth)
                     ┌──────▼───────┐
                     │   hoyo-sdk   │
                     │ (SQLite DB)  │
                     └──────────────┘
```

- **Rust + Axum 0.7** — no templating engine, all HTML generated via `format!()` in route handlers
- **Vanilla JS** — <50 lines total for dropdown cascading, image previews, mobile drawer
- **CSS inline** — no `.css` files; all styles in `<style>` blocks inside handlers
- **Protobuf parser** — custom `src/remielle_save.rs` for PlayerSave (fields 1-8)
- **ZON parser** — `src/zon.rs` for server config and template ZON files
- **5 locales** — EN, RU, ZH, KR, JA (auto-detected from `Accept-Language` header)

### Edit flow

Edits are staged in memory (`pending_writes`) and committed to disk in one batch via the "Apply Changes" button. Creations (weapons, discs, bangboos) write immediately.

### Directory layout on disk

```
bin_remielle/Persistent/LocalStorage/
  GENERAL_DATA.bin              # LE u64 array, index i -> player_uid = 666 + i
  USD_{uid}.bin                 # PlayerSave protobuf
configs_remielle/server{1,2,3}/
  config.zon                    # Server config with hadal_zone_entrances
```

## Quick Start

### Prerequisites

- Rust toolchain (nightly 2024 edition)
- [hoyo-sdk](https://git.xeondev.com/reversedrooms/hoyo-sdk) — provides the SQLite DB for login

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `GEAR_EDITOR_ADDR` | `127.0.0.1:3001` | Bind address |
| `HOYO_SDK_CONFIG` | `../hoyo-sdk/sdk_server.toml` | SDK config for login DB |
| `ZZZ_DUMP_DIR` | `../zzz_dump/latest` | Dump data for item names/icons |
| `GEAR_ROOT_DIR` | auto-detected | Workspace root dir |

### Build & run

```bash
# Dev build
cargo run

# Release build
cargo run --release

# Or use the provided startup script
bash scripts/start_gear_editor.sh
```

Open `http://127.0.0.1:3001` in a browser.

### Login

Use the same admin credentials as hoyo-sdk. The username `XaPoHbomj` is admin. Sessions persist for 30 days via `ge_session` cookie.

## Project Structure

```
src/
  main.rs          # App bootstrap, Router, dashboard HTML (~430 lines)
  app_state.rs     # AppState, cookie parsing, version reading
  auth.rs          # Session store, login validation
  config.rs        # SDK config loading
  assets.rs        # Static file serving (range requests, image cache)
  i18n.rs          # 5-locale translation table (~100 keys)
  player_state.rs  # UID resolution from GENERAL_DATA.bin, PlayerSave load/save
  remielle_save.rs # Manual protobuf parser/serializer for PlayerSave
  updates.rs       # Client updates panel
  utils.rs         # Apply changes, shared CSS, SVG helpers
  zon.rs           # ZON format parser/serializer
  data/
    hakushin.rs    # Game data: names, icons from dump directories
    templates.rs   # ZON template loading (via zon_parse_entries)
  domain/
    discs.rs       # Drive disc stat definitions, validation
  routes/
    auth.rs        # Login page, login/logout
    avatar.rs      # Agent edit, update, cards, add-all
    weapon.rs      # Weapon edit/new, update, add, cards
    equip.rs       # Disc edit/new/generate/delete/lock, cards
    bangboo.rs     # Bangboo edit, update, add-all, cards
    challenges.rs  # DA/Shiyu details + status tab
    admin.rs       # Client update upload/delete + hadal zone editing
```

## Performance

- Release profile uses `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = "symbols"`
- Dashboard renders only the active tab server-side (other panels are lazy)
- Gzip compression on all responses (via `tower-http`)
- Images served with `Cache-Control: max-age=604800, immutable`

## Localization

| Locale | `Accept-Language` | Dump source |
|--------|-------------------|-------------|
| EN | `en` | nanoka.cc |
| RU | `ru` | honeyhunterworld.net |
| ZH | `zh` | nanoka.cc |
| KR | `ko` | nanoka.cc |
| JA | `ja` | nanoka.cc |

Game data (agent/weapon/disc/bangboo names) is loaded from language-specific JSON dumps under `{dump_dir}/{locale_code}/`. RU falls back to EN for missing data.

## License

MIT
