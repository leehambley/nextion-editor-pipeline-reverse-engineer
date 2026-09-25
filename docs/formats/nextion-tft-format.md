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
| `+0x3C` | `0x74` | 1 | constant on text records specifically — **not type-agnostic, see §2b** |
| `+0x3D` | per-component index | 1 | increments per component (roughly `id+1`) |
| `+0x3E` | `0x01` | 1 | constant |
| `+0x3F` | `0x37` | 1 | constant on text records specifically — **not type-agnostic, see §2b** |

Exposed in code as [`tft::text_record_offset`](../../src/tft.rs) and
`tft::TEXT_RECORD_LEN`. This toolkit only ever writes to `+0x28` (`pco`)
and `+0x25` (`font`) via [`tft::patch_component_color_font`](../../src/tft.rs)
— every other field in this table is read-only knowledge, not something
the toolkit patches by offset.

## 2b. Button (`type: b`) component record **[confirmed for bco/bco2/font/geometry only]**

Same 84-byte record size as §2, found the same way (search for the known
`x,y,w,h` quad, record start = quad offset − 0x10). Confirmed against 9
real button instances (`bD0`..`bD9`, `bX`/`bY`/`bZ`, `bStatus`) spanning 4
distinct background colors and 3 distinct font ids — not just one diffed
pair, specifically to rule out coincidental matches:

| offset | field | width | notes |
|---|---|---|---|
| `+0x00–0x01` | unique tag | 2 | differs per component, same role as §2 |
| `+0x04–0x05` | sequential per-record counter | 2 | increases monotonically in file order across records — **not** color/font-related, despite being in the "reserved" range §2 assumed was `0x00` for text records. **[confirmed distinct from §2's assumption — flag: §2's "reserved" claim for this range was never tested against a second component type until now]** |
| `+0x10–0x1B` | `x,y,w,h,endx,endy` | 2 each | matches `.HMI` exactly, same offsets as §2 |
| `+0x25` | `font` | 1 | matches `.HMI`, same offset as §2 |
| `+0x26–0x27` | **`bco`** (background, normal state, RGB565) | 2 | confirmed against 4 distinct real values (`0x0000`, `0xf800`, `0x0c80`, `0x02df`) — exact LE match every time |
| `+0x28–0x29` | **`bco2`** (background, pressed state, RGB565) | 2 | all 9 samples happened to share the same `bco2` value (`0xce79`) — confirmed present at this offset and matching, but not independently distinguished from a hypothetical neighboring field since no sample varied it |
| `+0x2A–0x2D` | `pco`/`pco2` region | 4 | present, but every sample had `pco == pco2`, so the two 2-byte halves could not be independently confirmed — **[hypothesis only, do not patch]** |
| `+0x2E–0x2F` | `0x01 0x01` | 2 | constant on every button sample; candidate `xcen`/`ycen` (both `1` in `.HMI` on every sample) — **[hypothesis]** |
| `+0x30–0x31` | candidate text-pool pointer | 2 | present and plausible by analogy to §2/§3, but not independently verified against the button-label text pool the way §3 did for text — **[hypothesis]** |
| `+0x34–0x35` | varies per record, ~12-byte stride | 2 | candidate second, smaller text-pool table (button labels are short, e.g. single digits) — **[hypothesis, not verified]** |
| `+0x3C` | **varies** (`0x62`/`0x74`/`0x6d` seen) | 1 | **contradicts §2's assumption that this offset is a fixed constant** — that assumption was only ever tested against text (`t`) records; for buttons it's not constant. Correlates loosely with `+0x3D`, may actually be the low byte of a 2-byte LE value spanning `+0x3C–0x3D` rather than a separate marker+index pair — unresolved |
| `+0x3D` | per-component index | 1 | same role as §2 |
| `+0x3E` | `0x01` | 1 | constant, matches §2 |
| `+0x3F` | `0x37` | 1 | constant, matches §2 — this byte *is* type-agnostic (§2's speculation that it might be a type tag is contradicted: both `t` and `b` records share it) |

**Not resolved:** exact `pco`/`pco2` split, `pic`/`pic2` location (no
sample had a non-sentinel picture value — every button checked had
`pic == pic2 == 0xffff`, the "no picture" sentinel), and the true
role of `+0x30–0x31`/`+0x34–0x35`. **Do not patch any of these** — only
`bco`, `bco2`, `font`, and geometry are confirmed safe to write.

One button, `bMode` (a picture-styled button, `.HMI` `style: 4`), could
not be found in the `.tft` at all by its `.HMI` `x,y,w,h` — it likely
renders through a different record shape tied to its picture styling.
Flagged as unexplained; doesn't affect the solid-color buttons above.

Exposed in code as `tft::button_record_offset`, with a corresponding
[`tft::patch_button_color_font`](../../src/tft.rs) that only ever writes
`bco`/`bco2`/`font` — mirroring [`tft::patch_component_color_font`](../../src/tft.rs)'s
restriction to confirmed fields for text records.

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
  removes components. This is also the blocker for compiling a UI spec
  directly into a `.tft` with no Editor-built scaffold at all — see
  [targets.md](../targets.md) for the current status of that effort.
- The button-type record layout is now confirmed for `bco`/`bco2`/`font`/
  geometry (§2b) but **not** for `pco`/`pco2`, `pic`/`pic2`, or the
  candidate text-pool-pointer fields at `+0x30`/`+0x34`.
- The **compiled record layout for `type: m` (Hotspot) components is
  entirely unexplored** — see [nextion-hmi-format.md](nextion-hmi-format.md)
  §3.1 for what's known about `m` in the `.HMI` project format (very
  likely an invisible touch-only region with no visual attributes at
  all). Whether `m` components even have a `.tft` geometry/color record
  the way `t`/`b` do, or are encoded some other way (since they render
  nothing), hasn't been checked. This toolkit doesn't patch `m`
  components in any way.
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

**If a scaffold genuinely isn't an option** (no Nextion Editor access at
all), the only path to a working UI is reverse-engineering the page/
component-enumeration bookkeeping this section sidesteps — see
[targets.md](../targets.md) for the current status of that effort and
its validation method. On the NX8048P050-011R-Y, a `.tft` is flashed by
placing it as the *only* file on a FAT-formatted microSD card and power-
cycling the display — no serial/UART tooling required for this — which
makes "does this synthesized `.tft` actually boot" a fast, repeatable
check once that work is underway.
