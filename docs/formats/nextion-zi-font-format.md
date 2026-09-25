# Nextion `.zi` font format — reverse-engineered spec (partial)

Status: derived from static analysis of three real `.zi` files produced by
the Nextion Editor's font-compiler tooling
([`examples/fonts/mono72.zi`](../../examples/fonts/mono72.zi),
[`mono60.zi`](../../examples/fonts/mono60.zi),
[`mono50.zi`](../../examples/fonts/mono50.zi) — three point sizes of the
same "mono" font used by the reference `h5.HMI`/`h5.tft` project),
cross-referenced against where their bytes appear inside `h5.tft`. This is
the *container* level only — enough to locate and identify a font's data,
not to decode or re-encode individual glyphs. See §5 for exactly where
this stops and why.

This document exists so a future `compile` (or a from-scratch `.tft`
synthesis effort — see `docs/formats/nextion-tft-format.md` §5/§6 and
`docs/targets.md`) can **reference** an existing compiled font by id
without needing to understand its glyph encoding — which is genuinely a
separate, harder problem (§5).

## 1. Standalone `.zi` file layout

### 1.1 Header (44 bytes) **[confirmed]**

| offset | field | notes |
|---|---|---|
| `0x00–0x03` | `04 ff 00 XX` | constant prefix; last byte varies (`0x0a`/`0x0c` seen) — purpose unknown |
| `0x07` | point size | `u8` — `72`/`60`/`50` in the three reference files, matches the filename exactly |
| `0x14–0x17` | payload length | `u32` LE — equal to `file_size - 0x2c` (44) in every sample; the byte count of everything from the font-name string onward |
| `0x18–0x1b` | header length | `u32` LE — constant `44` (`0x2c`) in every sample, i.e. this field describes its own header's length |
| rest | uninterpreted | not investigated — no field here was needed to locate/identify a font |

### 1.2 Font name + encoding string **[confirmed]**

Immediately after the 44-byte header: an ASCII string, NUL-terminated
with a trailing space before the NUL, of the form `<name><encoding> \0`
— e.g. `mono72utf-8 \0`. In all three reference files this is exactly
`<filename-without-extension><ASCII "utf-8">`, confirming the filename
does encode this metadata directly, as observed. **Caveat**: despite the
literal string `utf-8`, the glyph table (§1.3) below only ever contains
entries for the printable Basic Latin / ASCII range (`!` through `~`,
codes 0x21–0x7E) in all three reference files — whatever "utf-8" means
here, it is not evidence of actual multi-byte/non-ASCII glyph coverage in
these particular fonts.

### 1.3 Glyph table **[confirmed]**

Starting immediately after the name string's trailing NUL: a flat array
of fixed 10-byte entries, one per glyph, in ascending character-code
order:

| field | width | notes |
|---|---|---|
| height | `u16` LE | constant across all entries in a given file (`28` for all three reference fonts — plausibly font point size related, not confirmed against a font with mixed glyph heights) |
| glyph data offset | `u32` LE | **not a byte offset into this file** — see §1.4 |
| width | `u16` LE | per-glyph advance/pixel width, varies per character |
| character code | `u16` LE | ASCII/Unicode code point this entry describes |

All three reference files have 94 entries (`!` through `~` inclusive,
`0x21`–`0x7E`), each exactly 10 bytes, with no gaps or padding between
entries. Table end = table start + `94 * 10` in these samples; in
general, end = wherever the character codes stop being sequential/valid
(the same "no explicit length field found yet" caveat as the `.HMI`
attribute-record table — see `nextion-hmi-format.md` §2's "open problem"
note for the analogous situation there).

### 1.4 Glyph data — **[not decoded, see §5]**

The "glyph data offset" field in §1.3 is **not** a byte offset into the
`.zi` file: sampled values (e.g. `243712`, `4237824`) far exceed the
file's actual size (`16881` bytes for `mono60.zi`), and aren't a clean
multiple of the file size either. This is almost certainly a **bit
offset** into a packed/compressed bitstream (`243712 / 8 = 30464` bytes
— still larger than the file, but consistent with per-glyph run-length
or Huffman-style compression where a handful of glyphs can expand well
past their compressed footprint). Nothing about the actual raster or
vector encoding of a glyph's pixels has been investigated. See §5.

## 2. How `.zi` fonts are packed into a compiled `.tft`

Confirmed by locating each reference font's name string
(`mono72utf-8`, `mono50utf-8`, `mono60utf-8`) inside `h5.tft` via a plain
byte search, then comparing the surrounding bytes against the standalone
`.zi` file:

- **All three fonts are embedded**, back-to-back, starting partway
  through the file (`h5.tft` is 874,820 bytes; the three font blocks
  start at `0x9fa88`, `0xa567b`, and `0xa888c` respectively — i.e. in
  the file's last ~220 KB).
- **Packing order is not size-sorted or name-sorted**: the file order is
  mono72, mono50, mono60 — some other criterion (likely font-id
  assignment order in the original `.HMI` project, or Editor-internal
  bookkeeping) determines the order, not anything visible in the font
  data itself.
- **Each font's header + name string + glyph table + glyph data is
  embedded as one contiguous block**, in the same shape as the
  standalone `.zi` file's own layout from its name string onward (§1.2
  onward) — but the leading 44-byte header (§1.1) is **not** copied
  as-is per block; instead, what precedes each block in `h5.tft` is
  the *next* font's own 44-byte header (i.e. the headers and
  name+table+data blocks appear to be laid out independently /
  interleaved, not as simple back-to-back copies of whole standalone
  `.zi` files). This was not fully resolved — see §5.
- **The embedded glyph tables are not byte-identical to the standalone
  `.zi` files**, despite being clearly the same font at the same point
  size with the same character set. Per-glyph `width` and `offset`
  values differ by small amounts (e.g. `width: 112` vs `111`,
  `offset: 258048` vs `258048` unchanged but others shifted by ~250–1500)
  consistently from the second table entry onward — this is the same
  signature already documented for `h5.HMI` vs `h5.tft` in
  `tests/fixtures/README.md` (**different compiler/Editor-version
  builds of conceptually the same asset**, not a deliberate repacking
  transform). It does **not** indicate the container-level packing
  scheme mangles font data; it means these two files were not compiled
  from the exact same project state.
- **No font directory/table of contents was found** in `h5.tft`'s header
  region (first 64 KB) that lists each font's absolute offset by id —
  a plain search for each font block's own starting offset (as a `u32`
  LE value, with a few plausible off-by-N adjustments) found no match.
  Either such a directory exists elsewhere in the file (not yet
  located), is encoded differently than a flat offset table, or font
  location is computed some other way at render time. **This is the
  open question blocking font-id → font-block resolution** — see §3.

## 3. What this means for `font` ids in `.HMI`/`.tft`

Every component's `font` attribute (`.HMI`) / `font` byte (`.tft`
text/button records — see `nextion-tft-format.md` §2/§2b) is a small
integer (`0`, `1`, `2`, `3` seen in `h5.HMI`) that presumably indexes into
whatever font directory assigns each embedded `.zi` font block a
sequential id at compile time. **This mapping (font id → byte offset of
that font's block in the `.tft`) has not been located.** Practically,
this means:

- This toolkit can (and does, via `compile`'s `font` field — see
  `docs/spec-format.md`) **reference** an existing font id already
  compiled into a scaffold `.tft`, since that only requires writing a
  small integer at a known offset (`text_record_offset::FONT` /
  `button_record_offset::FONT`) — no font-block knowledge needed.
- This toolkit **cannot** determine, from a `.tft` alone, which font id
  corresponds to which point size/typeface, or add a new font and assign
  it a fresh id — both would require solving §2's open directory
  question.

## 4. What this gets you today

- **Identify and locate** a font's block inside a compiled `.tft` by
  searching for its name string (`<name><encoding>`), if you already
  know or can guess the name (e.g. from the original `.HMI` project, or
  from having the standalone `.zi` the project was built with).
- **Confirm a `.zi` file's point size, name, and character-set coverage**
  without needing Nextion Editor, by reading the header (§1.1) and name
  string (§1.2) — useful for e.g. verifying which font a `font` id
  *probably* refers to, by comparing against known fonts used in the
  project.
- **Nothing beyond that.** No glyph rendering, no font addition, no
  font-id-to-block-offset resolution.

## 5. What's still unknown (the actual hard problem)

- **Glyph raster/vector encoding.** The `.zi` extension and the
  bit-offset-not-byte-offset table field (§1.4) both point to a
  compressed, non-trivial encoding — likely per-glyph
  run-length/Huffman-style compression of a monochrome bitmap, given the
  Nextion Editor's public documentation describes its fonts as
  monochrome anti-aliased bitmaps rather than outline/vector fonts, but
  this has **not** been verified against these files' actual bytes.
  Cracking this is a project-sized effort on its own, comparable in
  scope to the `.HMI`/`.tft` container-level work already flagged as
  unsolved.
- **Font directory / id-to-offset mapping in `.tft`** (§2/§3) — not
  located. This blocks adding a *new* font to a `.tft` (as opposed to
  referencing an existing one by id).
- **How multiple fonts' headers/blocks are actually interleaved** in the
  compiled file (§2's last bullet) — observed but not fully explained.
- **Whether `.HMI` embeds `.zi` font data the same way `.tft` does** —
  not checked in this pass; `nextion-hmi-format.md` §2 already flags
  `.zi` as one of the directory's resource extensions but the `.HMI`
  side of font embedding was out of scope here.

This toolkit does not attempt any of the above. "No font pipeline" (see
`README.md`) remains an accurate description of its scope — this
document narrows *where* that boundary is, from "fonts are a black box"
to "fonts can be located and referenced by id; their internal encoding
and the id-assignment directory are the two remaining unknowns."
