# Fixtures

`h5.HMI` / `h5.tft` are real Nextion Editor project/compiled-firmware
files (a NanoEls H5 lathe UI, the same project the original
reverse-engineering session's format docs were derived from). Both are
genuine Editor output for the NX8048P050-011R-Y, used by the `hmi_decode`,
`hmi_patch`, and `tft_patch` integration tests.

**They are not a matched scaffold pair** — the embedded build-date strings
differ between them (`h5.HMI` decodes to a different date than the text
baked into `h5.tft`), meaning they came from two different compiles of the
project, not `h5.tft` compiled directly from `h5.HMI`. Each file is
internally self-consistent and fine to test independently, but
`compile` specifically requires the `.HMI` and `.tft` to be the *same*
compiled project (see `docs/spec-format.md`) — these two don't satisfy
that, so `compile`'s integration tests (`tests/compile.rs`) use small
synthetic fixtures built in-test instead.
