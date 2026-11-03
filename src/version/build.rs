/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
use std::path::{Path, PathBuf};
use std::process::Command;

fn rerun_if_changed(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.to_str().unwrap());
}

pub fn main() {
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let package_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = package_root.join("../..");

    // Try to get the version using `git describe`, otherwise fall back to the
    // Cargo.toml version. This is used in main.rs

    let toml_version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let version = Command::new("git").arg("describe").arg("--always").output();
    let version = match version.as_ref() {
        Ok(version) if version.status.success() => {
            rerun_if_changed(&workspace_root.join(".git/HEAD"));
            rerun_if_changed(&workspace_root.join(".git/refs"));
            let git_version = std::str::from_utf8(&version.stdout)
                .unwrap()
                .trim_end()
                .to_string();
            git_version
        }
        _ => {
            rerun_if_changed(&workspace_root.join("Cargo.toml"));
            format!("v{toml_version} (git rev. unknown)")
        }
    };
    // The version shown in the app is iDared 32bit's own, set only in the iOS
    // app's Config.xcconfig, which the Xcode project also takes it from.
    let xcconfig_path = workspace_root.join("iphone/Config.xcconfig");
    rerun_if_changed(&xcconfig_path);
    let xcconfig = std::fs::read_to_string(&xcconfig_path).unwrap();
    let display_version = xcconfig
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == "MARKETING_VERSION").then(|| value.trim().to_string())
        })
        .expect("MARKETING_VERSION must be set in iphone/Config.xcconfig");
    std::fs::write(out_dir.join("display_version.txt"), display_version).unwrap();

    std::fs::write(out_dir.join("version.txt"), version).unwrap();
}
