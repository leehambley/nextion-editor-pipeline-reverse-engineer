//! CLI-level tests via `assert_cmd`, covering each subcommand's happy path
//! plus a couple of documented error paths.

use assert_cmd::Command;
use predicates::prelude::*;

fn nxtft() -> Command {
    Command::cargo_bin("nxtft").expect("binary must build")
}

#[test]
fn hmi_decode_happy_path_reports_page_and_component_counts() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    nxtft()
        .args(["hmi-decode", "tests/fixtures/h5.HMI"])
        .arg(tmp.path())
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "decoded 6 page(s), 281 component(s)",
        ));

    let contents = std::fs::read_to_string(tmp.path()).unwrap();
    let parsed: serde_yaml::Value = serde_yaml::from_str(&contents).expect("must be valid YAML");
    assert!(parsed.get("pages").is_some());
}

#[test]
fn hmi_patch_happy_path_and_round_trip() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    nxtft()
        .args(["hmi-patch", "tests/fixtures/h5.HMI"])
        .arg(tmp.path())
        .args(["--set", ":bMode:txt=GEARBX"])
        .assert()
        .success()
        .stdout(predicate::str::contains("patched :bMode:txt=GEARBX"));

    let decoded_out = tempfile::NamedTempFile::new().unwrap();
    nxtft()
        .args(["hmi-decode"])
        .arg(tmp.path())
        .arg(decoded_out.path())
        .assert()
        .success();
    let contents = std::fs::read_to_string(decoded_out.path()).unwrap();
    assert!(contents.contains("GEARBX"));
}

#[test]
fn hmi_patch_rejects_length_mismatch_with_clear_error() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    nxtft()
        .args(["hmi-patch", "tests/fixtures/h5.HMI"])
        .arg(tmp.path())
        .args(["--set", ":bMode:txt=WAYTOOLONGOFATEXTVALUE"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("length mismatch"));
}

#[test]
fn tft_patch_text_happy_path() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    nxtft()
        .args(["tft-patch-text", "tests/fixtures/h5.tft"])
        .arg(tmp.path())
        .args(["--set", "OFF=AWY"])
        .assert()
        .success()
        .stdout(predicate::str::contains("patched text"));

    let data = std::fs::read(tmp.path()).unwrap();
    assert!(data.windows(3).any(|w| w == b"AWY"));
}

#[test]
fn tft_patch_text_rejects_wrong_target() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    nxtft()
        .args(["tft-patch-text", "tests/fixtures/h5.tft"])
        .arg(tmp.path())
        .args(["--set", "OFF=AWY"])
        .args(["--target", "NX4832K035"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unsupported target"));
}

#[test]
fn tft_patch_geom_ambiguous_match_reports_offsets_and_hint() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    // Any quad known to repeat in the real file without --at should refuse.
    nxtft()
        .args(["tft-patch-geom", "tests/fixtures/h5.tft"])
        .arg(tmp.path())
        .args(["--set", "100,0,183,60=100,0,183,60"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("not unique"));
}
