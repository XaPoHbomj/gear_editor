# Gear Editor — UI Brandbook

## Color Palette

| Token | Hex | Usage |
|-------|-----|-------|
| `bg-page` | `#0f1115` | Page body background |
| `bg-header` | `#151a24` | Header bar |
| `bg-card` | `#1b1f2a` | Cards and panels |
| `bg-card-hover` | `#232a38` | Card/panel border |
| `bg-tab-inactive` | `#1b2230` | Tab button (inactive) |
| `bg-tab-active` | `#4c7dff` | Tab button (active), primary button |
| `bg-input` | `#121620` | Input/select fields |
| `bg-apply` | `#22c55e` | "Apply changes" button |
| `bg-danger` | `#ef4444` | Delete/danger buttons |
| `text-primary` | `#e6e6e6` | Body / heading text |
| `text-meta` | `#9aa4b2` | Secondary text (labels, metadata) |
| `text-tab-inactive` | `#c7d1e0` | Tab text when not selected |
| `border-default` | `#2a3140` | Borders on inputs, cards, panels |
| `border-panel` | `#232a38` | Panel/card border |

## CSS Architecture

All styles are **inlined in `main.rs`** inside the dashboard HTML template.
Sub-pages (edit/new forms) have their own `<style>` block, but always share the
same base variables listed above.

### Global (dashboard)

```css
body {
  font-family: system-ui, sans-serif;
  background: #0f1115;
  color: #e6e6e6;
  overflow-x: hidden;
}
```

### Layout Components

#### `.container`
Standard page wrapper:
```css
padding: 20px 24px 40px;
max-width: 900px;           /* sub-pages */
margin: 0 auto;
```
Dashboard `.container` has no `max-width`/`margin` — only `padding`.

Use `.container` for all full-page forms (edit/new/generate).
Dashboard content sits directly inside `<div class="container">{content}</div>`.

#### Header (`header`)
```css
padding: 16px 24px;
display: flex;
justify-content: space-between;
align-items: center;
gap: 12px;
background: #151a24;
position: sticky;
top: 0;
z-index: 20;
```

#### `.tabs`
```css
display: flex;
flex-wrap: wrap;
gap: 8px;
```
Tab links use `.tabs a` with `padding: 8px 12px; border-radius: 8px`.
Active tab → class `active` applied.

#### `.panel`
Action bar above card grids. Used on dashboard tabs (weapons, discs, characters, bangboos):
```css
background: #1b1f2a;
padding: 14px;
border-radius: 12px;
border: 1px solid #232a38;
margin-bottom: 16px;
display: flex;
align-items: center;
justify-content: space-between;
gap: 12px;
```

**Always wrap action buttons in** `<div style="display:flex; gap:8px;">`
to keep them vertically centered and avoid the `margin-top: 16px` from `.panel a`

Content inside a panel:
- `<h3>` — left-aligned title, `margin: 0; font-size: 14px`
- `<div style="display:flex; gap:8px;">` — right-aligned button group
  - Buttons/links inside this div get `margin-top: 0` (via `.panel div button, .panel div a`)

#### `.hero`
Header section for edit/detail pages (weapon edit, disc edit, character edit):
```css
display: flex;
gap: 16px;
align-items: center;
margin-bottom: 16px;
```
Left side: `<img>` (120x120px thumbnail).
Right side: `<div>` with `<h1>` title and `.meta` info.

#### `.cards`
Card grid for listing items (characters, weapons, discs, bangboos):
```css
display: grid;
grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
gap: 14px;
```

#### `.card`
Individual card item:
```css
background: #1b1f2a;
padding: 14px;
border-radius: 12px;
text-decoration: none;
color: #e6e6e6;
border: 1px solid #232a38;
```
Card inner elements:
- `.thumb` — image (width 100%, height 160px, `object-fit: cover`)
- `.pill` — tag/badge (`padding: 4px 8px; background: #2a3140; border-radius: 999px; font-size: 12px`)
- `h3` — name (`margin: 6px 0 8px; font-size: 16px`)
- `.meta` — metadata line (`color: #9aa4b2; font-size: 12px`)

### Form Elements

#### Inputs & Selects
```css
width: 100%;
box-sizing: border-box;
padding: 8px;
border-radius: 8px;
border: 1px solid #2a3140;
background: #121620;
color: #e6e6e6;
```

#### Labels
```css
display: block;
margin: 12px 0 6px;
font-size: 12px;
color: #9aa4b2;
```

#### Primary Button
```css
padding: 10px 14px;
border: 0;
border-radius: 8px;
background: #4c7dff;
color: #fff;
font-weight: 600;
cursor: pointer;
font-family: inherit;
font-size: inherit;
```
In `.panel`, buttons inside the flex div get `margin-top: 0`.

#### `.row`
Two-column grid for form fields:
```css
display: grid;
grid-template-columns: repeat(2, minmax(0, 1fr));
gap: 12px;
```

#### `.apply` (dashboard Apply changes button)
```css
background: #22c55e;
color: #0b1220;
border: 0;
padding: 8px 14px;
border-radius: 8px;
font-weight: 600;
cursor: pointer;
```

#### `.danger`
```css
background: #ef4444;
color: #fff;
border: 0;
padding: 8px 14px;
border-radius: 8px;
font-weight: 600;
cursor: pointer;
```

### Responsive Design

Breakpoint: `max-width: 768px`

| Element | Desktop | Mobile |
|---------|---------|--------|
| `.container` padding | `20px 24px` | `14px` |
| `.cards` | `repeat(auto-fill, minmax(220px, 1fr))` | `1fr` |
| `.row` | 2 columns | 1 column |
| `.panel` | `flex-direction: row` | `flex-direction: column; align-items: stretch` |
| `.panel a, .panel button` | auto width | `width: 100%` |
| header padding | `16px 24px` | `12px 14px` |
| `.lang-select` | visible | hidden |
| `.menu-button` | hidden | visible |
| `.desktop-tabs` | visible | hidden |
| `.card` padding | `14px` | `12px` |
| `.meta` | normal | `word-break: break-word` |

### Preview Image Pattern

Used on weapon/disc creation/generation pages. A responsive preview thumbnail
that appears when a selection is made in the combo box:

```css
.preview-img {
  display: none;                    /* hidden until selection */
  width: 33.33%;                    /* 1/3 of parent on desktop */
  aspect-ratio: 1/1;
  object-fit: contain;
  border-radius: 8px;
  border: 1px solid #2a3140;
  background: #0f1115;
  margin: 0 0 8px;                  /* left-aligned */
}
@media (max-width: 768px) {
  .preview-img { width: 100%; }
}
```

Place the `<img>` **before the `<label>`** in the HTML. The JS pattern:

```js
const images = {JSON_MAP};
const preview = document.getElementById("preview_id");
const select = document.getElementById("select_id");
select.addEventListener("change", function() {
  const url = images[select.value];
  if (url) { preview.src = url; preview.style.display = "block"; }
  else { preview.style.display = "none"; }
});
```

### JavaScript Patterns

#### Substat Cascading (used in disc new/edit pages)
1. Server builds JSON maps of `mainOptionsBySlot`, `subOptionsByMain`, `statLabels`.
2. Embedded in `<script>` as JS constants.
3. `change` listeners on slot select → update main stat options.
4. `change` listener on main stat select → update sub stat options.

#### Image Preview (used in weapon/disc creation/generation)
1. Server builds JSON map of `{itemId: imageUrl}` from hakushin data.
2. Embedded in `<script>` as a JS constant.
3. `change` listener on item select → update `<img>` src and visibility.

### Page Types

| Type | Description | Files |
|------|-------------|-------|
| **Dashboard** | Tabbed view with card grids | `main.rs` → `dashboard()` |
| **Edit page** | Full-page form to edit one item | `weapon_edit`, `equip_edit`, `avatar_edit`, `bangboo_edit` |
| **New/Create page** | Full-page form to create one item | `weapon_new`, `equip_new` |
| **Generate page** | Full-page form to create multiple items | `equip_generate` |
| **Detail page** | Read-only view of game data | `da_detail`, `shiyu_detail` |

### Adding a New Dashboard Tab

1. Add translation keys in `i18n.rs` (all 5 locales) for: `nav.{tab}`, tab-specific labels.
2. Add the tab link in both `.desktop-tabs` and `.mobile-drawer.tabs` in `main.rs`.
3. Add tab detection: `tab_{name} = if tab == "{name}" { "active" } else { "" }`.
4. Add content rendering in the `match tab.as_str()` block.
5. Add route in `main.rs` Router.
6. Create handler module in `src/routes/`.

### Adding a Panel with Action Buttons

Pattern (used by weapons, discs, characters, bangboos tabs):

```rust
fn render_add_something_panel(locale: Locale) -> String {
    format!(
        "<div class=\"panel\"><h3>{}</h3><div style=\"display:flex; gap:8px;\">{buttons}</div></div>",
        t(locale, "nav.something"),
    )
}
```

For a link button: `<a href="/something/new">{label}</a>`
For a form submit: `<form method="post" action="/something/add-all" style="margin:0;"><button type="submit">{label}</button></form>`

Wrap buttons in `<div>` to trigger `margin-top: 0` cleanup.
