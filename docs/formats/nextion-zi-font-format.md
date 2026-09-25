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
| height | `u16` LE | constant across all entries *within* a given file, but differs *between* files consistently with point size: `37` for `mono72.zi`, `28` for `mono60.zi`, `23` for `mono50.zi` (not a 1:1 ratio with the point-size byte in §1.1 — e.g. 72pt → 37px, not 72px — consistent with "point size" and "pixel height" being different units, as is normal for font metrics) |
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

### 1.4 The "glyph data offset" field is synthetic, not a real pointer **[confirmed]**

The "glyph data offset" field in §1.3 is **not** a byte offset into the
`.zi` file, and does not point at compressed glyph data at all — it is
fully explained by a simple formula, confirmed exactly (zero mismatches)
across all 94 entries in every reference file:

```
offset[n] = offset[0] + 256 * sum(width[0..n))
```

i.e. it's `256 × ` a running total of *declared advance widths* (§1.3's
`width` field), plus a fixed per-file base (`243712` — identical across
all three reference files regardless of point size, so this constant is
not derived from anything file-specific either). This was confirmed by
computing the predicted offset for every entry from `width` alone and
comparing byte-for-byte against the table's actual `offset` field — 94/94
exact matches in `mono50.zi`, and the same base constant across all three
files. **Practically: this field cannot be used to locate a glyph's real
compressed data in the file.** It is most plausibly a horizontal
cursor/advance value in a coordinate space used by the *display
controller* at render time (a fixed-point subpixel cursor, given the
×256 scaling), unrelated to this file's own byte layout.

### 1.5 Glyph data — **[not decoded, see §5]**

The actual per-glyph compressed byte data starts immediately after the
glyph table (§1.3) and runs to the end of the file, but **no reliable way
to find each glyph's individual byte boundary within that region has
been found** (see §1.4 — the natural candidate field turned out not to
point here at all). What's confirmed:

- The data is genuinely compressed, not a raw bitmap: an uncompressed
  1-bit-per-pixel bitmap (row-major, byte-aligned per row) would need
  roughly 3× more bytes than the file actually contains, for both
  row-major and column-major layout assumptions.
- The total glyph-data region size divided by the sum of all glyphs'
  declared `width` values is very close to `1.0` (`1.007` for
  `mono50.zi` — 11,822 real bytes vs. 11,736 sum-of-widths) — suggesting
  average compressed size is close to 1 byte per pixel-column, though
  this is a whole-file average, not evidence for any individual glyph's
  exact byte length.
- A byte-value frequency count of the glyph-data region shows `0x1f` (31)
  as by far the most common byte (~15% of all bytes) — consistent with
  it being a "background/blank run" marker in some run-length scheme,
  since most of a monospace ASCII glyph set's pixels are background.

See §5 for the specific decoding hypotheses tried and ruled out.

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

### 5.1 Glyph raster encoding — attempted, not cracked

An external, unfinished community write-up was found describing a
"ZI font format version 5" (title: "ZI font format version 5
specification", explicitly marked "unfinished... use at own risk") with
a documented per-glyph encoding: a leading mode byte (`0x01` = black/white,
`0x03` = 3-bit antialiased), followed by bytes of the form `YZdddddd`
(2-bit mode, 6-bit run length) for antialiased data, with 4 sub-modes for
runs of transparent/opaque pixels and packed 3-bit alpha values. **This
spec's field *positions* in the header/table do not match our reference
files** — applying its exact byte offsets to our files' header/table
produces nonsensical values (e.g. a "character width" field reading as
the glyph's line-orientation byte). This could mean our files are a
different `.zi` sub-version, or that spec's write-up (itself marked
unfinished) has errors. Its *general approach* — run-length codes with a
mode indicator in the top bits of each byte — remains the most plausible
lead, but applying its exact bit-layout to our glyph data did not
produce a recognizable glyph either (see below).

**Decoding attempts made and ruled out** (against `mono50.zi`'s `!`
glyph, height=23, width=39, chosen for being visually simple — a stem
and a dot):

- **Raw uncompressed 1bpp**, row-major and column-major, byte-aligned per
  row/column — ruled out by size alone (§1.5).
- **Full-byte alternating RLE** (each byte = run length of alternating
  background/ink color, starting from background) — column-major
  reshape produced a *convincing, clean vertical stem* for the first
  ~7-8 columns (a real positive signal — this matches `!`'s actual
  shape), but degraded into an unstructured diagonal noise pattern for
  all subsequent columns. Tried both starting colors (background-first
  and ink-first); tried resetting the alternation state at each column
  boundary vs. carrying it across boundaries — same result each time.
  Row-major reshape of the same decode produced no recognizable
  structure at all, confirming column-major is the correct axis, but the
  run-length/alternation model itself breaks down partway through.
- **Nibble-split RLE** (each byte = two 4-bit run lengths) — produced far
  too few pixels (38 from 11 bytes) to plausibly encode a full glyph;
  not pursued further.
- **The external spec's exact `YZdddddd` bit scheme**, both the
  black/white variant (2-bit mode + 6-bit run, tried against a byte
  presumed to be the `0x01` mode marker) and the 3-bit-antialiased
  variant (as literally specified: `00`=transparent run,
  `01`=opaque run, `10`=short run + 1 alpha pixel, `11`=2 packed alpha
  pixels) — both produced either an all-blank or an incoherent sparse
  scatter of pixels, no matter which nearby byte was tried as the
  starting mode marker. The literal `0x01`/`0x03` mode-marker bytes the
  spec describes are present in the file (found `0x03` roughly once per
  100-150 bytes, `0x01` far more rarely) but starting decode from any
  observed `0x03` position did not produce a recognizable glyph either.

**Why this stalled**: with no confirmed way to find a glyph's true byte
boundary (§1.4's field being synthetic removed the one candidate that
looked promising), every attempt above had to *guess* both the start
position and the bit-level codec simultaneously — too many degrees of
freedom to converge by inspection alone. **The single most effective
next step, identified during this session but not yet available**: a
Nextion Editor-generated `.zi` file containing exactly **one** glyph
(e.g. a single monospace character, anti-aliasing off, ASCII encoding).
With only one glyph, the byte range is unambiguous (`file_size -
table_end`, no boundary-finding needed), removing the biggest confound
above. This is a natural task for whoever has Windows + Nextion Editor
access to pick up before further blind guessing.

### 5.2 Other open questions

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
