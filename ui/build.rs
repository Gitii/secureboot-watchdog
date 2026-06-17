// Compile Blueprint files to .ui XML, then bundle them (plus style.css) into a
// single .gresource that the binary embeds via include_bytes!.
//
// Requires `blueprint-compiler` and `glib-compile-resources` on PATH. Inside
// `devbox shell` both come from nixpkgs; on a bare noble box they come from
// the `blueprint-compiler` and `libglib2.0-bin` apt packages, which are
// listed in debian/control Build-Depends.

use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let blueprints_dir = manifest.join("blueprints");
    let data_dir = manifest.join("data");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ui_out = out_dir.join("ui");
    std::fs::create_dir_all(&ui_out).expect("create OUT_DIR/ui");

    println!("cargo:rerun-if-changed=blueprints");
    println!("cargo:rerun-if-changed=data/style.css");
    println!("cargo:rerun-if-changed=data/io.github.gitii.SecurebootWatchdog.gresource.xml");

    // 1. Compile each .blp -> .ui in OUT_DIR/ui
    let blps: Vec<PathBuf> = std::fs::read_dir(&blueprints_dir)
        .expect("read blueprints/")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("blp"))
        .collect();
    assert!(
        !blps.is_empty(),
        "no .blp files in {}",
        blueprints_dir.display()
    );

    let mut cmd = Command::new("blueprint-compiler");
    cmd.arg("batch-compile").arg(&ui_out).arg(&blueprints_dir);
    for p in &blps {
        cmd.arg(p);
    }
    let status = cmd.status().expect("run blueprint-compiler");
    assert!(status.success(), "blueprint-compiler failed");

    // 2. Copy style.css alongside the .ui files so the gresource manifest can
    //    find it via sourcedir.
    std::fs::copy(data_dir.join("style.css"), ui_out.join("style.css")).expect("copy style.css");

    // 3. Bundle into a .gresource.
    let manifest_file = data_dir.join("io.github.gitii.SecurebootWatchdog.gresource.xml");
    let gresource_path = out_dir.join("secureboot-watchdog-ui.gresource");
    let status = Command::new("glib-compile-resources")
        .arg(format!("--sourcedir={}", ui_out.display()))
        .arg(format!("--target={}", gresource_path.display()))
        .arg(&manifest_file)
        .status()
        .expect("run glib-compile-resources");
    assert!(status.success(), "glib-compile-resources failed");
}
