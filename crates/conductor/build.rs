use std::process::Command;

fn main() {
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "release-v2.0.0".to_string());

    println!("cargo:rustc-env=GIT_COMMIT_HASH={}", commit);
    
    // Add cargo rerun-if-changed directives to satisfy BUG-P2
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads/");
    println!("cargo:rerun-if-changed=build.rs");
}
