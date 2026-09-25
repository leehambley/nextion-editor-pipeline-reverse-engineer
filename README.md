# nextion-tft-toolkit

Decode and patch [Nextion](https://nextion.tech/) `.HMI` project files and
compiled `.tft` firmware from the command line — without needing the
(closed-source, Windows-only) Nextion Editor for every change.

## What this is not

This is **not** a general-purpose Nextion compiler. Please read this
section before filing an issue that this doesn't do X.

- **One supported display: the NX8048P050-011R-Y** (800×480, Nextion
  Enhanced series). This is the only display we have real
  Editor-compiled reference files for, and every byte offset in this
  toolkit was reverse-engineered against those files specifically. Point
  it at a `.tft` compiled for any other resolution or model and it will
  refuse outright rather than risk producing corrupted output. See
  [`docs/targets.md`](docs/targets.md) for why, and what adding a second
  target would actually require.
- **No font or image pipeline.** Fonts and pictures are read/referenced
  by id, never generated or re-encoded. `compile` can point a component
  at a font id that's already compiled into your scaffold; it can't add
  a new font.
- **A subset of controls.** The compiled-record layout is only confirmed
  for text-type (`t`) components. Text and geometry patching work on any
  component type (they search for byte patterns rather than needing to
  know the record shape), but color/font patching is refused for anything
  that isn't `t` — see [`docs/formats/nextion-tft-format.md`](docs/formats/nextion-tft-format.md) §2.
- **No component add/remove.** Neither the `.HMI` project format nor the
  `.tft` compiled format's component-count bookkeeping has been cracked
  yet. This toolkit only ever changes *existing* components' text,
  position, color, or font — never the component list itself.
- **Nothing here has been re-verified against the real Nextion Editor in
  this repository's own history.** The `.HMI` patch path in particular
  (same-length attribute overwrites) is implemented exactly per the
  reverse-engineered record format and passes byte-level tests against a
  real reference file, but "the bytes look right" and "the Editor still
  opens it" are different claims — see the validation loop in
  [`docs/formats/nextion-hmi-format.md`](docs/formats/nextion-hmi-format.md) §6
  before trusting a `.HMI` patch on a project you care about.

If you need something outside these bounds — another target, button
color patching, adding components — the format docs under `docs/formats/`
describe exactly what's confirmed vs. hypothesis, and what reverse-engineering
work remains to get there.

## Why this exists

Nextion's Editor is the only supported way to go from a UI design to a
flashable `.tft`. For small, repetitive changes — retexting a label,
nudging a button, swapping a color — round-tripping through a GUI editor
on every change is slow and not scriptable. This toolkit automates the one
workflow that's actually been proven safe: build one reference project
(a **scaffold**) in the Editor, compile it once, and from then on patch
the compiled output directly from a version-controlled YAML spec.

## Installation

```bash
cargo install --path .
```

This installs the `nxtft` binary.

## Usage

### Decode a `.HMI` to readable YAML

```bash
nxtft hmi-decode my-project.HMI decoded.yaml
```

### Patch a `.HMI` attribute in place (same length only)

```bash
nxtft hmi-patch my-project.HMI patched.HMI --set "page1:bMode:txt=GEARED"
```

`PAGE:OBJNAME:ATTR=VALUE`. The new value must be the same byte length as
the old one — see [`docs/formats/nextion-hmi-format.md`](docs/formats/nextion-hmi-format.md).

### Patch text or geometry directly in a compiled `.tft`

```bash
nxtft tft-patch-text my-project.tft patched.tft --set "OFF=ON!"
nxtft tft-patch-geom my-project.tft patched.tft --set "100,0,183,50=100,0,200,60"
```

Both search for the current bytes and refuse to guess if the match isn't
unique (`--at OFFSET` disambiguates for geometry).

### Compile a scaffold against a YAML spec

```bash
nxtft compile \
  --scaffold-hmi scaffold.HMI \
  --scaffold-tft scaffold.tft \
  --spec ui-spec.yaml \
  -o output.tft
```

See [`docs/spec-format.md`](docs/spec-format.md) for the spec schema and
[`examples/els-page0.yaml`](examples/els-page0.yaml) for a full worked
example, and [`docs/formats/nextion-tft-format.md`](docs/formats/nextion-tft-format.md) §6
for why a scaffold is required at all.

## Testing

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Integration tests run against real Nextion Editor-compiled reference
files under `tests/fixtures/` (a NanoEls H5 lathe UI project, the same
files this toolkit's format docs were reverse-engineered from), not just
synthetic fixtures.

## History

This toolkit started as a reverse-engineering session (preserved under
[`inputs-from-claude/`](inputs-from-claude/)) that produced the original
format docs and a working Python reference implementation
(`hmi_tool.py`, `tft_tool.py`). This repository is a from-scratch Rust
rewrite of that same logic, with the format docs cleaned up and the
`compile` pipeline added, backed by tests against the real reference
project rather than manual spot-checks. See
[`inputs-from-claude/`](inputs-from-claude/) for the raw session output
this was built from.

## License

MIT — see [`LICENSE`](LICENSE).
