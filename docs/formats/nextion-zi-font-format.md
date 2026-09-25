# Nextion `.zi` Font Format (versions 5 and 6)

```
Status:     Descriptive specification, reverse-engineered
Implements: src/zi.rs (FontFile, Glyph, unpack_pixels, pack_pixels)
Validated:  7/7 reference files in examples/fonts/ — every glyph decodes
            to exactly cell_w × line_px pixels (tests/zi_decode.rs)
```

## Table of Contents

1. [Introduction](#1-introduction)
2. [Conventions and Terminology](#2-conventions-and-terminology)
3. [File Structure](#3-file-structure)
4. [Preamble](#4-preamble)
5. [Label](#5-label)
6. [Glyph Index](#6-glyph-index)
7. [Glyph Data](#7-glyph-data)
8. [Pixel Packing Schemes](#8-pixel-packing-schemes)
9. [Charset Parameters](#9-charset-parameters)
10. [Formal Grammar (EBNF)](#10-formal-grammar-ebnf)
11. [Worked Examples](#11-worked-examples)
12. [Embedding in `.tft`](#12-embedding-in-tft)
13. [Open Questions](#13-open-questions)
14. [Provenance and Errata](#14-provenance-and-errata)

## 1. Introduction

A `.zi` file holds one bitmap font for Nextion HMI displays: a
fixed-size preamble, a descriptive label, an index with one entry per
glyph, and each glyph's run-length-compressed coverage raster. The
compiled `.tft` firmware embeds `.zi` fonts (see §12).

This document specifies format revisions 5 and 6. All reference files
seen so far are revision 6.

## 2. Conventions and Terminology

The key words "MUST", "MUST NOT", "SHOULD", and "MAY" are to be
interpreted as described in [RFC 2119] when, and only when, they appear
in all capitals.

Every claim carries one of these confidence markers:

- **[normative]**: required to decode correctly, and validated against
  every reference file.
- **[observed]**: holds in every reference file, but no decoder depends
  on it.
- **[inferred]**: the purpose is a best guess and has not been verified.

Additional conventions:

- Multi-byte integers are **little-endian**. `u8`, `u16`, `u24`, and
  `u32` are unsigned integers of 1, 2, 3, and 4 octets.
- Octet offsets are zero-based and written in decimal. Hexadecimal
  values carry a `0x` prefix.
- Bit diagrams follow RFC 791 style. Each row is 32 bits (4 octets),
  and bit 0 is the most significant bit of the first octet in the row.
  Because the integers are little-endian, a multi-octet field in a
  diagram starts with its least significant octet.
- In bit-level layouts of a single octet, bit 7 is the MSB.

| Term | Meaning |
|---|---|
| *coverage* | 3-bit ink intensity of one pixel. `0` is background, `7` is full ink, and `1`–`6` are anti-aliasing levels. |
| *cell* | Rectangle a glyph is rasterised into, `cell_w × line_px` pixels. |
| *cell_w* | `ink_w + pad_left + pad_right` (§6). |
| *line_px* | Font-wide glyph height in pixels (§4). |
| *index base* | Absolute file offset of the first glyph index entry, `44 + label_len`. |

## 3. File Structure

```
+------------------------+  offset 0
|  Preamble   (44 B)     |  §4
+------------------------+  44
|  Label   (label_len B) |  §5
+------------------------+  44 + label_len  = index base
|  Glyph Index           |  §6   glyph_count × 10 B
|  (glyph_count × 10 B)  |
+------------------------+
|  Alignment Padding     |  §7.1 0–7 zero octets
+------------------------+
|  Glyph Data            |  §7   concatenated packed glyphs
+------------------------+  EOF
```

## 4. Preamble

The preamble is 44 octets long.

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|     Magic     | Trail Skip At |Trail Skip Cnt |    Layout     |   0
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|  Charset ID   |   Coverage    | Fixed Advance |    Line Px    |   4
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|  Lead First   |   Lead Last   |  Trail First  |  Trail Last   |   8
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                     Glyph Count (u32 LE)                      |  12
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|  Format Rev   |   Label Len   |        Reserved (u16 = 0)     |  16
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                   Payload Length (u32 LE)                     |  20
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                  Preamble Length (u32 LE = 44)                |  24
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
| Lead Skip At  | Lead Skip Cnt |  Antialiased  |  Unknown (1)  |  28
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|   Face Len    | Offset Units  |        Reserved (u16 = 0)     |  32
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                 Charset Glyph Count (u32 LE)                  |  36
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                     Reserved (u32 LE = 0)                     |  40
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

| Off | Size | Field | Conf. | Semantics |
|---:|---:|---|---|---|
| 0 | 1 | Magic | normative | MUST be `0x04`. |
| 1 | 1 | Trail Skip At | observed | Start of an excluded range for the final byte of a character code (§9). `0xFF` means no exclusion. |
| 2 | 1 | Trail Skip Cnt | observed | Length of that excluded range. |
| 3 | 1 | Layout | observed | `0x0A` vertical (the only value seen), `0x0B` horizontal, `0x0C`/`0x0D` rotated variants. |
| 4 | 1 | Charset ID | normative | Selects the character encoding (§9, Table 3). The label repeats it in text form. |
| 5 | 1 | Coverage | normative | `0` = full single-byte charset, `1` = full double-byte charset, `2` = subset (only the glyphs in the index). |
| 6 | 1 | Fixed Advance | observed | Monospace advance width. `0` = variable width, where `ink_w` (§6) applies. |
| 7 | 1 | Line Px | normative | Glyph cell height in pixels (*line_px*). Generator tools name files by this value, so it is easily mistaken for the point size. |
| 8 | 1 | Lead First | observed | Lowest lead byte (double-byte charsets). `0x00` or `0xFF` if unused. |
| 9 | 1 | Lead Last | observed | Highest lead byte. |
| 10 | 1 | Trail First | observed | Lowest final byte. For single-byte charsets this is the first code, e.g. `0x20`. |
| 11 | 1 | Trail Last | observed | Highest final byte. |
| 12 | 4 | Glyph Count | normative | Number of glyph index entries (§6). |
| 16 | 1 | Format Rev | normative | `5` or `6`. |
| 17 | 1 | Label Len | normative | Length of the label (§5) in octets. |
| 18 | 2 | Reserved | observed | `0`. |
| 20 | 4 | Payload Length | observed | Byte count from offset 44 to EOF, which equals `file_size − 44`. |
| 24 | 4 | Preamble Length | observed | Always `44`. Readers SHOULD still use the constant 44. |
| 28 | 1 | Lead Skip At | observed | Start of an excluded lead-byte range (§9). |
| 29 | 1 | Lead Skip Cnt | observed | Length of that excluded range. |
| 30 | 1 | Antialiased | inferred | `1` when anti-aliasing was requested at generation time. It is `0` exactly in the `-no-aa` samples. |
| 31 | 1 | Unknown | observed | Always `1`. |
| 32 | 1 | Face Len | observed | Length of the face-name prefix of the label (§5). |
| 33 | 1 | Offset Units | normative | `0`: index offsets (§6) count octets. `1`: they count 8-octet units. |
| 34 | 2 | Reserved | observed | `0`. |
| 36 | 4 | Charset Glyph Count | observed | Rev 6 only (`0` in rev 5). Equals Glyph Count in every sample. |
| 40 | 4 | Reserved | observed | `0`. |

A reader MUST reject a file shorter than 44 octets or whose Magic is
not `0x04`. A reader SHOULD reject a file with an unknown Layout,
Charset ID, or Coverage value.

## 5. Label

```
label      = face-name charset-name        ; exactly Label Len octets, ASCII
face-name  = Face Len octets                ; e.g. "mono50", "Arial-16-iso-8859-1-m"
```

The label directly follows the preamble. It MUST NOT be treated as
NUL-terminated: the octet that follows it is the first glyph index
entry. The charset name is the textual form of Charset ID from Table 3,
e.g. `utf-8` or `iso-8859-1`, and is appended with no separator.
**[normative for length; observed for structure]**

Example: `mono50utf-8` has Label Len 11 and Face Len 6.

## 6. Glyph Index

The index starts at the *index base* and holds `Glyph Count` entries of
10 octets each. **[normative]**

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|        Char ID (u16 LE)       |     Ink W     |   Pad Left    |  0
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|   Pad Right   |           Data Offset (u24 LE)                |  4
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|      Data Length (u16 LE)     |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

| Off | Size | Field | Semantics |
|---:|---:|---|---|
| 0 | 2 | Char ID | Character code in the font's charset. For double-byte charsets, the lead byte is the low octet (§9). |
| 2 | 1 | Ink W | Glyph width in pixels, excluding padding. |
| 3 | 1 | Pad Left | Blank columns to the left of the ink. |
| 4 | 1 | Pad Right | Blank columns to the right of the ink. |
| 5 | 3 | Data Offset | Location of the glyph's packed data, counted from the *index base* (not from the start of the file). Multiply by 8 when Offset Units = 1. |
| 8 | 2 | Data Length | Length of the packed data in octets, including the scheme octet. |

The glyph's packed data therefore occupies file octets:

```
start = index_base + Data Offset × (Offset Units = 1 ? 8 : 1)
end   = start + Data Length                    ; exclusive
cell_w = Ink W + Pad Left + Pad Right
```

Rules:

- Entries SHOULD be sorted by ascending Char ID. **[observed]**
- A generator MAY write duplicate entries. In the `-m` samples, Glyph
  Count is 2 and both entries describe `M` and point at the same data.
  Readers SHOULD tolerate duplicates and keep the first. **[observed]**
- Every sample includes `0x20` (space), even when it was not requested.
  **[observed]**
- A reader MUST reject an entry whose `[start, end)` range extends past
  EOF.

## 7. Glyph Data

### 7.1 Placement

In every sample the first glyph's Data Offset is the index length
(`Glyph Count × 10`) rounded up to a multiple of 8. The gap is filled
with `0x00` octets. After that, each glyph's data follows the previous
one with no gaps, and the last glyph ends exactly at EOF. **[observed]**

When the data region would exceed the 24-bit offset range (2²⁴ octets),
a generator sets Offset Units = 1. It then MUST place every glyph on an
8-octet boundary relative to the index base, using zero padding, and
store `offset / 8`. **[inferred from the decompiled writer; no such
sample available]**

### 7.2 Glyph data unit

```
glyph-data  = scheme-octet *op-octet
```

The first octet selects the packing scheme (§8). The remaining octets
are opcodes that emit coverage values. **[normative]**

Pixels are emitted in **row-major order across the whole cell**, from
the top-left pixel `(0, 0)` to `(cell_w − 1, line_px − 1)`. A run MAY
continue past the end of a row onto the next row. After the last
opcode, the decoder MUST have emitted exactly `cell_w × line_px`
coverage values; any other count is an error. A zero-length glyph data
unit is an empty glyph. **[normative]**

## 8. Pixel Packing Schemes

| Scheme octet | Name | Coverage values used |
|---:|---|---|
| `0x01` | Bilevel RLE | {0, 7} only |
| `0x03` | Grey-3 RLE | 0–7 |

Any other scheme octet MUST be rejected. A single font MAY mix schemes
per glyph: the `mono*` samples use both `0x01` and `0x03`.

Every opcode octet has this layout:

```
  7   6   5   4   3   2   1   0
+---+---+---+---+---+---+---+---+
|  Op   | F |       N           |   N = bits 4..0  (0–31)
+---+---+---+---+---+---+---+---+
|  Op   |     A     |     B     |   A = bits 5..3, B = bits 2..0 (0–7)
+---+---+---+---+---+---+---+---+
```

Each opcode reads the octet with one of the two field views above.
Table 1 gives the meaning under each scheme. `0ⁿ` means n background
pixels, `7ⁿ` means n full-ink pixels, and `v` is a single pixel with
coverage v.

**Table 1: Opcode semantics**

| Op | View | Scheme `0x01` (bilevel) | Scheme `0x03` (grey-3) |
|:--:|:--:|---|---|
| `00` | F,N | F=0: `0ᴺ`; F=1: `7ᴺ` | *same as bilevel* |
| `01` | F,N | `0ᴺ 7¹⁺ᶠ` | *same as bilevel* |
| `10` | F,N / A,B | `0ᴺ 7³⁺ᶠ` (F,N view) | `0ᴬ B` (A,B view) |
| `11` | A,B | `0ᴬ 7ᴮ` | `A B` (two pixels) |

Notes:

- An opcode with N = 0 (Op `00`) emits nothing and is legal.
- Grey-3 Op `10` with B = 7 or B = 0 is legal, although B = 7 is
  normally encoded with Op `01`.
- Converting coverage to 8-bit alpha: `alpha = v × 36` (0, 36, …, 252).
  The original tooling also uses this mapping. **[observed]**
- Only scheme `0x03` can represent anti-aliasing. The `Antialiased`
  samples in `examples/fonts/single-glyph/` nonetheless use `0x01` for
  every glyph, so the generator appears to choose per glyph.
  **[observed]**

### 8.1 Encoding (non-normative)

Any opcode sequence that decodes to the intended raster is valid. The
reference encoder in `src/zi.rs` (`pack_pixels`) uses only scheme
`0x03`. At each position it applies the first matching rule:

1. Run of 7s (length 1–31): Op `00`, F = 1.
2. Run of 0s (length 1–31):
   - followed by one or two 7s: Op `01`, F = (number of 7s − 1);
   - otherwise, if the run is shorter than 8 and followed by a grey
     value g: Op `10` with A = run length, B = g;
   - otherwise: Op `00`, F = 0.
3. Grey pixel g followed by any pixel h: Op `11` with A = g, B = h. If
   g is the last pixel, use Op `10` with A = 0, B = g.

Output of this encoder is not guaranteed to be byte-identical to the
Nextion Editor's output.

## 9. Charset Parameters

**Table 3: Charset ID**

| ID | Name | | ID | Name |
|---:|---|---|---:|---|
| 1 | ascii | | 13 | iso-8859-15 |
| 2 | gb2312 | | 14 | iso-8859-11 |
| 3 | iso-8859-1 | | 15 | ks_c_5601-1987 |
| 4 | iso-8859-2 | | 16 | big5 |
| 5 | iso-8859-3 | | 17 | windows-1255 |
| 6 | iso-8859-4 | | 18 | windows-1256 |
| 7 | iso-8859-5 | | 19 | windows-1257 |
| 8 | iso-8859-6 | | 20 | windows-1258 |
| 9 | iso-8859-7 | | 21 | windows-874 |
| 10 | iso-8859-8 | | 22 | koi8-r |
| 11 | iso-8859-9 | | 23 | shift-jis |
| 12 | iso-8859-13 | | 24 | utf-8 |

**Char ID construction.**

- *Single-byte charsets:* Char ID is the code byte, zero-extended to
  16 bits.
- *Double-byte charsets* (IDs 2, 15, 16, 23): `Char ID = trail << 8 |
  lead`. The two octets therefore appear on disk in stream order: lead
  first, then trail.
- *utf-8* (ID 24): Char ID is the Unicode code point. Only the Basic
  Multilingual Plane (≤ U+FFFF) is representable. **[inferred from the
  decompiled writer]**

**Full-charset ranges** (Coverage 0 or 1). A lead byte `l` is included
if `Lead First ≤ l ≤ Lead Last` and not
`Lead Skip At < l ≤ Lead Skip At + Lead Skip Cnt`. A final byte `t` is
included under the same rule, using the Trail fields. Double-byte
charsets additionally include ASCII `0x20`–`0x7E` as single-byte
entries. The ranges below were produced by the decompiled generator
**[inferred]**:

| Charset | Lead range | Lead skip | Trail range | Trail skip |
|---|---|---|---|---|
| single-byte sets (ascii: 95 codes) | – | – | `0x20`… (224 codes) | none |
| gb2312 | `A1–F7` | none | `A1–FE` | none |
| ks_c_5601-1987 | `A1–C8` | none | `A1–FE` | none |
| big5 | `A0–F9` | none | `40–FE` | `7F–A0` |
| shift-jis | `81–EF` | `A0–DF` | `40–FC` | `7F` |

With Coverage 2 (subset), the ranges describe the charset only and the
index lists exactly the glyphs present.

## 10. Formal Grammar (EBNF)

Notation: W3C XML EBNF. `#xNN` is one octet, and `[#xNN-#xMM]` is an
octet in that range. Grammar rules annotated with `(: … :)` carry
constraints that cannot be expressed in a context-free grammar (length
prefixes and offsets). These constraints are normative and are
restated in prose in §4–§8.

```ebnf
zi-file          ::= preamble label glyph-index align-pad glyph-data*

(: ---------------- §4 preamble: exactly 44 octets ---------------- :)
preamble         ::= magic trail-skip-at trail-skip-cnt layout
                     charset-id coverage fixed-advance line-px
                     lead-first lead-last trail-first trail-last
                     glyph-count
                     format-rev label-len reserved16
                     payload-len preamble-len
                     lead-skip-at lead-skip-cnt antialiased unknown31
                     face-len offset-units reserved16
                     charset-glyph-count
                     reserved32

magic            ::= #x04
layout           ::= [#x0A-#x0D]
charset-id       ::= [#x01-#x18]
coverage         ::= #x00 | #x01 | #x02
format-rev       ::= #x05 | #x06
offset-units     ::= #x00 | #x01
antialiased      ::= #x00 | #x01
unknown31        ::= OCTET
preamble-len     ::= #x2C #x00 #x00 #x00
glyph-count      ::= U32          (: = N, number of index-entry below :)
label-len        ::= U8           (: = L, length of label :)
face-len         ::= U8           (: ≤ L :)
payload-len      ::= U32          (: = file size − 44 :)
charset-glyph-count ::= U32
trail-skip-at    ::= U8
trail-skip-cnt   ::= U8
lead-skip-at     ::= U8
lead-skip-cnt    ::= U8
fixed-advance    ::= U8
line-px          ::= U8           (: = H :)
lead-first       ::= U8
lead-last        ::= U8
trail-first      ::= U8
trail-last       ::= U8
reserved16       ::= #x00 #x00
reserved32       ::= #x00 #x00 #x00 #x00

(: ---------------- §5 label ---------------- :)
label            ::= face-name charset-name
                                  (: exactly L octets; NOT NUL-terminated :)
face-name        ::= ASCII*       (: exactly face-len octets :)
charset-name     ::= "ascii" | "gb2312" | "iso-8859-" [0-9]+
                   | "ks_c_5601-1987" | "big5" | "windows-" [0-9]+
                   | "koi8-r" | "shift-jis" | "utf-8"

(: ---------------- §6 glyph index ---------------- :)
glyph-index      ::= index-entry*  (: exactly N entries :)
index-entry      ::= char-id ink-w pad-left pad-right data-offset data-length
char-id          ::= U16
ink-w            ::= U8
pad-left         ::= U8
pad-right        ::= U8
data-offset      ::= U24          (: × 8 when offset-units = #x01;
                                     relative to start of glyph-index :)
data-length      ::= U16          (: length of the referenced glyph-data :)

(: ---------------- §7 glyph data ---------------- :)
align-pad        ::= #x00*        (: 0–7 octets :)
glyph-data       ::= bilevel-glyph | grey3-glyph
                                  (: located by index-entry; decodes to
                                     exactly (ink-w + pad-left + pad-right)
                                     × H pixels :)

(: ---------------- §8 packing schemes ---------------- :)
bilevel-glyph    ::= #x01 bilevel-op*
grey3-glyph      ::= #x03 grey3-op*

bilevel-op       ::= run-bg | run-ink | bg-then-ink12
                   | bg-then-ink34 | bg-a-then-ink-b
grey3-op         ::= run-bg | run-ink | bg-then-ink12
                   | bg-a-then-grey  | grey-pair

run-bg           ::= [#x00-#x1F]  (: Op=00 F=0: N × 0 :)
run-ink          ::= [#x20-#x3F]  (: Op=00 F=1: N × 7 :)
bg-then-ink12    ::= [#x40-#x7F]  (: Op=01: N × 0, then (1+F) × 7 :)
bg-then-ink34    ::= [#x80-#xBF]  (: Op=10, bilevel: N × 0, then (3+F) × 7 :)
bg-a-then-grey   ::= [#x80-#xBF]  (: Op=10, grey-3: A × 0, then 1 pixel = B :)
bg-a-then-ink-b  ::= [#xC0-#xFF]  (: Op=11, bilevel: A × 0, then B × 7 :)
grey-pair        ::= [#xC0-#xFF]  (: Op=11, grey-3: pixel A, then pixel B :)

(: ---------------- primitives (little-endian) ---------------- :)
OCTET            ::= [#x00-#xFF]
U8               ::= OCTET
U16              ::= OCTET OCTET
U24              ::= OCTET OCTET OCTET
U32              ::= OCTET OCTET OCTET OCTET
ASCII            ::= [#x20-#x7E]
```

## 11. Worked Examples

### 11.1 Preamble: `mono50.zi`

```
00: 04 ff 00 0a 18 00 00 32  ff ff 00 ff 5f 00 00 00
10: 06 0b 00 00 e7 31 00 00  2c 00 00 00 ff 00 01 01
20: 06 00 00 00 5f 00 00 00  00 00 00 00
```

| Field | Raw | Value |
|---|---|---|
| Magic / Layout | `04` / `0a` | valid, vertical |
| Charset ID / Coverage | `18` / `00` | utf-8, full single-byte |
| Fixed Advance / Line Px | `00` / `32` | variable width, 50 px |
| Glyph Count | `5f 00 00 00` | 95 |
| Format Rev / Label Len | `06` / `0b` | rev 6, 11 octets (`mono50utf-8`) |
| Payload Length | `e7 31 00 00` | 12775 = 12819 − 44 |
| Antialiased / Face Len / Offset Units | `01` / `06` / `00` | yes, `mono50`, byte offsets |

### 11.2 Glyph: space (`0x20`) in `Arial-16-iso-8859-1-m.zi`

The index entry is `20 00 04 00 00 18 00 00 04 00`:

| Field | Value |
|---|---|
| Char ID | `0x0020` |
| Ink W | 4 |
| Pad Left, Pad Right | 0, 0 |
| Data Offset | 24 (Glyph Count 2 × 10 = 20, rounded up to 24) |
| Data Length | 4 |

The data is `01 1f 1f 02`:

| Octet | Meaning |
|---|---|
| `01` | bilevel scheme |
| `1f` | Op 00, F=0, N=31: 31 background pixels |
| `1f` | 31 more background pixels |
| `02` | 2 more background pixels |

Total: 64 background pixels, which is `4 × 16`, so the pixel count
checks out.

### 11.3 Opcode decoding

| Octet | Binary | Scheme 1 | Scheme 3 |
|---|---|---|---|
| `0x25` | `00 1 00101` | `7⁵` | `7⁵` |
| `0x63` | `01 1 00011` | `0³ 7²` | `0³ 7²` |
| `0x9A` | `10 0 11010` / `10 011 010` | `0²⁶ 7³` | `0³ 2` |
| `0xD7` | `11 010 111` | `0² 7⁷` | `2 7` |

## 12. Embedding in `.tft`

Each font used by a project is embedded in the compiled `.tft`. The
three `mono*` fonts appear in `tests/fixtures/h5.tft` at `0x9fa88`,
`0xa567b`, and `0xa888c`, in the order mono72, mono50, mono60. They can
be located by searching for the label string. **[observed]**

It is still unknown how preambles relate to label, index, and data
blocks inside the `.tft`, and where the table that maps font ids to
offsets lives. The earlier comparison between the embedded and
standalone indexes used an incorrect index layout (see §14), so it
should be redone using §6.

Consequences for this toolkit:

- `compile` can reference a font id that already exists in a scaffold
  `.tft`.
- It cannot add a font or resolve font id → block offset.

## 13. Open Questions

1. The purpose of preamble octet 31 (always `1`).
2. Whether octet 30 is truly an anti-aliasing flag, and why AA-requested
   samples still contain only bilevel glyphs.
3. Whether Charset Glyph Count (octets 36–39) ever differs from Glyph
   Count in Editor-generated files.
4. Semantics of the non-vertical Layout values. No samples are
   available.
5. `.tft` font directory and preamble placement (§12).
6. Rev 5 files and Offset Units = 1 files. The spec for these comes
   from the decompiled writer and has not been validated against real
   samples.

## 14. Provenance and Errata

This specification was derived from (a) the reference files in
`examples/fonts/` and (b) static analysis of a third-party .NET library
for the format, decompiled with ILSpy. The Rust implementation in
`src/zi.rs` is an independent reimplementation and uses its own naming
throughout.

Corrections to the previous revision of this document:

The previous revision read the label as NUL-terminated followed by a
space. It therefore consumed the first index entry's Char ID (`20 00`,
the space character) as `" \0"`, and every index field after that was
read 2 octets late. Most earlier "mysteries" follow from that shift:

| Previous claim | Correct |
|---|---|
| Octet 7 is point size | It is the cell height in pixels (Line Px). |
| Label is NUL-terminated, followed by a space | It is length-prefixed via octet 17. The `20 00` was the space glyph's Char ID. |
| Index entry = height u16, offset u32, width u16, code u16 (shifted 2 octets) | Char ID u16, Ink W u8, Pad L u8, Pad R u8, Offset u24, Length u16 (§6). |
| "height" constant per font (e.g. 37) | That was Ink W + (Pad Left << 8), and Ink W is constant in a monospace font. |
| Offset = `base + 256·Σwidth`, "synthetic" | The shifted u32 was `Pad Right \| Offset << 8`, so it is the real offset × 256. The "width" was Data Length, which is why the running sum matched. Base 243712 = 952 × 256. |
| "width" of `M` = 4, implausibly narrow | That was the space glyph's Data Length (4 octets: `01 1f 1f 02`). |
| Extra "sentinel" index row after the real glyphs | Rows are genuine. Space is always present, and `-m` files contain a duplicate `M` entry. |
| Glyph raster encoding not decoded | Fully specified in §7–§8. |

[RFC 2119]: https://www.rfc-editor.org/rfc/rfc2119
