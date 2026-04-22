use std::env;
use std::path::Path;
use std::process::Command;

fn resolve_git_head_path() -> Option<String> {
    let git_path = Path::new(".git");
    if git_path.is_file() {
        // Worktree: .git is a pointer file containing "gitdir: /path/to/real/.git/worktrees/<name>"
        if let Ok(content) = std::fs::read_to_string(git_path) {
            if let Some(gitdir) = content.strip_prefix("gitdir:") {
                let gitdir = gitdir.trim();
                return Some(format!("{}/HEAD", gitdir));
            }
        }
    } else if git_path.is_dir() {
        // Regular repo: .git is a directory
        return Some(".git/HEAD".to_string());
    }
    None
}

fn main() {
    // Get git SHA (short hash)
    let git_sha = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .map_or_else(|| "unknown".to_string(), |s| s.trim().to_string());

    println!("cargo:rustc-env=GIT_SHA={git_sha}");

    // TARGET is always set by Cargo during build
    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=TARGET={target}");

    // Build date from SOURCE_DATE_EPOCH (reproducible builds) or current UTC date.
    // Intentionally ignoring time component to keep output deterministic within a day.
    let build_date = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|epoch| epoch.parse::<i64>().ok())
        .map(|_ts| {
            // Use SOURCE_DATE_EPOCH to derive date via chrono if available;
            // for simplicity we just use the env var as a signal and fall back
            // to build-time env. In practice CI sets this via workflow.
            std::env::var("BUILD_DATE").unwrap_or_else(|_| "unknown".to_string())
        })
        .or_else(|| std::env::var("BUILD_DATE").ok())
        .unwrap_or_else(|| {
            // Fall back to current date via `date` command
            Command::new("date")
                .args(["+%Y-%m-%d"])
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        String::from_utf8(o.stdout).ok()
                    } else {
                        None
                    }
                })
                .map_or_else(|| "unknown".to_string(), |s| s.trim().to_string())
        });
    println!("cargo:rustc-env=BUILD_DATE={build_date}");

    // Rerun if git state changes
    // In worktrees, .git is a pointer file, so watch the actual HEAD location
    if let Some(head_path) = resolve_git_head_path() {
        println!("cargo:rerun-if-changed={}", head_path);
    } else {
        // Fallback to .git/HEAD for regular repos (won't trigger in worktrees, but prevents silent failure)
        println!("cargo:rerun-if-changed=.git/HEAD");
    }
    println!("cargo:rerun-if-changed=.git/refs");
}
