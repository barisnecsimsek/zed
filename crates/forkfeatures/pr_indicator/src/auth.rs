use smol::process::{Command, Stdio};

/// Resolves a GitHub auth token by trying, in order:
///
/// 1. `gh auth token` (the GitHub CLI's stored token)
/// 2. The `GITHUB_TOKEN` environment variable
///
/// Returns `None` if neither source produces a token. The v1 design is silent
/// — no UI prompt, no toast — so callers that need authenticated calls just
/// degrade gracefully when this returns `None`.
pub async fn resolve_token() -> Option<String> {
    if let Some(token) = read_from_gh_cli().await {
        return Some(token);
    }
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Some(token);
        }
    }
    None
}

async fn read_from_gh_cli() -> Option<String> {
    let output = Command::new("gh")
        .args(["auth", "token"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if token.is_empty() {
        return None;
    }
    Some(token)
}
