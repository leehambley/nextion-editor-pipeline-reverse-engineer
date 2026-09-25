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
