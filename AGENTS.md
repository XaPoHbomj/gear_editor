# Gear Editor — AGENTS.md

## Project Overview

A web-based player profile/state editor for Zenless Zone Zero private server. It reads and writes ZON-format state files directly on disk, shared with the Yoshunko game server process.

**Tech stack:** Rust + Axum 0.7, no templating engine (all HTML generated inline via `format!()`), vanilla JS only.

---

## Key Architectural Decisions

### Why inline HTML templates (no Tera/Hadron/etc)?
The project started with server-side `format!()` and never needed more. Each page is small (1-2 screens), and using `format!()` means no build-time template compilation, no template directories, and everything is in one place. All HTML lives inside handler functions in `src/routes/*.rs`.

### Why no JavaScript framework?
All interactions are form POST/GET. The only dynamic UI is:
- Substat cascading dropdowns (disc edit/new pages — ~30 lines of vanilla JS)
- Image preview on select change (weapon/disc creation — ~10 lines)
- Mobile drawer toggle (native DOM API)

### Why ZON files and not a database?
The Yoshunko game server stores player state as flat ZON files (a Zig object notation). The gear_editor reads/writes the same files directly, avoiding any sync layer or API.

### Why pending_writes pattern?
Edits are staged in an in-memory `HashMap<PathBuf, String>` and written to disk in one batch via `/apply`. This allows the user to make multiple edits before committing. Direct writes (for creation) bypass this and write immediately.

### Why global mutable session store?
A `OnceLock<Mutex<HashMap<String, Session>>>` holds all active sessions. This is acceptable because gear_editor is a single-user admin tool (not a multi-tenant service).

---

## Project Structure

```
gear_editor/
├── AGENTS.md           # This file
├── BRANDBOOK.md        # UI design documentation
├── BRANDBOOK.html      # Visual HTML reference of all UI elements
├── Cargo.toml          # Dependencies: axum, tokio, serde, rusqlite, etc.
└── src/
    ├── main.rs         # App bootstrap, Router setup, dashboard HTML template (~385 lines)
    ├── app_state.rs    # AppState struct, ServerMode enum, cookie parsing
    ├── auth.rs         # Session store, login validation, HTML escaping
    ├── config.rs       # SDK config loading (TOML)
    ├── assets.rs       # Static file serving (range requests)
    ├── i18n.rs         # 5-locale translation (~100 keys x 5 = 500 strings)
    ├── player_state.rs # UID resolution, next-uid counter, stat select rendering
    ├── updates.rs      # Client updates panel (patch/update file listing)
    ├── utils.rs        # apply_changes handler, svg_data_uri helper
    ├── zon.rs          # ZON format parser/serializer (~744 lines)
    ├���─ data/
    │   ├── mod.rs
    │   ├── hakushin.rs # Hakushin.gg dump data: char/weapon/disc/bangboo names + images
    │   └── templates.rs# Game template JSON loading (Avatar, Weapon, Equipment)
    ├── domain/
    │   ├── mod.rs
    │   └── discs.rs    # Disc stat definitions, main/sub stat options, base values
    └── routes/
        ├── mod.rs
        ├── auth.rs     # Login, login page, server switch
        ├── avatar.rs   # Character edit, update, card rendering, add-all
        ├── weapon.rs   # Weapon edit/new/update/add, card rendering
        ├── equip.rs    # Disc edit/new/generate/delete/lock, card rendering
        ├── bangboo.rs  # Bangboo edit, update, card rendering, add-all
        └── challenges.rs # Deadly Assault & Shiyu detail panels
```

---

## How the App Works

1. **Login** — User authenticates via `/login` (pbkdf2 against hoyo-sdk SQLite DB). Session created with random 48-char ID stored in `ge_session` cookie.

2. **Dashboard** — `GET /dashboard?tab={tab}` renders the full HTML page with header, tabs, and content panel. Tab query param switches between: `avatars` (default), `weapons`, `discs`, `bangboos`, `da`, `shiyu`, `updates`.

3. **Tab content** — Each tab renders cards via a `render_*_cards()` function that reads ZON files from `{state_dir}/player/{uid}/{entity_kind}/`.

4. **Editing** — Clicking a card opens `/entity/:id` edit page. Changes go to `session.pending_writes`, applied via `/apply`.

5. **Creation** — New weapons/discs/avatars/bangboos are written directly to disk (no pending_writes).

6. **Server switching** — `gear_server` cookie toggles between `beta`/`prod`. Pending writes are cleared on switch.

---

## Key Data Flow

### State directory layout
```
{state_dir}/player/{player_uid}/
    account/{uid}               # Account -> player_uid mapping
    avatar/{avatar_id}          # ZON file, no extension
    weapon/{weapon_uid}         # ZON file, no extension
    equip/{equip_uid}           # ZON file, no extension
    buddy/{bangboo_uid}         # ZON file, no extension
    hadal_zone/info             # Shiyu/Deadly Assault progress
    weapon/next                 # Auto-increment counter
    equip/next                  # Auto-increment counter
```

### How entities are read
1. `resolve_player_uid()` maps account UID → player UID
2. Each entity has a directory: `player/{uid}/avatar/`, `player/{uid}/weapon/`, etc.
3. `read_zon()` parses the ZON file into a `ZValue` enum
4. `zon_get_number()`, `zon_get_array_numbers()`, etc. extract fields

### How entities are written
- **Creations** (weapon_add, equip_add, avatar_add_all, bangboo_add_all): `fs::write()` directly to disk
- **Edits** (avatar_update, weapon_update, etc.): `session.pending_writes.insert(path, serialized)` → later flushed by `/apply`

---

## Adding a New Feature

1. **New translation keys** → Add to all 5 locale functions in `i18n.rs`
2. **New route handler** → Add to appropriate file in `src/routes/`
3. **New route** → Register in `main.rs` Router
4. **New dashboard tab** → Add tab link in both `.desktop-tabs` and `.mobile-drawer.tabs` in `main.rs`, add tab detection + content match arm
5. **Panel with buttons** → Use `<div class="panel">` pattern (see BRANDBOOK.md/BRANDBOOK.html)
6. **Form page** → Use `.container` + `.row` grid + standard form elements (see existing edit/new pages for reference)

---

## UI Conventions

Full design system documented in `BRANDBOOK.md` and `BRANDBOOK.html`.

- **Colors**: Dark theme (`#0f1115` bg, `#e6e6e6` text), blue primary (`#4c7dff`), green apply (`#22c55e`), red danger (`#ef4444`)
- **Panels**: `.panel` with `<h3>` on left, `<div style="display:flex; gap:8px;">` on right with action buttons
- **Cards**: `.cards` grid with `.card` items, each containing `.thumb` image, `.pill` badge, `h3` title, `.meta` lines
- **Forms**: `.row` 2-column grid, labels at `12px #9aa4b2`, inputs at `#121620` bg with `#2a3140` border
- **Breakpoint**: 768px — panels stack vertically, cards become 1-column, hamburger menu appears
- **Preview images**: `.preview-img` class, hidden until selection, 33.33% width desktop / 100% mobile, left-aligned

---

## Notes for Agents

- **Don't add emojis** unless explicitly asked.
- **Don't add comments** to code unless asked.
- **All CSS is inline** in `main.rs` (dashboard) or inside each handler's `format!()` (sub-pages). There are no `.css` files.
- **Use the same variable names** as existing code for consistency (e.g., `locale`, `state`, `active_state`, `hakushin`).
- **The `t()` function** requires the locale from `locale_from_headers(&headers)`.
- **Cache-aware**: `load_hakushin_data()` caches results; it's cheap to call multiple times.
- **Never SCP/SSH** to remote without explicit permission.
- **Commit messages** should follow the existing convention: short description, then blank line, then bullet points.
