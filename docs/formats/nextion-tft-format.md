# Nextion `.tft` compiled-firmware format — reverse-engineered spec (partial)

Status: derived from static analysis of a real compiled `.tft` (874,820
bytes, 800×480 target — the NX8048P050-011R-Y), cross-referenced against
the known-good values from the `.HMI` decode. Much smaller scope than the
`.HMI` doc — this is the beginning of the format, not the whole thing —
but everything below is **directly confirmed against real values from a
real project**, not guessed.

Unlike `.HMI`, object names (`objname`, `vscope`, `sendkey`, ...) and the
Editor's bookkeeping fields do **not** survive compilation — this is a
genuinely different, denser, purpose-built runtime format. The good news:
what does survive is completely unencrypted and unobfuscated. Position,
size, color, font, and the display text itself are all plain little-endian
values, findable by searching for numbers you already know.

This document describes the format this toolkit's [`tft`](../../src/tft.rs)
module implements. See [targets.md](../targets.md) for why every offset
here is scoped to one physical display and resolution.

## 1. File header (64 bytes) **[confirmed]**

| offset | field | notes |
|---|---|---|
| `0x00–0x01` | `00 01` | constant |
| `0x02–0x03` | `44 4e` (`"DN"`) | magic |
| `0x0C–0x0D` | width | `u16` LE — `800` in the reference file |
| `0x0E–0x0F` | height | `u16` LE — `480` |
| `0x10–0x13` | width, height repeated | same values again |
| `0x3C–0x3F` | total file size | `u32` LE — verified equal to actual file length |

Everything else in the header is uninterpreted so far. [`tft::parse_header`](../../src/tft.rs)
validates the magic, the size field against the actual buffer length, and
the width/height against the [`Target`](../../src/target.rs) the caller
asked for — a `.tft` compiled for a different resolution is rejected
outright rather than silently mis-patched.

## 2. Text-labeled component record (84 bytes) **[confirmed for `type: t` only]**

Found by searching the file for the exact `x,y,w,h` bytes already known
from the `.HMI` decode (e.g. `72,22,180,60` → byte pattern
`48 00 16 00 b4 00 3c 00`), then diffing that record against a sibling
component (same page, same size, different position) to see which bytes
vary and which stay constant.

Record start = (offset of the `x,y,w,h` bytes) − 0x10. Fields, relative to
record start:

| offset | field | width | notes |
|---|---|---|---|
| `+0x00` | unique tag | 2 | differs per component; purpose not yet known, not needed for value patching |
| `+0x02–0x09` | reserved | 8 | `0x00` on every sample so far |
| `+0x0A` | `aph` (alpha) | 2 | matches `.HMI`'s alpha (127 on both samples) |
| `+0x0C–0x0F` | reserved | 4 | `0x00` |
| `+0x10` | `x` | 2 | **matches `.HMI` exactly** |
| `+0x12` | `y` | 2 | matches |
| `+0x14` | `w` | 2 | matches |
| `+0x16` | `h` | 2 | matches |
| `+0x18` | `endx` (= x+w−1) | 2 | matches |
| `+0x1A` | `endy` (= y+h−1) | 2 | matches |
| `+0x1C–0x1F` | reserved | 4 | `0x00` |
| `+0x20` | flag (`sta`/`vscope`?) | 1 | `1` on both samples |
| `+0x21–0x24` | reserved | 4 | `0x00` |
| `+0x25` | `font` | 1 | `2` on both samples, matches `.HMI` font id |
| `+0x26–0x27` | reserved | 2 | `0x00` |
| `+0x28` | `pco` (text color, RGB565) | 2 | matches `.HMI` exactly |
| `+0x2A–0x2D` | reserved (likely `bco`) | 4 | `0x00` on both samples — **need a component with a non-default background to confirm this is really `bco`** |
| `+0x2E` | `txt_maxl` | 2 | matches `.HMI` |
| `+0x30` | **text-pool pointer** | 2 | see §3 — a `u16` byte offset, not the text itself |
| `+0x32–0x3B` | reserved | 10 | `0x00` |
| `+0x3C` | `0x74` | 1 | constant marker |
| `+0x3D` | per-component index | 1 | increments per component (roughly `id+1`) |
| `+0x3E` | `0x01` | 1 | constant |
| `+0x3F` | `0x37` | 1 | constant — possibly a component-type tag, only one type sampled so far |

Exposed in code as [`tft::text_record_offset`](../../src/tft.rs) and
`tft::TEXT_RECORD_LEN`. This toolkit only ever writes to `+0x28` (`pco`)
and `+0x25` (`font`) via [`tft::patch_component_color_font`](../../src/tft.rs)
— every other field in this table is read-only knowledge, not something
the toolkit patches by offset.

**Not yet done:** the equivalent record for a *button* type component
(will have paired normal/pressed fields — `bco`/`bco2`, `pco`/`pco2`,
`pic`/`pic2` — that text labels don't need). Same diffing technique
should crack it quickly; just needs picking two sibling buttons and
repeating this section's method. **Because this is unknown, the toolkit
refuses to patch color/font on any component whose `.HMI` `type` isn't
`t`** (see [`SpecError::ColorFontUnsupportedForType`](../../src/error.rs)) —
writing to these offsets on a button record would silently corrupt
whatever button-specific field actually lives there.

## 3. Text pool **[confirmed]**

Each page's display strings live in a separate table: fixed **104-byte
slots**, one per text-bearing component, in the same order as the page's
component records, immediately following the last geometry record of the
page. Each slot is the ASCII text, left-aligned, NUL-padded to fill the
104 bytes (there is no explicit length prefix — the NUL padding is the
only terminator). Exposed as `tft::TEXT_SLOT_LEN`.

The `+0x30` pointer in each geometry record (§2) is a `u16` **byte
offset**, relative to a fixed per-page base address: `text_address = page_base + pointer`.
Confirmed by solving for `page_base` from two different components and
getting the identical answer both times. How `page_base` itself is
computed/stored (per-page header? fixed table?) is not yet located — for
patching *existing* text this doesn't matter, since you can find a slot by
searching for its current text directly (`tft::patch_text` does exactly
this); it only matters if you need to compute the pointer for a **new**
slot from scratch, which this toolkit never does.

## 4. What this gets you today

- **Patch existing display text** directly in a compiled `.tft`, no
  Editor, no `.HMI`, by searching for the current string and overwriting
  with a same-or-shorter string (the 104-byte slot is NUL-padded, so
  shorter text just needs the remainder zeroed). Longer replacement text
  is rejected — see the note below about a bug in the original
  `tft_tool.py` reference script that this toolkit's `tft::patch_text`
  does **not** reproduce.
- **Patch existing geometry** (reposition/resize a component) by
  searching for the current `x,y,w,h` quad and rewriting all six
  position fields (`x,y,w,h,endx,endy`) consistently.
- **Patch color/font** of an existing `type: t` component the same way,
  using the confirmed offsets in §2, once its record is located by
  geometry search.

`nxtft tft-patch-text` / `tft-patch-geom` implement the first two, and the
`compile` subcommand (see [spec-format.md](../spec-format.md)) automates
all three from a YAML spec. All have been run against the real reference
`.tft` in `tests/fixtures/`.

> **Reference-implementation note:** the Python `tft_tool.py` this was
> ported from documents "longer text is rejected" but its actual length
> check only rejected a replacement longer than *both* the old text and
> the 104-byte slot — meaning a same-slot-fitting but longer-than-current
> replacement would have silently grown the file and shifted every
> subsequent byte. This toolkit's `tft::patch_text` enforces the
> documented rule for real (rejects anything longer than the current
> text), closing that gap.

## 5. What's still unknown

- How a page's component list is enumerated/counted (needed to **add or
  remove** a component — same class of problem as the `.HMI` directory,
  just not yet investigated for `.tft`). This toolkit never adds or
  removes components.
- The button-type record layout (§2's "not yet done").
- Font/image resource encoding (not investigated — irrelevant if you're
  only repositioning/retexting components that already reference fonts
  that exist in the file). This toolkit never touches font/image data.

## 6. Recommended strategy given "one hardware target only"

Don't try to synthesize a `.tft` from nothing. Instead:

1. Build **one** `.HMI` in Nextion Editor containing every component your
   UI needs — right `type`, right `id` (matching your firmware's touch-id
   table), roughly-right position — and compile it once. This is the only
   time the Editor is involved. Call this the **scaffold**.
2. From then on, iterate entirely by patching the scaffold's `.tft`
   directly from a YAML spec: reposition, retext, recolor — all of which
   are proven-workable in-place edits (§4). No component count ever
   changes, so the one genuinely unsolved problem (§5, "how do you add a
   component") never comes up. This is exactly what `nxtft compile` does
   — see [spec-format.md](../spec-format.md).
3. Only fall back to re-opening the Editor if you need to add a
   component the scaffold didn't already contain.

This sidesteps both open problems (`.HMI` directory bookkeeping and
`.tft` component enumeration) entirely, at the cost of one manual Editor
session instead of zero.
