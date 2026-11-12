//! Integration tests for the `dusrelimage` binary.
//!
//! Every test builds its fixtures with `dusrelimage mock` (deterministic
//! seed), runs the real binary via `CARGO_BIN_EXE_dusrelimage`, and checks
//! exit codes plus stdout / output files. No binary blobs are committed.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dusrelimage"))
}

/// A fresh scratch directory per test (unique per process + test name).
fn tmpdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dusrelimage-cli-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create tmpdir");
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .output()
        .expect("run dusrelimage binary")
}

fn run_in(dir: &PathBuf, args: &[String]) -> Output {
    Command::new(bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run dusrelimage binary")
}

fn s(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// `dusrelimage mock --kind solid --width 64 --height 48 -o <name>`, asserting success.
fn mock_solid(dir: &Path, name: &str) -> PathBuf {
    let out = dir.join(name);
    let result = run(&[
        "mock",
        "--kind",
        "solid",
        "--width",
        "64",
        "--height",
        "48",
        "-o",
        &s(&out),
    ]);
    assert!(
        result.status.success(),
        "mock failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(out.exists(), "mock output {name} was not created");
    assert!(
        std::fs::metadata(&out).expect("stat mock").len() > 0,
        "mock output {name} is empty"
    );
    out
}

#[test]
fn mock_round_trip_is_deterministic() {
    let dir = tmpdir("mock-deterministic");
    let a = mock_solid(&dir, "a.png");
    let b = mock_solid(&dir, "b.png");
    assert_eq!(
        std::fs::read(&a).expect("read a"),
        std::fs::read(&b).expect("read b"),
        "same seed must give identical bytes"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn info_reports_mock_dimensions() {
    let dir = tmpdir("info");
    let img = mock_solid(&dir, "img.png");
    let result = run(&["info", &s(&img)]);
    assert!(result.status.success(), "info failed");
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.contains("width: 64"), "stdout was: {stdout}");
    assert!(stdout.contains("height: 48"), "stdout was: {stdout}");
    assert!(stdout.contains("format:"), "stdout was: {stdout}");
    assert!(stdout.contains("has_alpha:"), "stdout was: {stdout}");
    assert!(stdout.contains("file_size:"), "stdout was: {stdout}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn compress_round_trip() {
    let dir = tmpdir("compress");
    let img = mock_solid(&dir, "img.png");
    let out = dir.join("out.png");
    let result = run(&["compress", &s(&img), "-o", &s(&out), "--quality", "70"]);
    assert!(
        result.status.success(),
        "compress failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(out.exists() && std::fs::metadata(&out).expect("stat").len() > 0);
    // The compressed output is still a readable image.
    let info = run(&["info", &s(&out)]);
    assert!(info.status.success(), "info on compressed output failed");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn compress_to_target_size() {
    let dir = tmpdir("compress-target");
    let img = mock_solid(&dir, "img.png");
    let out = dir.join("small.jpg");
    let result = run(&[
        "compress",
        &s(&img),
        "-o",
        &s(&out),
        "--format",
        "jpeg",
        "--target-size",
        "20000",
    ]);
    assert!(
        result.status.success(),
        "compress --target-size failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        std::fs::metadata(&out).expect("stat").len() <= 20_000,
        "output exceeds the requested target size"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn resize_round_trip() {
    let dir = tmpdir("resize");
    let img = mock_solid(&dir, "img.png");
    let out = dir.join("small.png");
    let result = run(&[
        "resize",
        &s(&img),
        "-o",
        &s(&out),
        "--width",
        "32",
        "--height",
        "24",
    ]);
    assert!(
        result.status.success(),
        "resize failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let info = run(&["info", &s(&out)]);
    assert!(info.status.success());
    let stdout = String::from_utf8_lossy(&info.stdout);
    assert!(stdout.contains("width: 32"), "stdout was: {stdout}");
    assert!(stdout.contains("height: 24"), "stdout was: {stdout}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn resize_by_percent() {
    let dir = tmpdir("resize-percent");
    let img = mock_solid(&dir, "img.png");
    let out = dir.join("half.png");
    let result = run(&["resize", &s(&img), "-o", &s(&out), "--percent", "50"]);
    assert!(
        result.status.success(),
        "resize --percent failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let info = run(&["info", &s(&out)]);
    let stdout = String::from_utf8_lossy(&info.stdout);
    assert!(stdout.contains("width: 32"), "stdout was: {stdout}");
    assert!(stdout.contains("height: 24"), "stdout was: {stdout}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn crop_round_trip() {
    let dir = tmpdir("crop");
    let img = mock_solid(&dir, "img.png");
    let out = dir.join("crop.png");
    let result = run(&[
        "crop",
        &s(&img),
        "-o",
        &s(&out),
        "--x",
        "8",
        "--y",
        "8",
        "--width",
        "16",
        "--height",
        "16",
    ]);
    assert!(
        result.status.success(),
        "crop failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let info = run(&["info", &s(&out)]);
    let stdout = String::from_utf8_lossy(&info.stdout);
    assert!(stdout.contains("width: 16"), "stdout was: {stdout}");
    assert!(stdout.contains("height: 16"), "stdout was: {stdout}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn convert_round_trip() {
    let dir = tmpdir("convert");
    let img = mock_solid(&dir, "img.png");
    let out = dir.join("out.jpg");
    let result = run(&["convert", &s(&img), "-o", &s(&out), "--format", "jpeg"]);
    assert!(
        result.status.success(),
        "convert failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let info = run(&["info", &s(&out)]);
    let stdout = String::from_utf8_lossy(&info.stdout);
    assert!(
        stdout.to_ascii_lowercase().contains("jpeg"),
        "stdout was: {stdout}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn crop_out_of_bounds_fails() {
    let dir = tmpdir("crop-oob");
    let img = mock_solid(&dir, "img.png");
    let out = dir.join("crop.png");
    let result = run(&[
        "crop",
        &s(&img),
        "-o",
        &s(&out),
        "--x",
        "1000",
        "--y",
        "1000",
        "--width",
        "10",
        "--height",
        "10",
    ]);
    assert!(
        !result.status.success(),
        "crop out of bounds must exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("CROP_OUT_OF_BOUNDS"),
        "stderr was: {stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn invalid_image_fails() {
    let dir = tmpdir("invalid-image");
    let bad = dir.join("bad.png");
    std::fs::write(&bad, b"this is not an image").expect("write bad file");
    let result = run(&["info", &s(&bad)]);
    assert!(!result.status.success(), "info on garbage must fail");
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("ERROR"), "stderr was: {stderr}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pick_colour_and_palette_on_mock() {
    let dir = tmpdir("pick-palette");
    let img = mock_solid(&dir, "img.png");
    let pick = run(&["pick-colour", &s(&img), "--x", "0", "--y", "0"]);
    assert!(
        pick.status.success(),
        "pick-colour failed: {}",
        String::from_utf8_lossy(&pick.stderr)
    );
    let stdout = String::from_utf8_lossy(&pick.stdout);
    assert!(stdout.contains('#'), "stdout was: {stdout}");
    assert!(stdout.contains("r:"), "stdout was: {stdout}");
    assert!(stdout.contains("hsl:"), "stdout was: {stdout}");

    let palette = run(&["palette", &s(&img), "--count", "3"]);
    assert!(
        palette.status.success(),
        "palette failed: {}",
        String::from_utf8_lossy(&palette.stderr)
    );
    let stdout = String::from_utf8_lossy(&palette.stdout);
    assert!(stdout.contains("share:"), "stdout was: {stdout}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn collage_and_pixelate_round_trip() {
    let dir = tmpdir("collage-pixelate");
    let a = mock_solid(&dir, "a.png");
    let b = mock_solid(&dir, "b.png");
    let collage = dir.join("collage.png");
    let result = run_in(
        &dir,
        &[
            "collage".to_string(),
            s(&a),
            s(&b),
            "-o".to_string(),
            s(&collage),
            "--layout".to_string(),
            "2x1".to_string(),
            "--width".to_string(),
            "128".to_string(),
            "--height".to_string(),
            "64".to_string(),
        ],
    );
    assert!(
        result.status.success(),
        "collage failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let info = run(&["info", &s(&collage)]);
    let stdout = String::from_utf8_lossy(&info.stdout);
    assert!(stdout.contains("width: 128"), "stdout was: {stdout}");
    assert!(stdout.contains("height: 64"), "stdout was: {stdout}");

    let pix = dir.join("pix.png");
    let result = run(&["pixelate", &s(&a), "-o", &s(&pix), "--block-size", "8"]);
    assert!(
        result.status.success(),
        "pixelate failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(pix.exists());
    let _ = std::fs::remove_dir_all(&dir);
}
