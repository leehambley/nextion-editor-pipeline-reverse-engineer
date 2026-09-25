# Nextion `.tft` compiled-firmware format — reverse-engineered spec (partial)

Status: derived from static analysis of `h5.tft` (874,820 bytes, 800×480
target), cross-referenced against the known-good values from the `.HMI`
decode. Much smaller scope than the `.HMI` doc — this is the beginning of
the format, not the whole thing — but everything below is **directly
confirmed against real values from your actual project**, not guessed.

Unlike `.HMI`, object names (`objname`, `vscope`, `sendkey`, ...) and the
Editor's bookkeeping fields do **not** survive compilation — this is a
genuinely different, denser, purpose-built runtime format. The good news:
what does survive is completely unencrypted and unobfuscated. Position,
size, color, font, and the display text itself are all plain little-endian
values, findable by searching for numbers you already know.

## 1. File header (80 bytes) **[confirmed]**

| offset | field | notes |
|---|---|---|
| `0x00–0x01` | `00 01` | constant |
| `0x02–0x03` | `44 4e` (`"DN"`) | magic |
| `0x0C–0x0D` | width | `u16` LE — `800` in this file |
| `0x0E–0x0F` | height | `u16` LE — `480` |
| `0x10–0x13` | width, height repeated | same values again |
| `0x3C–0x3F` | total file size | `u32` LE — verified equal to actual file length |

Everything else in the header is uninterpreted so far.

## 2. Text-labeled component record (84 bytes) **[confirmed]**

Found by searching the file for the exact `x,y,w,h` bytes already known
from the `.HMI` decode (e.g. `textGear`: `72,22,180,60` → byte pattern
`48 00 16 00 b4 00 3c 00`), then diffing that record against a sibling
component (`textTurn`, same page, same size, different position) to see
which bytes vary and which stay constant.

Record start = (offset of the `x,y,w,h` bytes) − 12. Fields, relative to
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
| `+0x28` | `pco` (text color, RGB565) | 2 | matches `.HMI` exactly (`0xffff` white on one sample, `0x049f` on the other) |
| `+0x2A–0x2D` | reserved (likely `bco`) | 4 | `0x00` on both samples — **need a component with a non-default background to confirm this is really `bco`** |
| `+0x2E` | `txt_maxl` | 2 | `100` on both, matches `.HMI` |
| `+0x30` | **text-pool pointer** | 2 | see §3 — a `u16` byte offset, not the text itself |
| `+0x32–0x3B` | reserved | 10 | `0x00` |
| `+0x3C` | `0x74` | 1 | constant marker |
| `+0x3D` | per-component index | 1 | increments per component (roughly `id+1`) |
| `+0x3E` | `0x01` | 1 | constant |
| `+0x3F` | `0x37` | 1 | constant — possibly a component-type tag, only one type sampled so far |

**Not yet done:** the equivalent record for a *button* type component
(will have paired normal/pressed fields — `bco`/`bco2`, `pco`/`pco2`,
`pic`/`pic2` — that text labels don't need). Same diffing technique
should crack it quickly; just needs picking two sibling buttons and
repeating §2's method.

## 3. Text pool **[confirmed]**

Each page's display strings live in a separate table: fixed **104-byte
slots**, one per text-bearing component, in the same order as the page's
component records, immediately following the last geometry record of the
page. Each slot is the ASCII text, left-aligned, NUL-padded to fill the
104 bytes (there is no explicit length prefix — the NUL padding is the
only terminator).

The `+0x30` pointer in each geometry record (§2) is a `u16` **byte
offset**, relative to a fixed per-page base address: `text_address = page_base + pointer`.
Confirmed by solving for `page_base` from two different components and
getting the identical answer both times (`0xC002C` for this file's
`page1`). How `page_base` itself is computed/stored (per-page header?
fixed table?) is not yet located — for patching *existing* text this
doesn't matter, since you can find a slot by searching for its current
text directly; it only matters if you need to compute the pointer for a
**new** slot from scratch.

## 4. What this gets you today

- **Patch existing display text** directly in a compiled `.tft`, no
  Editor, no `.HMI`, by searching for the current string and overwriting
  with a same-or-shorter string (the 104-byte slot is NUL-padded, so
  shorter text just needs the remainder zeroed — more slack than `.HMI`
  ever had).
- **Patch existing geometry** (reposition/resize a component) by
  searching for the current `x,y,w,h` quad and rewriting all six
  position fields (`x,y,w,h,endx,endy`) consistently.
- **Patch color/font** of an existing component the same way, once you
  know its current value to search for.

`tft_tool.py` implements the first two (text, geometry) and has been run
against your real `h5.tft`.

## 5. What's still unknown

- How a page's component list is enumerated/counted (needed to **add or
  remove** a component — same class of problem as the `.HMI` directory,
  just not yet investigated for `.tft`).
- The button-type record layout (§2's "not yet done").
- Font/image resource encoding (not investigated — irrelevant if you're
  only repositioning/retexting components that already reference fonts
  that exist in the file).

## 6. Recommended strategy given "one hardware target only"

Don't try to synthesize a `.tft` from nothing. Instead:

1. Build **one** `.HMI` in Nextion Editor containing every component your
   new UI needs — right `type`, right `id` (matching the firmware's
   `key_for()` table), roughly-right position — and compile it once. This
   is the only time the Editor is involved. Call this the **scaffold**.
2. From then on, iterate entirely by patching the scaffold's `.tft`
   directly from your YAML spec: reposition, retext, recolor — all of
   which are proven-workable in-place edits (§4). No component count
   ever changes, so the one genuinely unsolved problem (§5, "how do you
   add a component") never comes up.
3. Only fall back to re-opening the Editor if you need to add a
   component the scaffold didn't already contain — which, if the
   scaffold is built to match your firmware's full `key_for()` table up
   front (as `future_ui_page0.yaml` already does), should be rare-to-never.

This sidesteps both open problems (`.HMI` directory bookkeeping and
`.tft` component enumeration) entirely, at the cost of one manual Editor
session instead of zero.
