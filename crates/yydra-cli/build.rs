// SPDX-License-Identifier: MIT OR Apache-2.0

fn main() {
    println!("cargo:rerun-if-env-changed=TARGET");
    println!(
        "cargo:rustc-env=YYDRA_BUILD_TARGET={}",
        std::env::var("TARGET").expect("Cargo must provide TARGET to the build script")
    );
}
