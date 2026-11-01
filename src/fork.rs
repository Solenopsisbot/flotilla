//! Keeping the Solenopsisbot/horizon fork honest.
//!
//! The fork's `main` is upstream's main plus our in-flight branches plus commits marked
//! `FORK ONLY` (offline bots: Horizon upstream follows Mojang's rules and refuses them). Two
//! rules keep that safe:
//!
//! - **Sync by merging.** `sync-fork` merges `origin/main` (horizon-rust/horizon) into the
//!   local `fork-main` and pushes it to the fork's `main` as a fast-forward. Never
//!   `gh repo sync --force`: it would replace the fork's main with upstream's and drop the
//!   fork-only commits.
//! - **No pull request from main.** Every upstream PR comes from a branch on upstream's main.
//!   `pr-check BRANCH` refuses a branch that carries a `FORK ONLY` commit.

use std::path::Path;
use std::process::Command;

/// The subject that marks a commit that must never go upstream.
pub const FORK_ONLY: &str = "FORK ONLY";

fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    } else {
        Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

/// The subjects of `commits` that are fork-only.
pub fn fork_only<'a>(subjects: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    subjects
        .into_iter()
        .filter(|s| s.starts_with(FORK_ONLY))
        .collect()
}

/// Refuses `branch` if it has a fork-only commit that upstream's main doesn't.
pub fn pr_check(repo: &Path, branch: &str) -> Result<(), String> {
    git(repo, &["fetch", "-q", "origin"])?;
    let log = git(
        repo,
        &["log", "--format=%s", &format!("origin/main..{branch}")],
    )?;
    let bad = fork_only(log.lines());
    if bad.is_empty() {
        println!("{branch}: no fork-only commit, fine for a pull request");
        Ok(())
    } else {
        Err(format!(
            "{branch} carries fork-only commits; open pull requests from a branch on origin/main:\n  {}",
            bad.join("\n  ")
        ))
    }
}

/// Merges upstream's main into `fork-main` (checked out in `repo`) and pushes it to the fork.
pub fn sync(repo: &Path) -> Result<(), String> {
    let branch = git(repo, &["branch", "--show-current"])?;
    if branch != "fork-main" {
        return Err(format!(
            "{} has {branch} checked out; sync-fork works on fork-main (a worktree of it)",
            repo.display()
        ));
    }
    if !git(repo, &["status", "--porcelain", "--untracked-files=no"])?.is_empty() {
        return Err(format!("{} has uncommitted changes", repo.display()));
    }
    git(repo, &["fetch", "-q", "origin"])?;
    git(repo, &["fetch", "-q", "fork"])?;
    // The fork's main must be ours already (no one pushed elsewhere): a fast-forward only.
    git(repo, &["merge-base", "--is-ancestor", "fork/main", "HEAD"]).map_err(|_| {
        "fork/main has commits fork-main lacks: merge them in by hand first".to_string()
    })?;
    if let Err(e) = git(repo, &["merge", "--no-edit", "origin/main"]) {
        let _ = git(repo, &["merge", "--abort"]);
        return Err(format!(
            "upstream's main doesn't merge cleanly; nothing pushed:\n{e}"
        ));
    }
    git(repo, &["push", "-q", "fork", "fork-main:main"])?;
    println!(
        "fork main is upstream's main plus our branches: {}",
        git(repo, &["log", "--oneline", "-1"])?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_marked_commits_are_fork_only() {
        let subjects = [
            "spawn: a spawned player needs --session-file",
            "FORK ONLY: spawn plays offline without a session file",
            "Merge remote-tracking branch 'origin/main'",
        ];
        assert_eq!(
            fork_only(subjects),
            ["FORK ONLY: spawn plays offline without a session file"]
        );
        assert!(fork_only(["fork only in lower case is a normal commit"]).is_empty());
    }
}
