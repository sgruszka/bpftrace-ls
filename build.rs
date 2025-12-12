include!("build/gen.rs");

fn main() {
    println!("cargo:rerun-if-changed=build/stdlib.md");
    println!("cargo:rerun-if-changed=build/language.md");

    gen_completion_probes();
    gen_completion_stdlib();

    // TODO: how to detect when gen files were removed,
    //       but not force to rebuild every time?
    //
    // println!("cargo:rerun-if-changed=src/gen/completion_stdlib.rs");
    // println!("cargo:rerun-if-changed=src/gen/completion_probes.rs");
}
