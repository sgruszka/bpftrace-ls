use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build/stdlib.md");
    println!("cargo:rerun-if-changed=build/gen_completion_stdlib.py");

    let status = Command::new("build/gen_completion_stdlib.py")
        .arg("build/stdlib.md")
        .arg("src/gen/completion_stdlib.rs")
        .status()
        .expect("Failed to run Python script");

    assert!(status.success(), "Code generation failed");
}
