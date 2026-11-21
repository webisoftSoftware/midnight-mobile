// This file is part of MIDNIGHT-ZK.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Build of the Metal shaders.
//!
//! On macOS and iOS every `shaders/*.metal` file is compiled with `xcrun metal`
//! for the SDK of the target (macOS, iOS device or iOS simulator) and linked
//! into one embedded `.metallib` (cfg `midnight_metal_metallib`). The
//! shader source, all files as one translation unit with each `#include`
//! expanded once, is always embedded too, and is compiled at first use when
//! the toolchain is missing, the compile failed, or the system refuses the
//! metallib. Since a kernel file is its own translation unit on one path and
//! shares one on the other, each file includes the headers it needs, prefixes
//! its kernels with its file stem (`ntt_*`, `msm_*`, `quotient_*`) and keeps
//! its helpers in a namespace of that name.
//!
//! [`TEST_SHADERS`] are compiled only with the feature `test-support`.
//!
//! Environment:
//! - `MIDNIGHT_METAL_SHADER_SOURCE=1`: skip the toolchain and embed the source.
//! - `MIDNIGHT_METAL_REQUIRE_METALLIB=1`: a missing toolchain or a failed
//!   compile is a build error.
use std::{
    collections::HashSet,
    env,
    path::{Path, PathBuf},
    process::Command,
};

/// The Metal Shading Language version of every shader (`-std=metal<V>`, and
/// `MTLCompileOptions.languageVersion` on the source path).
const MSL_VERSION: &str = "3.2";
/// The oldest macOS that runs MSL [`MSL_VERSION`].
const MACOS_MIN: &str = "15.0";
/// The oldest iOS that runs MSL [`MSL_VERSION`].
const IOS_MIN: &str = "18.0";

/// The Apple SDK the shaders are compiled for.
#[derive(Clone, Copy)]
struct Sdk {
    /// The `xcrun -sdk` name.
    name: &'static str,
    /// The minimum-version flag of the Metal compiler.
    min_flag: &'static str,
    /// The minimum OS version.
    min: &'static str,
}

impl Sdk {
    /// The SDK of the build target; `None`: the target has no Metal backend.
    fn of_target() -> Option<Sdk> {
        let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
        let abi = env::var("CARGO_CFG_TARGET_ABI").unwrap_or_default();
        let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
        match os.as_str() {
            "macos" => Some(Sdk {
                name: "macosx",
                min_flag: "-mmacosx-version-min",
                min: MACOS_MIN,
            }),
            // An x86_64 iOS target is always a simulator.
            "ios" if abi == "sim" || arch == "x86_64" => Some(Sdk {
                name: "iphonesimulator",
                min_flag: "-mios-simulator-version-min",
                min: IOS_MIN,
            }),
            "ios" if abi.is_empty() => Some(Sdk {
                name: "iphoneos",
                min_flag: "-mios-version-min",
                min: IOS_MIN,
            }),
            // Mac Catalyst and other Apple targets: no backend.
            _ => None,
        }
    }
}
/// Shader files of test and diagnostic kernels, compiled only with the
/// feature `test-support`.
const TEST_SHADERS: [&str; 3] = ["diag.metal", "field_test.metal", "msm_test.metal"];
/// Shader files of the quotient kernels, compiled only with the feature
/// `quotient`: the prover versions this copy serves have no quotient hook.
const QUOTIENT_SHADERS: [&str; 1] = ["quotient.metal"];

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let shaders = manifest.join("shaders");
    println!("cargo:rerun-if-changed=shaders");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rustc-check-cfg=cfg(midnight_metal_metallib)");
    for v in [
        "MIDNIGHT_METAL_SHADER_SOURCE",
        "MIDNIGHT_METAL_REQUIRE_METALLIB",
        "DEVELOPER_DIR",
    ] {
        println!("cargo:rerun-if-env-changed={v}");
    }
    for v in ["CARGO_CFG_TARGET_OS", "CARGO_CFG_TARGET_ABI", "CARGO_CFG_TARGET_ARCH"] {
        println!("cargo:rerun-if-env-changed={v}");
    }
    println!("cargo:rustc-check-cfg=cfg(midnight_metal_device)");
    let Some(sdk) = Sdk::of_target() else {
        return;
    };
    println!("cargo:rustc-cfg=midnight_metal_device");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    println!("cargo:rustc-env=MIDNIGHT_METAL_MSL_VERSION={MSL_VERSION}");

    let test_support = env::var_os("CARGO_FEATURE_TEST_SUPPORT").is_some();
    let quotient = env::var_os("CARGO_FEATURE_QUOTIENT").is_some();
    let named = |p: &PathBuf, names: &[&str]| p.file_name().is_some_and(|n| names.iter().any(|t| n == *t));
    let mut files: Vec<PathBuf> = std::fs::read_dir(&shaders)
        .expect("metal/shaders exists")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "metal"))
        .filter(|p| test_support || !named(p, &TEST_SHADERS))
        .filter(|p| quotient || !named(p, &QUOTIENT_SHADERS))
        .collect();
    files.sort();
    let include = shaders.join("include");

    // The source path: one translation unit.
    let mut source = String::new();
    let mut seen = HashSet::new();
    for f in &files {
        expand(f, &include, &mut seen, &mut source);
    }
    std::fs::write(out.join("midnight_metal.metal"), &source).unwrap();

    let require = env::var("MIDNIGHT_METAL_REQUIRE_METALLIB").as_deref() == Ok("1");
    let fallback = |why: String| {
        if require {
            panic!("midnight-metal: {why} (MIDNIGHT_METAL_REQUIRE_METALLIB=1)");
        }
        println!(
            "cargo:warning=midnight-metal: {why}; embedding the shader source, compiled with \
             newLibraryWithSource at first use"
        );
    };
    if env::var("MIDNIGHT_METAL_SHADER_SOURCE").as_deref() == Ok("1") {
        return fallback("MIDNIGHT_METAL_SHADER_SOURCE=1".into());
    }
    match metallib(sdk, &files, &include, &out) {
        Ok(()) => {
            println!("cargo:rustc-cfg=midnight_metal_metallib");
            println!(
                "cargo:warning=midnight-metal: compiled {} shader file(s) to a metallib with \
                 xcrun metal (MSL {MSL_VERSION}, SDK {} {}+)",
                files.len(),
                sdk.name,
                sdk.min
            );
        }
        Err(why) => fallback(why),
    }
}

/// Compiles each file to AIR and links them into `midnight_metal.metallib`.
fn metallib(sdk: Sdk, files: &[PathBuf], include: &Path, out: &Path) -> Result<(), String> {
    let probe = Command::new("xcrun").args(["-sdk", sdk.name, "-f", "metal"]).output();
    if !probe.as_ref().is_ok_and(|o| o.status.success()) {
        return Err(format!(
            "the Metal toolchain was not found (`xcrun -sdk {} -f metal` failed)",
            sdk.name
        ));
    }
    let mut airs = Vec::new();
    for f in files {
        let air = out.join(f.with_extension("air").file_name().unwrap());
        // Compiled from the shader directory with relative names, so no
        // absolute path of the build machine is recorded in the library.
        let o = Command::new("xcrun")
            .current_dir(include.parent().unwrap_or(include))
            .args(["-sdk", sdk.name, "metal"])
            .arg(format!("-std=metal{MSL_VERSION}"))
            .arg(format!("{}={}", sdk.min_flag, sdk.min))
            .args(["-I", "include", "-c"])
            .arg(f.file_name().unwrap())
            .arg("-o")
            .arg(&air)
            .output()
            .map_err(|e| format!("xcrun metal could not run: {e}"))?;
        if !o.status.success() {
            return Err(format!(
                "xcrun metal failed on {}: {}",
                f.display(),
                String::from_utf8_lossy(&o.stderr).replace('\n', " | ")
            ));
        }
        airs.push(air);
    }
    let lib = out.join("midnight_metal.metallib");
    let o = Command::new("xcrun")
        .args(["-sdk", sdk.name, "metallib"])
        .args(&airs)
        .arg("-o")
        .arg(&lib)
        .output()
        .map_err(|e| format!("xcrun metallib could not run: {e}"))?;
    if !o.status.success() {
        return Err(format!(
            "xcrun metallib failed: {}",
            String::from_utf8_lossy(&o.stderr).replace('\n', " | ")
        ));
    }
    Ok(())
}

/// The name of a shader file in `#line` directives: `include/<name>` for a
/// header, else the file name.
fn shown_name(file: &Path, include: &Path) -> String {
    let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let include = include.canonicalize().unwrap_or_else(|_| include.to_path_buf());
    if file.parent() == Some(include.as_path()) { format!("include/{name}") } else { name }
}

/// Appends `file` to `out`, replacing each `#include "name"` with the header
/// the first time it is met and dropping it after that. `#line` directives
/// keep compiler messages pointing at the original files.
fn expand(file: &Path, include: &Path, seen: &mut HashSet<PathBuf>, out: &mut String) {
    let file = file.canonicalize().unwrap_or_else(|_| file.to_path_buf());
    if !seen.insert(file.clone()) {
        return;
    }
    let text = std::fs::read_to_string(&file)
        .unwrap_or_else(|e| panic!("midnight-metal: cannot read {}: {e}", file.display()));
    // The name in `#line` is relative to the shader directory, so the embedded
    // source does not depend on where the crate was built.
    let shown = shown_name(&file, include);
    out.push_str(&format!("#line 1 \"{shown}\"\n"));
    for (n, line) in text.lines().enumerate() {
        let quoted = line
            .trim_start()
            .strip_prefix("#include")
            .map(str::trim)
            .and_then(|r| r.strip_prefix('"'))
            .and_then(|r| r.split('"').next());
        match quoted {
            Some(name) => {
                let local = file.parent().unwrap().join(name);
                let header = if local.is_file() {
                    local
                } else {
                    include.join(name)
                };
                if !header.is_file() {
                    panic!(
                        "midnight-metal: {} includes missing \"{name}\"",
                        file.display()
                    );
                }
                expand(&header, include, seen, out);
                out.push_str(&format!("#line {} \"{shown}\"\n", n + 2));
            }
            None => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
}
