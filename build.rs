use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build/stdlib.md");
    println!("cargo:rerun-if-changed=build/gen_completion_stdlib.py");

    println!("cargo:rerun-if-changed=build/language.md");
    println!("cargo:rerun-if-changed=build/gen_completion_probes.py");

    let status = Command::new("build/gen_completion_stdlib.py")
        .arg("build/stdlib.md")
        .arg("src/gen/completion_stdlib.rs")
        .status()
        .expect("Failed to run Python script");

    assert!(status.success(), "Code generation failed");

    let status = Command::new("build/gen_completion_probes.py")
        .arg("build/language.md")
        .arg("src/gen/completion_probes.rs")
        .status()
        .expect("Failed to run Python script");

    assert!(status.success(), "Code generation failed");

    // TODO: how to detect when gen files were removed,
    //       but not force to rebuild every time?
    //
    // println!("cargo:rerun-if-changed=src/gen/completion_stdlib.rs");
    // println!("cargo:rerun-if-changed=src/gen/completion_probes.rs");
}
