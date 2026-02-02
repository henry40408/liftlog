use std::process::Command;

fn main() {
    // Re-run build script when git state changes
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");

    let git_version = get_git_version();
    println!("cargo:rustc-env=GIT_VERSION={}", git_version);
}

fn get_git_version() -> String {
    // Tier 1: Use environment variable (for Docker/CI builds)
    if let Ok(version) = std::env::var("GIT_VERSION") {
        if !version.is_empty() && version != "dev" {
            return version;
        }
    }

    // Tier 2: Use git describe
    Command::new("git")
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "dev".to_string()) // Tier 3: fallback to "dev"
}
