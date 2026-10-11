//! `include_dir` embeds `data/` at compile time but cannot tell cargo to
//! rebuild when a data file changes, so this script does.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../data");
}
