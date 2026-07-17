//! Embeds build provenance (git commit + commit timestamp) into the `euclid`
//! crate at compile time. Every contract depends on `euclid`, so the values
//! flow into every wasm artifact via `euclid::build_info`.
//!
//! Resolution precedence:
//!   1. `BUILD_COMMIT` / `BUILD_TIME` env vars, if set. The optimized CosmWasm
//!      build runs in Docker mounting only `contracts/cosmwasm`, where `.git`
//!      is NOT visible; `build.sh` captures the values on the host and passes
//!      them in via `--env`.
//!   2. Otherwise shell out to git (the local `cargo build`/`cargo test` path,
//!      where the repo `.git` is reachable).
//!   3. Otherwise `"unknown"`.

use std::process::Command;

fn run_git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let t = s.trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some(t)
    }
}

fn git_is_dirty() -> bool {
    // Tracked changes only (staged or unstaged); untracked files don't count,
    // matching scripts/build-info.sh.
    let unstaged = Command::new("git")
        .args(["diff", "--quiet"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false);
    let staged = Command::new("git")
        .args(["diff", "--cached", "--quiet"])
        .status()
        .map(|s| !s.success())
        .unwrap_or(false);
    unstaged || staged
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

fn main() {
    let commit = env_nonempty("BUILD_COMMIT")
        .or_else(|| {
            let sha = run_git(&["rev-parse", "HEAD"])?;
            Some(if git_is_dirty() {
                format!("{sha} (dirty)")
            } else {
                sha
            })
        })
        .unwrap_or_else(|| "unknown".to_string());

    let build_time = env_nonempty("BUILD_TIME")
        .or_else(|| run_git(&["show", "-s", "--format=%cI", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=EUCLID_BUILD_COMMIT={commit}");
    println!("cargo:rustc-env=EUCLID_BUILD_TIME={build_time}");

    // Re-embed when the injected env changes (the Docker path) or HEAD moves.
    println!("cargo:rerun-if-env-changed=BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=BUILD_TIME");
    // `HEAD` alone only catches branch switches: on a branch it holds the text
    // `ref: refs/heads/<name>`, which does not change when you commit, amend, or rebase.
    // Also watch the ref it points at, which lives either in a loose file or in
    // `packed-refs`. `git rev-parse --git-path` resolves both to the right place even
    // from a linked worktree, whose refs live in the main `.git`, not its own git dir.
    //
    // Only emit paths that exist: cargo reruns the script unconditionally when a
    // declared `rerun-if-changed` path is missing.
    let mut watch = vec!["HEAD".to_string()];
    if let Some(head_ref) = run_git(&["symbolic-ref", "-q", "HEAD"]) {
        watch.push(head_ref);
    }
    watch.push("packed-refs".to_string());
    for rel in &watch {
        let Some(path) = run_git(&["rev-parse", "--git-path", rel]) else {
            continue;
        };
        let Ok(abs) = std::fs::canonicalize(&path) else {
            continue;
        };
        println!("cargo:rerun-if-changed={}", abs.display());
    }
}
