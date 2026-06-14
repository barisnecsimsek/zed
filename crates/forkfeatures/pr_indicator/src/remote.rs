/// Parses a git remote URL and returns `Some((owner, repo))` when it points
/// at github.com. Returns `None` for any non-github host (GitHub Enterprise,
/// GitLab, Bitbucket, file paths, etc.) — those are out of scope for v1.
///
/// Handles the three common URL shapes:
/// - `https://github.com/owner/repo.git`
/// - `git@github.com:owner/repo.git`
/// - `ssh://git@github.com/owner/repo.git`
pub fn parse_github_remote(url: &str) -> Option<(String, String)> {
    let url = url.trim();
    if url.is_empty() {
        return None;
    }

    let (host, path) = if let Some(rest) = url.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        (host, path)
    } else if let Some(rest) = url.strip_prefix("ssh://") {
        let rest = rest.strip_prefix("git@").unwrap_or(rest);
        let (host, path) = rest.split_once('/')?;
        (host, path)
    } else if let Some(rest) = url.strip_prefix("https://") {
        let rest = rest.split_once('@').map(|(_, r)| r).unwrap_or(rest);
        let (host, path) = rest.split_once('/')?;
        (host, path)
    } else if let Some(rest) = url.strip_prefix("http://") {
        let (host, path) = rest.split_once('/')?;
        (host, path)
    } else {
        return None;
    };

    if !host.eq_ignore_ascii_case("github.com") {
        return None;
    }

    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, repo) = path.split_once('/')?;
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_https() {
        assert_eq!(
            parse_github_remote("https://github.com/owner/repo.git"),
            Some(("owner".into(), "repo".into()))
        );
        assert_eq!(
            parse_github_remote("https://github.com/owner/repo"),
            Some(("owner".into(), "repo".into()))
        );
    }

    #[test]
    fn parses_ssh_short() {
        assert_eq!(
            parse_github_remote("git@github.com:owner/repo.git"),
            Some(("owner".into(), "repo".into()))
        );
    }

    #[test]
    fn parses_ssh_full() {
        assert_eq!(
            parse_github_remote("ssh://git@github.com/owner/repo.git"),
            Some(("owner".into(), "repo".into()))
        );
    }

    #[test]
    fn rejects_non_github_hosts() {
        assert_eq!(parse_github_remote("https://gitlab.com/owner/repo"), None);
        assert_eq!(parse_github_remote("git@bitbucket.org:owner/repo.git"), None);
        assert_eq!(parse_github_remote("https://ghe.example.com/owner/repo"), None);
    }

    #[test]
    fn rejects_malformed() {
        assert_eq!(parse_github_remote(""), None);
        assert_eq!(parse_github_remote("github.com/owner/repo"), None);
        assert_eq!(parse_github_remote("https://github.com/owner"), None);
    }
}
