# Targets

This toolkit supports exactly **one** display: the **NX8048P050-011R-Y**
(800×480, Nextion Enhanced series). That's not a placeholder for "more to
come soon" — it's the honest boundary of what's actually been verified.

## Why only one

Every byte offset in [`src/tft.rs`](../src/tft.rs) and every record shape
in [`src/hmi.rs`](../src/hmi.rs) was derived by reverse-engineering a
single real project: a `.HMI`/`.tft` pair compiled by the real Nextion
Editor for this exact display. The technique (documented in
[`docs/formats/`](formats/)) was:

1. Decode the `.HMI` to get known-good values (an attribute's name,
   position, color, text).
2. Search the compiled `.tft` for byte patterns matching those known
   values.
3. Diff two structurally similar components (two buttons, two labels) to
   see which surrounding bytes vary and which are constant across both —
   the constant ones are either format markers or this-target constants,
   and only repeating the process against *another* target's reference
   files can tell which is which.

None of that process has been repeated against a second target. It's
entirely possible — likely, even — that:

- A different resolution changes the header layout, or field widths.
- A different Nextion series (Basic vs. Enhanced vs. Intelligent) changes
  the component record shape entirely.
- Even a same-resolution Enhanced-series display compiled by a different
  Editor version shifts something in the "reserved" bytes we've assumed
  are constant.

Shipping a `Target` enum with a second, *unverified* variant would be
worse than not having one: it would look supported while silently
producing corrupted output the first time an assumption doesn't hold.

## What the `Target` type is for, then

[`src/target.rs`](../src/target.rs) exists so the *rest* of the codebase
doesn't hardcode `800`/`480`/model strings inline — every place that cares
about target dimensions or identity goes through `Target`. That's
deliberate scaffolding for the day a second target is added, not evidence
that one is supported today. [`tft::parse_header`](../src/tft.rs) uses it
to hard-fail (not silently proceed) if you point the toolkit at a `.tft`
compiled for a different resolution.

## What it would take to add a second target

1. **A real reference pair.** A `.HMI` project compiled for the new
   target, and the `.tft` that Nextion Editor produces from it. Without
   both, there's nothing to verify offsets against — don't guess from the
   Nextion protocol docs or by analogy to this target's offsets.
2. **Re-run the diffing process** from `docs/formats/nextion-tft-format.md`
   against the new pair: confirm the header layout, confirm (or find
   differences in) the 84-byte text-component record, confirm the
   text-pool slot size and layout.
3. **Add a `Target` variant** with its own `width()`/`height()`, and only
   change the offset/layout constants in `tft.rs` if the new target's
   diffing showed they actually differ — don't assume they're identical
   just because they were for this one target.
4. **Add reference fixtures** for the new target under `tests/fixtures/`
   and duplicate the integration-test pattern in `tests/hmi_decode.rs` /
   `tests/tft_patch.rs` against them, so a regression in one target's
   support can't silently break unnoticed while working on the other.
5. Update this document and the format docs to mark the new target's
   findings with the same **[confirmed]**/**[hypothesis]** rigor as the
   first one — resist the temptation to mark something confirmed just
   because it matched on one file.
