# TODO — gear_editor Next Steps

## 1. Fix garbled Chinese characters in zh locale ✔️
**Files:** `src/i18n.rs:647,653`

- `da.selectable_buffs`: `"可选增益"` ✅
- `shiyu.buffs`: `"增益"` ✅

## 2. Fix da-shiyu form redirect ✔️
**Files:** `src/routes/challenges.rs:59`

`da_shiyu_update` redirects to `/dashboard?tab=da-shiyu`, but no such tab exists — falls through to `_ => avatar_cards`. Changed to `/dashboard?tab=shiyu`. ✅

## 3. Add pagination to disc inventory ✔️
**Files:** `src/routes/equip.rs`, `src/main.rs:TabQuery`

Added `page` query param (default 1, 50 per page), slice rendered cards, prev/next pagination links at bottom with "Showing X–Y / Z" counter. Preserves filter params in pagination links. ✅

## 4. Add logout endpoint ✔️
**Files:** `src/routes/auth.rs`, `src/auth.rs`, `src/main.rs`

- Added `GET /logout` — reads `ge_session` cookie, removes session from `SESSION_STORE`, sets cookie with `Max-Age=0`, redirects to `/` ✅
- Added `remove_session()` to `auth.rs` ✅

## 5. Add 1-week persistent session cookie ✔️
**Files:** `src/routes/auth.rs:118-124`

Added `; Max-Age=604800` to `ge_session` cookie in login handler. Cookie persists 7 days instead of clearing on browser close. ✅

## 6. Add session GC for abandoned sessions ✔️
**Files:** `src/auth.rs`, `src/main.rs`

- Added `last_active: Instant` to `Session` struct, updated on every `get_session()` lookup ✅
- Added `gc_sessions(max_age)` that removes sessions with no activity in the given duration ✅
- Background task in `main.rs` runs every hour, removes sessions inactive > 24h ✅

## 7. Add audit log to `logs/` folder ✔️
**Files:** `src/utils.rs`, `src/routes/equip.rs`, `avatar.rs`, `weapon.rs`, `bangboo.rs`, `admin.rs`, `auth.rs`

Added `audit_log()` in `utils.rs` — appends JSONL to `logs/gear_editor_audit.log`. Logs login, logout, apply_changes, admin upload/delete, weapon/disc/equip creates, avatar/bangboo bulk add, disc deletes and generates. Format: `{"ts":"...","user":"...","uid":1,"action":"...","detail":"..."}`. ✅

## 8. Add disc search/filter UI ✔️
**Files:** `src/routes/equip.rs`, `src/main.rs:TabQuery`, `src/i18n.rs`

Inline filter panel above disc cards with dropdowns for: set (all sets default), slot (all slots), main stat (all stats), lock status (all/locked/unlocked). Uses `onchange="this.form.submit()"` for instant filtering. 12 i18n keys added in all 5 locales. ✅

## 9. Share CSS via helper function ✔️
**Files:** `src/utils.rs`, `src/routes/avatar.rs`, `weapon.rs`, `equip.rs`, `bangboo.rs`

Extracted duplicated inline CSS into `shared_page_css()` in `utils.rs`. All edit/new pages reference `{shared_css}`. Shared CSS includes body, container, inputs, labels, buttons, row, hero, meta, preview-img, and mobile @media rules. ✅

---

## Execution Order

| # | Task | Effort |
|---|------|--------|
| 1 | Fix Chinese garbled chars | trivial |
| 2 | Fix da-shiyu redirect | trivial |
| 4+5 | Add logout + persistent session | small |
| 9 | Share CSS via helper | small |
| 7 | Add audit log | medium |
| 8 | Add disc search/filter | medium |
| 3 | Add pagination | medium |
| 6 | Add session GC | small |
