# Nextion `.HMI` container format — reverse-engineered spec

Status: derived entirely from static analysis of `h5.HMI` (a NanoELS H5
project file, Nextion Editor format, `ver21234` internally). Nothing here
came from decompiling the Editor itself — only from the file on disk.
Confirmed parts are marked **[confirmed]**; guessed/unverified parts are
marked **[hypothesis]**. Treat hypotheses as "test before you rely on it."

No encryption, no compression, no per-record checksum was found anywhere
in the areas described below. This is a flat, mostly self-describing
binary format — closer to a tiny append-only filesystem than to a
compiled bytecode blob.

---

## 1. Top-level layout **[confirmed]**

The file is a raw image of the display's internal flash, laid out in
`0x80000`-byte (512 KiB) blocks:

| Block(s) | Offset range | Content |
|---|---|---|
| 0 | `0x000000–0x07FFFF` | **Directory / superblock A** — small (<1 KiB used), rest zero-filled |
| 1 | `0x080000–0x0FFFFF` | **Directory / superblock B** — byte-for-byte identical to block 0 |
| 2–13 | `0x100000–0x6FFFFF` | Entirely zero — reserved/erased flash, unused by this project |
| 14–15 | `0x700000–end` | **Resource payload** — fonts, images, and the per-page component tables |

Blocks 0 and 1 are an exact mirror of each other. This is the classic
"A/B superblock" redundancy pattern used so the device can recover from a
power loss mid-write — not a checksum, just a second copy. If you hand-edit
block 0, **you must copy the identical change into block 1** or the two
copies will disagree (behavior in that case is untested — could mean the
Editor/device picks one arbitrarily, or refuses the file).

The payload doesn't start until block 14 (`0x700000`) regardless of how
little data the project has — this is very likely because the directory
entries store resource locations as `(64 KiB sector, byte offset)` pairs
(see §2), and `0x700000 / 0x10000 = sector 0x70`. That looks like a fixed
starting sector baked into how the Editor lays out a fresh project, not
something computed from content size.

## 2. Directory (block 0/1) **[confirmed, partially]**

Starts at offset 0, ends once the file goes all-zero (~0x3C0 bytes used in
this project). It's a flat list of resource entries, one per compiled
asset. Names visible in this project: `main.HMI`, `Program.s` (the global
startup script — plain readable Nextion Instruction Set text), and per
resource-index entries named `<n>.<ext>`:

| ext | meaning |
|---|---|
| `.i` | compiled per-page component table (this is the part §3 decodes) |
| `.is` | icon/image slice set for that page |
| `.zi` | compressed font subset used on that page |
| `.pa` | palette |
| `.wav` | embedded audio |

Each entry carries a **[hypothesis]** `(sub-offset: u16, sector: u16, size:
u32, flags: u32)` — the sector number times `0x10000` plus the sub-offset
gives the absolute file offset. E.g. bytes `00 70 e0 00 00 00 01 00 00 00`
decode as sector `0x70`, sub-offset `0`, size `0xE0` (224) → absolute
offset `0x700000`, matching where `Program.s`'s actual bytes sit. This
held up for the handful of entries checked but wasn't exhaustively
verified against all ~15 entries, and the exact meaning of the trailing
`flags` word is unknown.

**Not found:** any checksum over the directory bytes themselves, or over
an individual resource's size/offset pair. This is a soft spot — a
community write-up of the related NSPanel `.HMI` variant mentions a
directory checksum they couldn't crack; we didn't hit it here, which
either means this project's variant doesn't have one, or it lives
somewhere we haven't poked yet (e.g. covering the whole directory as one
block, rather than per-entry).

**Open problem:** how the Editor knows the *total* size of a page's `.i`
table (i.e. where to stop reading when there's no explicit end-of-page
marker other than "next known resource's offset"). This matters for
adding/removing components — see §5.

## 3. Component attribute records **[confirmed]**

This is the part that matters for a textual round-trip, and it's fully
decoded. Starting at each page's `.i` resource offset, the page is a flat
sequence of these fixed-shape records, one per **attribute**, with no
padding or alignment between records:

```
+----------------+------------------+-----------+------------+
| name (16 bytes)| value (N bytes)  | type (1B) | pad (3×0x00)|
| ASCII, NUL-pad | LE bytes, N>=0   |           |             |
+----------------+------------------+-----------+------------+
```

- **name**: ASCII attribute name, NUL-padded to exactly 16 bytes. Always
  fits — the longest name seen (`groupid0`, `txt_maxl`) is 8 characters.
- **value**: variable length. For `type=0x11` it's the literal ASCII
  bytes of a string value (component name, text content — not
  NUL-terminated, length is implicit from where the type byte sits). For
  `type=0x12` it's a little-endian unsigned integer, 1 or 2 bytes
  depending on the attribute's declared width (not on the magnitude of
  the value — `id` is always 1 byte even when the value is small enough
  to not need it; `x`/`y`/`w`/`h` are always 2 bytes even when 0).
- **type byte**: `0x11` = string, `0x12` = integer. No other type codes
  were catalogued, though components with picture/font references likely
  use others we haven't hit (large binary blobs — see caveat below).
- **pad**: always exactly three `0x00` bytes, regardless of type or value
  width.

A **component** is simply a contiguous run of these records; a new
component starts wherever a `type` record appears (empirically, `type`
is always the first attribute written for a component). A page starts
with one `type` record whose objname is the page name itself (e.g.
`page1`), width/height = screen size, followed by one `type` record per
widget on that page.

### 3.1 Confirmed attribute names and what they hold

Universal (present on every component seen):

| name | meaning | type |
|---|---|---|
| `type` | component class, stored as a **single ASCII letter** (`t`=text, `b`=button, `p`=picture/page, `y`=page container) | string |
| `id` | component id — **this is the number your firmware's touch handler receives** | int |
| `objname` | the component's name as shown in the Editor (`bOff`, `textGear`, …) | string |
| `vscope` | visibility scope (0/1 seen) | int |
| `drag` | draggable flag | int |
| `sendkey` | "send key" touch option | int |
| `aph` | alpha/opacity (0–127 seen; Nextion opacity is 0–127) | int |
| `movex`, `movey` | drag-move offsets | int |
| `x`, `y`, `w`, `h` | position and size in pixels | int (2 bytes) |
| `endx`, `endy` | bottom-right corner (`x+w-1`, `y+h-1`) | int |
| `effect`, `first`, `time` | show/hide transition effect settings | int |
| `lockobj` | lock-in-editor flag | int |
| `groupid0`, `groupid1` | radio-button group ids | int |
| `borderc`, `borderw` | border color / width | int |

Seen on text-like and button components (present when relevant):

| name | meaning |
|---|---|
| `sta` | background style (crop/image/solid) |
| `bco`, `bco2` | background color (normal / pressed state) |
| `pco`, `pco2` | text/font color (normal / pressed state) |
| `pic`, `pic2` | background picture id (normal / pressed state) |
| `picc`, `picc2` | cropped-picture id (normal / pressed state) |
| `xcen`, `ycen` | horizontal/vertical text alignment |
| `txt` | text content — **note:** short strings decode fine as ASCII, but this field is also where the scanner will swallow large opaque blobs (see caveat) |
| `txt_maxl` | max text length |
| `isbr` | word-wrap flag |
| `spax`, `spay` | character/line spacing |
| `style` | border style |
| `font` | font id |
| `pw` | picture width override |
| `key` | keyboard-popup id |
| `val` | numeric value (sliders, progress bars, checkboxes) |

### 3.2 Caveat: large binary values break naive scanning

Some attribute values are raw binary blobs (embedded PNGs for pictures,
compressed font data) that are **many kilobytes long and contain no
`0x00`-padding structure of their own**. A dumb "find the next valid
16-byte name pattern" scanner (like the one used to produce this doc) has
no way to know where such a blob ends except "wherever the next
recognizable name pattern happens to reappear" — so it will occasionally
attribute several KB of an embedded PNG to whatever attribute preceded
it. This doesn't corrupt anything on disk, it's purely a limitation of a
scanner that doesn't know the declared length up front. A byte-accurate
tool needs the *actual* length either from the directory (§2) or from a
proper per-attribute length prefix we haven't found yet for the blob
types — plain string/int attributes are unaffected and decode perfectly.

## 4. What this enables today

- **Decode**: turn any `.HMI`'s component tables into a readable,
  diffable, version-controllable text form (YAML/JSON). Solid — this is
  what `hmi_tool.py`'s `decode` command does.
- **Patch in place**: change the *value* of an existing attribute — a
  color, a coordinate, a short text string — **as long as the new value
  is the same byte length as the old one** (same string length, or a
  number that still fits in the attribute's fixed int width). Because
  each record is self-contained with no length prefix elsewhere pointing
  to it, an equal-length in-place overwrite doesn't disturb anything
  around it. This is what `hmi_tool.py`'s `patch` command does. **Not
  yet independently verified against the real Nextion Editor** — see §6.

## 5. What this does *not* yet enable

- **Adding or removing a component**, or changing a string's length
  (e.g. renaming `textGear` to `textGearRatio`), shifts every byte after
  it in the file. That's fine for the component-record stream itself
  (it's just a flat sequence, no internal offsets to fix up) — but it
  changes the resource's total size, which the directory entry (§2)
  records. We have a hypothesis for that field's layout but haven't
  proven we can recompute and rewrite it correctly. Until that's
  verified, treat structural edits (component count changes) as
  unsupported.
- We have not looked at the `.tft` compiled-firmware format at all yet.
  Everything above is about the `.HMI` *project* file that only the
  Editor reads. Getting from edited `.HMI` → `.tft` still goes through
  Nextion Editor's own (closed, unreverse-engineered) compiler.

## 6. Recommended validation loop before trusting this for real work

1. Pick one harmless attribute (e.g. `textGear`'s `bco` background color,
   or its `txt` value with a same-length replacement).
2. Patch it with `hmi_tool.py patch`.
3. Open the patched file in the real Nextion Editor. Two outcomes:
   - Opens fine, shows the new value in the property panel → same-length
     in-place patching is confirmed safe, and this becomes a real
     bypass for that class of edit.
   - Refuses to open / shows garbage → there's a check we haven't found
     (whole-directory checksum, or a hidden length table), and we go
     hunting for it with a known-bad file in hand, which is a much
     easier reverse-engineering problem than working blind.
4. Only after that, attempt the harder case: add one new component (copy
   an existing record block, edit its `objname`/`id`/`x`/`y`, insert it,
   and see if updating the directory's size field the way we predict
   keeps the file valid).
