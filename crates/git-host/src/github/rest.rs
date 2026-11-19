use db::models::merge::MergeStatus;
use reqwest::Method;
use serde::Deserialize;

use super::{
    cli::{GhCli, GitHubRepoInfo},
    http::{GitHubApi, api_url},
};
use crate::{CreatePrRequest, GitHostError, PullRequestDetail, UnifiedPrComment};

pub(super) fn github_path(url: &str) -> Option<(String, Vec<String>)> {
    let parsed = url::Url::parse(url)
        .ok()
        .filter(|url| url.has_host())
        .or_else(|| {
            let (_, rest) = url.split_once('@')?;
            let (host, path) = rest.split_once(':')?;
            url::Url::parse(&format!("ssh://{host}/{path}")).ok()
        })?;
    if !matches!(parsed.scheme(), "https" | "http" | "ssh" | "git") {
        return None;
    }
    let host = parsed.host_str()?.to_string();
    let parts = parsed
        .path()
        .trim_matches('/')
        .split('/')
        .map(str::to_string)
        .collect();
    Some((host, parts))
}

pub(super) fn public_repo(url: &str) -> Option<GitHubRepoInfo> {
    let (host, parts) = github_path(url)?;
    let [owner, repo] = parts.as_slice() else {
        return None;
    };
    (host.eq_ignore_ascii_case("github.com") && !owner.is_empty() && !repo.is_empty()).then(|| {
        GitHubRepoInfo {
            owner: owner.clone(),
            repo_name: repo.strip_suffix(".git").unwrap_or(repo).to_string(),
            hostname: Some(host),
        }
    })
}

pub(super) fn public_pr(url: &str) -> Option<(GitHubRepoInfo, i64)> {
    let (host, parts) = github_path(url)?;
    let [owner, repo, kind, number] = parts.as_slice() else {
        return None;
    };
    let number = number.parse::<i32>().ok().filter(|n| *n > 0)?;
    (host.eq_ignore_ascii_case("github.com")
        && kind == "pull"
        && !owner.is_empty()
        && !repo.is_empty())
    .then(|| {
        (
            GitHubRepoInfo {
                owner: owner.clone(),
                repo_name: repo.clone(),
                hostname: Some(host),
            },
            i64::from(number),
        )
    })
}

#[derive(Deserialize)]
struct GitRef {
    #[serde(rename = "ref")]
    name: String,
}

#[derive(Deserialize)]
struct RestPr {
    number: i64,
    html_url: String,
    state: String,
    merged_at: Option<chrono::DateTime<chrono::Utc>>,
    merge_commit_sha: Option<String>,
    title: String,
    updated_at: Option<chrono::DateTime<chrono::Utc>>,
    base: GitRef,
    head: GitRef,
}

impl From<RestPr> for PullRequestDetail {
    fn from(pr: RestPr) -> Self {
        let status = match (pr.state.as_str(), pr.merged_at) {
            ("open", _) => MergeStatus::Open,
            ("closed", Some(_)) => MergeStatus::Merged,
            ("closed", None) => MergeStatus::Closed,
            _ => MergeStatus::Unknown,
        };
        Self {
            number: pr.number,
            url: pr.html_url,
            merge_commit_sha: matches!(status, MergeStatus::Merged)
                .then_some(pr.merge_commit_sha)
                .flatten(),
            status,
            merged_at: pr.merged_at,
            title: pr.title,
            base_branch: pr.base.name,
            head_branch: pr.head.name,
            checks: None,
        }
    }
}

pub(super) async fn create_pr(
    api: &GitHubApi,
    repo: &GitHubRepoInfo,
    request: &CreatePrRequest,
) -> Result<PullRequestDetail, GitHostError> {
    let head_repo = request
        .head_repo_url
        .as_deref()
        .map(|url| public_repo(url).ok_or(GitHostError::UnsupportedProvider))
        .transpose()?;
    let head = match &head_repo {
        Some(fork)
            if !fork.owner.eq_ignore_ascii_case(&repo.owner)
                || !fork.repo_name.eq_ignore_ascii_case(&repo.repo_name) =>
        {
            format!("{}:{}", fork.owner, request.head_branch)
        }
        _ => request.head_branch.clone(),
    };
    let mut body = serde_json::json!({"title": request.title, "body": request.body, "head": head, "base": request.base_branch, "draft": request.draft.unwrap_or(false)});
    if let Some(fork) = head_repo
        && fork.owner.eq_ignore_ascii_case(&repo.owner)
        && !fork.repo_name.eq_ignore_ascii_case(&repo.repo_name)
    {
        body["head_repo"] = fork.repo_name.into();
    }
    let url = api_url(&["repos", &repo.owner, &repo.repo_name, "pulls"]);
    let pr: RestPr = api
        .send(api.request(Method::POST, url)?.json(&body))
        .await?;
    Ok(pr.into())
}

pub(super) async fn list_recent_prs(
    api: &GitHubApi,
    repo: &GitHubRepoInfo,
) -> Result<Vec<PullRequestDetail>, GitHostError> {
    let url = api_url(&["repos", &repo.owner, &repo.repo_name, "pulls"]);
    let mut prs: Vec<RestPr> = api
        .send(api.request(Method::GET, url.clone())?.query(&[
            ("state", "open"),
            ("sort", "updated"),
            ("per_page", "30"),
        ]))
        .await?;
    let closed: Vec<RestPr> = api
        .send(api.request(Method::GET, url)?.query(&[
            ("state", "closed"),
            ("sort", "updated"),
            ("per_page", "20"),
        ]))
        .await?;
    prs.extend(closed);
    prs.sort_by_key(|pr| std::cmp::Reverse(pr.updated_at));
    Ok(prs.into_iter().map(Into::into).collect())
}

#[derive(Deserialize)]
struct Author {
    login: String,
}

#[derive(Deserialize)]
struct RestComment {
    id: i64,
    user: Option<Author>,
    body: String,
    created_at: chrono::DateTime<chrono::Utc>,
    html_url: String,
    author_association: Option<String>,
    path: Option<String>,
    line: Option<i64>,
    side: Option<String>,
    diff_hunk: Option<String>,
}

impl RestComment {
    fn into_unified(self) -> UnifiedPrComment {
        let author = self
            .user
            .map_or_else(|| "unknown".into(), |user| user.login);
        match self.path {
            Some(path) => UnifiedPrComment::Review {
                id: self.id,
                author,
                author_association: self.author_association,
                body: self.body,
                created_at: self.created_at,
                url: Some(self.html_url),
                path,
                line: self.line,
                side: self.side,
                diff_hunk: self.diff_hunk,
            },
            None => UnifiedPrComment::General {
                id: self.id.to_string(),
                author,
                author_association: self.author_association,
                body: self.body,
                created_at: self.created_at,
                url: Some(self.html_url),
            },
        }
    }
}

pub(super) async fn comments(
    api: &GitHubApi,
    repo: &GitHubRepoInfo,
    number: i64,
    review: bool,
) -> Result<Vec<UnifiedPrComment>, GitHostError> {
    let number = number.to_string();
    let url = api_url(&[
        "repos",
        &repo.owner,
        &repo.repo_name,
        if review { "pulls" } else { "issues" },
        &number,
        "comments",
    ]);
    let mut comments = Vec::new();
    for page in 1.. {
        let chunk: Vec<RestComment> = api
            .send(
                api.request(Method::GET, url.clone())?
                    .query(&[("per_page", 100), ("page", page)]),
            )
            .await?;
        let last = chunk.len() < 100;
        comments.extend(chunk.into_iter().map(RestComment::into_unified));
        if last {
            break;
        }
    }
    Ok(comments)
}

pub(super) async fn review_comments(
    api: Option<&GitHubApi>,
    repo: &GitHubRepoInfo,
    number: i64,
) -> Result<Vec<UnifiedPrComment>, GitHostError> {
    if let Some(api) = api {
        return comments(api, repo, number, true).await;
    }
    let repo = repo.clone();
    let pages =
        tokio::task::spawn_blocking(move || GhCli::new().paged_review_comments(&repo, number))
            .await
            .map_err(|_| {
                GitHostError::PullRequest("GitHub CLI request did not complete".into())
            })??;
    let pages: Vec<Vec<RestComment>> = serde_json::from_str(&pages).map_err(|_| {
        GitHostError::UnexpectedOutput("Invalid GitHub review comments response".into())
    })?;
    Ok(pages
        .into_iter()
        .flatten()
        .map(RestComment::into_unified)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_transport_only_accepts_public_github_urls() {
        assert!(public_repo("git@github.com:owner/name.git").is_some());
        assert!(public_pr("https://github.com/owner/name/pull/42").is_some());
        assert!(public_pr("https://github.com.evil.test/owner/name/pull/42").is_none());
        assert!(public_pr("https://github.com/owner/name/pull/42abc").is_none());
    }

    #[test]
    fn parses_rest_review_comment_and_merge_status() {
        let comment: RestComment =
            serde_json::from_str(include_str!("fixtures/rest-review-comment.json")).unwrap();
        assert!(matches!(
            comment.into_unified(),
            UnifiedPrComment::Review {
                id: 1024,
                line: Some(12),
                ..
            }
        ));
        let detail: PullRequestDetail =
            serde_json::from_str::<RestPr>(include_str!("fixtures/rest-pull.json"))
                .unwrap()
                .into();
        assert!(matches!(detail.status, MergeStatus::Merged));
        assert_eq!(detail.merge_commit_sha.as_deref(), Some("merge-sha"));
    }
}
