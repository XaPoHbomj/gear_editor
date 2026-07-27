# gear_editor

Web admin panel for the remielle game server. All mutations are sent to a running server via UDP control protocol (ctl). PlayerSave files are only read for card views.

## Features

| Panel | Edit | Create | Delete | Card view |
|-------|------|--------|--------|-----------|
| Agents (avatars) | Level, rank, core skill, talents | Add All | No | Yes |
| W-Engines (weapons) | Level, star, refine | Yes | Yes | Yes |
| Drive Discs | Main/sub stats, level, star | Yes, single & bulk generate | Yes | Yes |
| Bangboo | Level, rank, stars | No | No | Yes |
| DA/Shiyu Status | Zone ID via ctl (admin only) | No | No | Detail view |
| Client Updates | No | Upload patch .zip | Remove | File listing |

## Architecture

```
┌──────────────┐     ┌──────────────┐     ┌─────────────────────────────────┐
│   Browser    │────▶│ gear_editor  │────▶│ remielle gamesv (ctl UDP port)  │
│  (all HTML   │     │ (Rust+Axum)  │     │ modAvatarMeta / createWeapon /  │
│   inline)    │◀────│ localhost:   │     │ modEquip / deleteEquip / etc.   │
│              │     │   3001       │     └─────────────────────────────────┘
└──────────────┘     └──────┬───────┘
                             │ (read-only)
                             ▼
                     ┌───────────────────────┐
                     │ Persistent/LocalStorage│
                     │ USD_{uid}.bin (proto) │
                     └───────────────────────┘
```

- **Rust + Axum 0.7** — no templating engine, all HTML generated via `format!()` in route handlers
- **Vanilla JS** — <50 lines total for dropdown cascading, image previews, mobile drawer
- **CSS inline** — no `.css` files; all styles in `<style>` blocks inside handlers
- **Protobuf parser** — custom `src/remielle_save.rs` for reading PlayerSave
- **Ctl protocol** — `src/ctl.rs` sends packed UDP packets to the server control port
- **ZON parser** — `src/zon.rs` for template ZON files
- **5 locales** — EN, RU, ZH, KR, JA (auto-detected from `Accept-Language` header)

### Edit flow

All edits go through ctl UDP protocol immediately. Each save/update button sends the corresponding ctl command to the running server:

| Action | Ctl Command | Target |
|--------|-------------|--------|
| Level up | `modAvatarMeta` (field=0) | Server's control port |
| Rank up | `modAvatarMeta` (field=2) | Server's control port |
| Skill edit | `modAvatarMeta` (field=5, packed skill_id+level) | Server's control port |
| Update weapon | `modWeapon` | Server's control port |
| Create weapon | `createWeapon` | Server's control port |
| Delete weapon | `deleteWeapon` | Server's control port |
| Update disc | `modEquip` | Server's control port |
| Create disc | `createEquip` | Server's control port |
| Delete disc | `deleteEquip` | Server's control port |
| Change zone | `modHadalEntrance` | Server's control port |

Read-only views (cards, status tab) load `USD_{uid}.bin` directly from disk.

Admin auth is validated against `Persistent/SDK/passwd` (same account file as remielle's built-in SDK server). The passwd file contains bcrypt-hashed passwords; admin rights are granted by the `ADMIN_LOGIN` constant in `auth.rs` (default: `XaPoHbomj`).

### Directory layout on disk

```
bin_remielle/Persistent/
  LocalStorage/
    GENERAL_DATA.bin              # LE u64 array, index i -> player_uid found by file scan
    USD_{uid}.bin                 # PlayerSave protobuf (read-only)
    version/                      # Version marker for dashboard
  SDK/
    passwd                        # Account DB with bcrypt passwords
```

## Quick Start

### Prerequisites

- Rust toolchain (nightly 2024 edition)
- remielle (built-in sdksv handles account auth)

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `GEAR_EDITOR_ADDR` | `127.0.0.1:3001` | Bind address |
| `GEAR_CTL_ADDRESS` | `127.0.0.1:15811` | Target server ctl port for mutations |
| `GEAR_STATE_DIR` | `../remielle/Persistent/LocalStorage` | Player save directory |
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

Auth is validated against `Persistent/SDK/passwd` (bcrypt). Only the user with login `XaPoHbomj` gets admin rights. Sessions persist for 30 days via `ge_session` cookie. Add more admins by adding their username to the `ADMIN_LIST` constant in `auth.rs`.

## Project Structure

```
src/
  main.rs          # App bootstrap, Router, dashboard HTML (~430 lines)
  app_state.rs     # AppState, cookie parsing, version reading
  auth.rs          # Session store, login via bcrypt from remielle SDK/passwd
  ctl.rs           # UDP control protocol client (9 commands)
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
