// SPDX-License-Identifier: MIT OR Apache-2.0

fn main() {
    // SQLx embeds existing files; also invalidate when migrations are added or removed.
    println!("cargo::rerun-if-changed=../../migrations");
}
