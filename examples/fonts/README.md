# Font fixtures

`mono72.zi`, `mono60.zi`, `mono50.zi` are real Nextion Editor-compiled
font files (three point sizes of the same "mono" font used by the
reference `h5.HMI`/`h5.tft` project in `tests/fixtures/`), used as the
basis for [`docs/formats/nextion-zi-font-format.md`](../../docs/formats/nextion-zi-font-format.md).

They live under `examples/` rather than `tests/fixtures/` because nothing
in this toolkit's test suite depends on them yet — no code here parses
`.zi` glyph data (see the format doc's §5 for why: the glyph
raster/vector encoding hasn't been reverse-engineered). They're kept as
a citable reference for that future work, and so the format doc's byte
offsets/examples can be checked against real files without needing
Nextion Editor to regenerate them.

## `single-glyph/`

Four additional `.zi` files, generated later specifically to reduce the
degrees of freedom in the glyph-decoding investigation: Arial, 16pt,
ISO-8859-1 encoding, each with either 1 character (`M` alone) or 3
(`abc`), with and without anti-aliasing — `Arial-16-iso-8859-1-{m,abc}
[-no-aa].zi`. Generated with the Nextion Editor's own Font Generator
tool (not the same tool/machine/Editor version as `mono72/60/50.zi`
above — confirmed by a different header byte layout at the same
offsets, e.g. a different fixed prefix at `0x03-0x0A`). Having a
*single*-glyph file removes the "where does this glyph's data end"
ambiguity that blocked progress with the multi-glyph `mono*.zi` files —
see `docs/formats/nextion-zi-font-format.md` §5 for what was (and
wasn't) figured out using them.
