use std::sync::Arc;
use std::time::SystemTime;

use anyhow::{Context as _, Result, anyhow};
use futures::AsyncReadExt as _;
use http_client::{AsyncBody, HttpClient, Request};
use serde::Deserialize;

use crate::state::{PrState, PullRequestInfo};

const USER_AGENT: &str = "zed-fork-pr-indicator";
const MAX_BODY_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Deserialize)]
struct ApiPullRequest {
    number: u64,
    title: String,
    html_url: String,
    state: String,
    draft: Option<bool>,
    merged_at: Option<String>,
    updated_at: String,
    auto_merge: Option<serde_json::Value>,
}

/// Fetches the most-recently-updated PR for `{owner}:{branch}` against
/// `{owner}/{repo}`. Returns a single `PrState` describing what was found
/// (an open PR, only a closed/merged PR, or no PR at all).
pub async fn fetch_branch_pr(
    http_client: Arc<dyn HttpClient>,
    token: Option<&str>,
    owner: &str,
    repo: &str,
    branch: &str,
) -> Result<PrState> {
    if let Some(pr) = fetch_with_state(http_client.clone(), token, owner, repo, branch, "open").await? {
        return Ok(PrState::Found(pr));
    }
    if let Some(pr) = fetch_with_state(http_client, token, owner, repo, branch, "closed").await? {
        return Ok(PrState::Found(pr));
    }
    Ok(PrState::None)
}

async fn fetch_with_state(
    http_client: Arc<dyn HttpClient>,
    token: Option<&str>,
    owner: &str,
    repo: &str,
    branch: &str,
    state: &str,
) -> Result<Option<PullRequestInfo>> {
    let url = format!(
        "https://api.github.com/repos/{owner}/{repo}/pulls?head={owner}:{branch}&state={state}&sort=updated&direction=desc&per_page=5",
        owner = owner,
        repo = repo,
        branch = urlencode(branch),
        state = state,
    );

    let mut builder = Request::get(&url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", USER_AGENT);
    if let Some(token) = token {
        builder = builder.header("Authorization", format!("Bearer {token}"));
    }
    let request = builder.body(AsyncBody::default())?;

    let mut response = http_client
        .send(request)
        .await
        .with_context(|| format!("failed to query GitHub PRs for {owner}/{repo} branch {branch}"))?;

    let status = response.status();
    if !status.is_success() {
        let mut body = Vec::new();
        response
            .body_mut()
            .take(MAX_BODY_BYTES)
            .read_to_end(&mut body)
            .await
            .ok();
        return Err(anyhow!(
            "GitHub returned {} querying PRs for {}/{} branch {}: {}",
            status.as_u16(),
            owner,
            repo,
            branch,
            String::from_utf8_lossy(&body)
        ));
    }

    let mut body = Vec::new();
    response
        .body_mut()
        .take(MAX_BODY_BYTES)
        .read_to_end(&mut body)
        .await
        .context("failed to read GitHub response body")?;

    let prs: Vec<ApiPullRequest> = serde_json::from_slice(&body)
        .context("failed to parse GitHub PRs response")?;

    let mut prs = prs;
    prs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let Some(pr) = prs.into_iter().next() else {
        return Ok(None);
    };

    let state = classify_state(&pr);
    Ok(Some(PullRequestInfo {
        number: pr.number,
        title: pr.title,
        html_url: pr.html_url,
        state,
        fetched_at: SystemTime::now(),
    }))
}

fn classify_state(pr: &ApiPullRequest) -> &'static str {
    if pr.merged_at.is_some() {
        "merged"
    } else if pr.state == "closed" {
        "closed"
    } else if pr.draft == Some(true) {
        "draft"
    } else if pr.auto_merge.is_some() {
        "merge_queue"
    } else {
        "open"
    }
}

fn urlencode(input: &str) -> String {
    // Branch names allow `/`, which GitHub accepts as `%2F` in query params.
    // Conservatively percent-encode everything that isn't unreserved.
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}
