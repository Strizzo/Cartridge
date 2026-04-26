//! Visual regression test: spawns the snapshot binary in a subprocess to
//! capture screens, then diffs them against baselines committed in
//! `tests/baseline/`.
//!
//! Subprocess isolation is required: `cargo test`'s panic handler doesn't
//! play nicely with SDL2 on macOS, causing aborts during cleanup.
//!
//! To update baselines (after an intentional UI change):
//!     UPDATE_SNAPSHOTS=1 cargo test --test snapshot_test
//!
//! To run normally:
//!     cargo test --test snapshot_test

use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn baseline_dir() -> PathBuf {
    workspace_root().join("tests/baseline")
}

fn snapshots_dir() -> PathBuf {
    workspace_root().join("snapshots")
}

fn pixel_diff(a: &Path, b: &Path) -> (usize, usize) {
    let img_a = image::open(a).expect("load a").to_rgba8();
    let img_b = image::open(b).expect("load b").to_rgba8();
    if img_a.dimensions() != img_b.dimensions() {
        panic!("dimension mismatch");
    }
    let mut diff = 0;
    for (pa, pb) in img_a.pixels().zip(img_b.pixels()) {
        if pa != pb {
            diff += 1;
        }
    }
    (diff, (img_a.width() * img_a.height()) as usize)
}

// Snapshot tests are pixel-exact and platform-dependent (font rendering
// and anti-aliasing differ between macOS and Linux). They're meant for
// local development as a regression catcher, not for CI. Skip in CI.
#[test]
#[cfg_attr(any(), ignore)]
fn ui_snapshots() {
    if std::env::var("CI").is_ok() {
        eprintln!("skipping snapshot test in CI (platform-dependent baselines)");
        return;
    }
    // Run the snapshot binary, which generates PNGs in `snapshots/`.
    // Use the same target the test was built with (debug or release).
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let status = Command::new(cargo)
        .args(["run", "--bin", "snapshot", "--quiet"])
        .current_dir(workspace_root())
        .status()
        .expect("failed to invoke cargo run snapshot");
    assert!(status.success(), "snapshot binary failed");

    let updating = std::env::var("UPDATE_SNAPSHOTS").as_deref() == Ok("1");
    let snapshots = snapshots_dir();
    let baselines = baseline_dir();
    std::fs::create_dir_all(&baselines).ok();

    let names = ["home", "store", "settings", "app_detail"];
    let mut errors = Vec::new();

    for name in &names {
        let current = snapshots.join(format!("{name}.png"));
        let baseline = baselines.join(format!("{name}.png"));

        if !current.exists() {
            errors.push(format!("{name}: snapshot not generated"));
            continue;
        }

        if updating || !baseline.exists() {
            std::fs::copy(&current, &baseline).expect("copy baseline");
            eprintln!("Updated baseline: {}", baseline.display());
            continue;
        }

        let (diff, total) = pixel_diff(&baseline, &current);
        let pct = diff as f64 / total as f64 * 100.0;
        let threshold_pct = 1.0;
        if pct > threshold_pct {
            errors.push(format!(
                "{name}: {diff}/{total} pixels differ ({pct:.2}% > {threshold_pct}%)"
            ));
        } else {
            eprintln!("{name}: ok ({diff} px, {pct:.3}%)");
        }
    }

    if !errors.is_empty() {
        panic!(
            "Snapshot regressions:\n  {}\n\nTo update: UPDATE_SNAPSHOTS=1 cargo test --test snapshot_test",
            errors.join("\n  "),
        );
    }
}
