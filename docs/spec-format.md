# UI spec format

The same YAML spec has two independent consumers:

- **`nxtft compile`** patches a **scaffold** `.tft` (see
  [`docs/formats/nextion-tft-format.md`](formats/nextion-tft-format.md) §6
  for what a scaffold is and why this toolkit doesn't synthesize a `.tft`
  from nothing) so it matches the spec's *desired* state for each named
  component. It never adds, removes, or renames components — only text,
  geometry, and color/font (for the confirmed component types — see
  "Limitations" below). It reads each component's `type` from the
  scaffold `.HMI`, so a spec's own `type` field is optional noise as far
  as `compile` is concerned.
- **`nxtft render-html`** renders the spec directly as a static HTML page
  (see the README) with no scaffold at all. Since it has no `.HMI` to
  read a component's type from, it needs the spec's own `type`/`bco`
  fields to render buttons and hotspots correctly.

## Top-level shape

```yaml
target: NX8048P050-011R-Y   # must match docs/targets.md's one supported target
page: page1                  # documentation only today -- see "Limitations" below
components:
  - objname: textGear         # must exist in the scaffold .HMI (for compile), same spelling
    type: t                   # optional; ignored by compile, used by render-html (t/b/m)
    x: 90                     # optional -- omit to leave unchanged
    y: 22
    w: 180
    h: 60
    txt: "ON!"                # optional -- omit to leave unchanged
    pco: 0x049f               # optional, RGB565 text color -- 't' components only
    bco: 0x0640               # optional, RGB565 background color -- 'b' components only
    font: 5                   # optional, font id already present in the scaffold
```

Every field except `objname` is optional. `compile` diffs each field
against the scaffold's current value (read from the scaffold `.HMI`, not
the `.tft` — see below) and only writes bytes for fields that actually
changed; an unset field is left exactly as the scaffold had it.

## Why `compile` needs both a scaffold `.HMI` and a scaffold `.tft`

`.tft` doesn't carry `objname` (see the format doc) — there's no way to
look up "the component named `textGear`" in a compiled file directly.
`compile` resolves that by decoding the scaffold's own `.HMI` (which still
has `objname`s) to find each spec component's *current* type, geometry,
and text, then locates the matching record in the scaffold `.tft` by
searching for that current geometry — exactly the manual workflow
`nextion-tft-format.md` §6 describes, automated.

This means the scaffold `.HMI` and scaffold `.tft` **must be the exact
same compiled project** — a `.tft` compiled from a different `.HMI` (even
a very similar one) will have different byte content, and `compile` will
either fail to find a component's geometry or, worse, patch the wrong one.

## Fields

| field | type | effect |
|---|---|---|
| `objname` | string, required | looked up in the scaffold `.HMI` (`compile`); error if absent |
| `type` | string (`t`/`b`/`m`), optional | ignored by `compile` (read from the scaffold `.HMI` instead); used by `render-html` to choose styling and text-vs-objname labeling, since it has no scaffold to read type from |
| `x`, `y`, `w`, `h` | integer, optional | if any differs from the scaffold's current value, `tft::patch_geom` rewrites the whole quad plus `endx`/`endy` |
| `txt` | string, optional | if it differs from the scaffold's current text, `tft::patch_text` rewrites the text-pool slot; must not be longer than the scaffold's current text (see the format doc's slot-size caveat) |
| `pco` | integer (RGB565), optional | **only valid when the scaffold's `type` for this component is `t`** — see below |
| `bco` | integer (RGB565), optional | background color, normal state — **only valid when the scaffold's `type` for this component is `b`** |
| `font` | integer (font id), optional | same restriction as `pco`/`bco` depending on type; refers to a font id already compiled into the scaffold, never new font data |

## Limitations (read before relying on this)

- **Color/font patching is restricted by confirmed record layout, per
  type.** `pco`/`font` require the scaffold's `type` to be `t`; `bco`/
  `font` require `type` to be `b`. Setting a color/font field on the
  wrong type (or on `m`, which has no compiled color/font record at all
  — see `docs/formats/nextion-hmi-format.md` §3.2) makes `compile` refuse
  with `SpecError::ColorFontUnsupportedForType` rather than guessing at
  offsets that might belong to a different field on that component type.
  `bco2`/`pco2` (pressed-state colors) have no patch path yet — see
  `docs/formats/nextion-tft-format.md` §2b for what's still unconfirmed.
- **`txt`/`x`/`y`/`w`/`h` work on any component type** — these use
  pattern search (`tft::patch_text`/`patch_geom`), which doesn't need to
  know the record layout at all.
- **No font/image data.** `font` only changes which already-compiled font
  *id* a component references — this toolkit has no font compiler and
  never will until someone reverse-engineers the `.zi` font-blob format
  (see the `.HMI` format doc §2).
- **No structural changes.** Components aren't added, removed, or
  reordered. A spec can't introduce a new `objname` that isn't already in
  the scaffold.
- **`page` is not yet enforced.** It's recorded for documentation/future
  multi-page support, but `compile` currently searches across all pages
  of the decoded scaffold `.HMI` for each `objname` rather than scoping to
  the named page. If your scaffold reuses an `objname` across pages,
  disambiguating is not yet possible — give distinct names instead.
- **Single target.** `target` must be the one target this toolkit
  supports (see [`docs/targets.md`](targets.md)); anything else is a hard
  error, checked against the scaffold `.tft`'s own header dimensions too.

## Example

See [`examples/els-page0.yaml`](../examples/els-page0.yaml) for a full
worked example: a complete page-0 layout for a NanoEls-style ELS
(Electronic Lead Screw) firmware, with every component tied to that
firmware's touch-id table via comments.
