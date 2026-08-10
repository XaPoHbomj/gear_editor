# gear_editor

Web admin panel for the remielle game server. All mutations are sent to a running server via the UDP control protocol (ctl); PlayerSave files are read from disk only for card views.

## Features

| Panel | Edit | Create | Delete | Card view |
|-------|------|--------|--------|-----------|
| Agents (avatars) | Level, exp, rank, talents, skills, skin, awakening, favorite, show-weapon | No | No | Yes |
| W-Engines (weapons) | Level, star, refine | Yes | No | Yes |
| Drive Discs | Main/sub stats, level, star | Yes, single & bulk generate | Yes, single & delete-all-unlocked | Yes |
| DA/Shiyu Status | Zone ID via ctl (admin only) | No | No | Detail view |
| Client Updates | No | Upload patch .zip | Remove | File listing |

All edits require the player to be **online** on the selected server. Mutations are gated on a live presence check (see [Online check](#online-check)) both in the UI (inputs disabled) and server-side on every POST.

## Architecture

```
┌──────────────┐     ┌──────────────┐     ┌─────────────────────────────────┐
│   Browser    │────▶│ gear_editor  │────▶│ remielle gamesv (ctl UDP port)  │
│  (all HTML   │     │ (Rust+Axum)  │     │ modAvatarMeta / createWeapon /  │
│   inline)    │◀────│ localhost:   │     │ modEquip / modHadalZone / etc.  │
│              │     │   3001       │     └─────────────────────────────────┘
└──────────────┘     └──────┬───────┘
                            │ (read-only)
                            ▼
               ┌───────────────────────────────┐
               │ bin_remielle/server{N}/       │
               │ Persistent/LocalStorage/      │
               │   GENERAL_DATA.bin (uid map)  │
               │   USD_{uid}.bin (proto save)  │
               └───────────────────────────────┘
```

- **Rust + Axum 0.7** — no templating engine, all HTML generated via `format!()` in route handlers
- **Vanilla JS** — dropdown cascading, image previews, mobile drawer
- **CSS inline** — no `.css` files; all styles in `<style>` blocks inside handlers
- **Protobuf parser** — custom `src/remielle_save.rs` for reading PlayerSave (read-only)
- **Ctl protocol** — `src/ctl.rs` sends packed UDP packets to each server's control port
- **ZON parser** — `src/zon.rs` for template ZON files
- **5 locales** — EN, RU, ZH, KR, JA (via `gear_lang` cookie, falling back to `Accept-Language`)

### Online check

gear_editor determines player presence by probing each game server's ctl port with a **no-op `createWeapon` packet** (`count = 0`):

- The server replies **ACK** → player is **Online**
- The server replies **NAK** with reason `no_entry` (uid not in the live `uid_map`) → **Offline**
- A silent socket / malformed reply → **Unreachable**

Results are cached for 10 s (`PROBE_CACHE` in `ctl.rs`) and invalidated after any successful ctl mutation. The dashboard probes all servers in parallel and renders a status dot on each server pill (green = online, grey = offline, red = unreachable). Admin-only server-wide operations (Hadal zone edits) use `server_reachable`, which probes with a synthetic uid 0 and treats any reply as "server up".

### Edit flow

All edits go through the ctl UDP protocol immediately. Each save/update button sends the corresponding ctl command to the running server, then a `savePlayer` flush so on-disk saves reflect the change:

| Action | Ctl Command | Target |
|--------|-------------|--------|
| Agent level | `modAvatarMeta` (field=0) | Server's control port |
| Agent exp | `modAvatarMeta` (field=1) | Server's control port |
| Agent rank | `modAvatarMeta` (field=2) | Server's control port |
| Agent talents | `modAvatarMeta` (field=3) | Server's control port |
| Agent mindscape tab | `modAvatarMeta` (field=4) | Server's control port |
| Agent skill | `modAvatarMeta` (field=5, packed skill_id+level) | Server's control port |
| Agent skin | `modAvatarMeta` (field=6) | Server's control port |
| Agent awakening | `modAvatarMeta` (field=7) | Server's control port |
| Agent favorite | `modAvatarMeta` (field=8) | Server's control port |
| Agent show-weapon | `modAvatarMeta` (field=9) | Server's control port |
| Update weapon | `modWeapon` | Server's control port |
| Create weapon | `createWeapon` | Server's control port |
| Update disc | `modEquip` | Server's control port |
| Create disc | `createEquip` (single) / `createEquips` (bulk, up to 49 per packet) | Server's control port |
| Delete disc | `deleteEquip` | Server's control port |
| Change Hadal zone | `modHadalZoneSchedule` (admin) | Server's control port |
| Flush save | `savePlayer` | Server's control port |

Read-only views (cards, status tab) load `USD_{uid}.bin` directly from disk.

Admin auth is validated against `Persistent/SDK/passwd` (same account file as remielle's built-in SDK server). The passwd file contains bcrypt-hashed passwords; admin rights are granted to `ADMIN_LOGIN` in `auth.rs` (default: `XaPoHbomj`).

### Server selection

Beta is consolidated to a **single server instance** (server 1); prod keeps 3 instances. The selected server is stored in the `gear_server` cookie (`beta:N` / `prod:N`, prod clamped to 1-3). Each server has its own save directory and ctl port. Per-server `base_player_uid` is read from `configs_remielle/server{N}/config.zon` (`.base_player_uid`) and must match the server's config or UID resolution will fail.

### Directory layout on disk

```
bin_remielle/server{N}/Persistent/LocalStorage/
  GENERAL_DATA.bin              # LE u64 array: account_uid -> player_uid space index
  USD_{uid}.bin                 # PlayerSave protobuf (read-only)
  CALENDAR.bin                  # Hadal zone IDs (written on graceful shutdown)
  version/                      # Version marker for dashboard
bin_remielle/server{N}/Persistent/SDK/
  passwd                        # Shared account DB with bcrypt passwords (symlinked)
configs_remielle/server{N}/config.zon
                                # Per-server game/ctl bind + .base_player_uid
```

`bin_remielle_prod/server{N}/...` follows the same layout for prod.

## Quick Start

### Prerequisites

- Rust toolchain (edition 2024)
- remielle (built-in sdksv handles account auth)
- Dump data synced via `scripts/sync_zzz_dump_assets.py`

### Environment variables

| Variable | Default | Description |
|----------|---------|-------------|
| `GEAR_EDITOR_ADDR` | `127.0.0.1:3001` | Bind address |
| `GEAR_CTL_ADDRESS` | `127.0.0.1:15811` | Base beta ctl port (server 1); prod server N = port + N - 1 |
| `GEAR_CTL_ADDRESS_PROD` | `127.0.0.1:15911` | Base prod ctl port (server 1) |
| `GEAR_STATE_DIR` | `<root>/bin_remielle` | Base beta per-server save dir (server N under `serverN/Persistent/LocalStorage`) |
| `GEAR_STATE_DIR_PROD` | `<root>/bin_remielle_prod` | Base prod per-server save dir |
| `GEAR_ASSET_DIR` | `<root>/remielle/assets/filecfg` | Game asset filecfg dir (Beta) |
| `GEAR_ASSET_DIR_PROD` | `<root>/remielle_prod/assets/filecfg` | Game asset filecfg dir (Prod) |
| `ZZZ_DUMP_DIR` | `<root>/zzz_dump/latest` | Dump data for item names/icons (Beta) |
| `ZZZ_LIVE_DUMP_DIR` | `<root>/zzz_dump/live` | Dump data for item names/icons (Prod) |
| `GEAR_PASSWD_DIR` | `<root>/remielle/Persistent/SDK` | Directory containing the shared `passwd` file |

`<root>` is resolved automatically as `CARGO_MANIFEST_DIR/..` (the workspace root), so no root env var is needed.

### Build & run

```bash
# Dev build
cargo run

# Release build
cargo run -r -j1

# Or use the provided startup script
bash scripts/start_gear_editor.sh
```

Open `http://127.0.0.1:3001` in a browser.

### Login & registration

Auth is validated against `Persistent/SDK/passwd` (bcrypt). Only the user with login `XaPoHbomj` gets admin rights (see `ADMIN_LOGIN` in `auth.rs`). Sessions persist for 30 days via the `ge_session` cookie.

New accounts can be registered from the login page. Registration RSA-1024-encrypts the credentials (PKCS#1 v1.5, matching the SDK server's private key) and POSTs them to the SDK's `appLoginByPassword` endpoint, which auto-creates the account if it does not exist yet.

## Project Structure

```
src/
  main.rs          # App bootstrap, Router, dashboard HTML, presence probing
  app_state.rs     # AppState, ServerSelection (beta/prod + server_num), cookie parsing, per-server dirs/ports/base_uid
  auth.rs          # Session store, login via bcrypt from remielle SDK/passwd
  sdk.rs           # Account registration: RSA-1024 encrypt + POST to sdksv login endpoint
  assets.rs        # Static file serving (range requests, image cache)
  ctl.rs           # UDP control protocol client (presence probe + mutations)
  i18n.rs          # 5-locale translation table
  player_state.rs  # UID resolution from GENERAL_DATA.bin, PlayerSave load
  remielle_save.rs # Manual protobuf parser for PlayerSave
  updates.rs       # Client updates panel
  utils.rs         # Apply changes, shared CSS, audit log, SVG helpers
  zon.rs           # ZON format parser/serializer
  data/
    mod.rs
    hakushin.rs    # Game data: names, icons from dump directories
    templates.rs   # ZON template loading (via zon_parse_entries)
  domain/
    mod.rs
    discs.rs       # Drive disc stat definitions, validation
  routes/
    mod.rs
    auth.rs        # Login/register pages, login/logout, switch-server
    avatar.rs      # Agent edit, update, cards
    weapon.rs      # Weapon edit/new, update, add, cards
    equip.rs       # Disc edit/new/generate/delete, cards
    challenges.rs  # DA/Shiyu details + status tab
    admin.rs       # Client update upload/delete + hadal zone editing
```

## Performance

- Release profile uses `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `strip = "symbols"`
- Dashboard renders only the active tab server-side (other panels are lazy)
- Gzip compression on all responses (via `tower-http` `CompressionLayer`)
- Images served with `Cache-Control: max-age=604800, immutable`
- Presence probes run in parallel and are cached for 10 s to avoid hammering servers on every page load

## Localization

| Locale | `gear_lang` / `Accept-Language` | Dump source |
|--------|--------------------------------|-------------|
| EN | `en` | nanoka.cc |
| RU | `ru` | honeyhunterworld.net |
| ZH | `zh` | nanoka.cc |
| KR | `ko` | nanoka.cc |
| JA | `ja` | nanoka.cc |

Game data (agent/weapon/disc/bangboo names) is loaded from language-specific JSON dumps under `{dump_dir}/{locale_code}/`. RU names are scraped from honeyhunterworld.net by `translate_dump_to_ru.py`, which also copies untranslatable files (DA/Shiyu, stat names, etc.) from EN — so RU shows EN content for those files. There is no code-level fallback; the RU files are populated on every sync.

## License

MIT
