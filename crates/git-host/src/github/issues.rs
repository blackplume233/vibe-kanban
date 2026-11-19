use std::{fmt, str::FromStr};

use reqwest::Method;
use serde::{Deserialize, Serialize};
use url::Url;

use super::{
    GhCli,
    http::{GitHubApi, api_url},
};
use crate::GitHostError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitHubRepository {
    owner: String,
    name: String,
}

impl FromStr for GitHubRepository {
    type Err = GitHostError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let valid_name = |part: &str, extra: &[u8]| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || extra.contains(&byte))
        };
        match value.split_once('/') {
            Some((owner, name))
                if valid_name(owner, b"-")
                    && valid_name(name, b"-_.")
                    && !matches!(name, "." | "..") =>
            {
                Ok(Self {
                    owner: owner.to_ascii_lowercase(),
                    name: name.to_ascii_lowercase(),
                })
            }
            _ => Err(GitHostError::Repository(
                "Expected a GitHub repository in owner/name form".into(),
            )),
        }
    }
}

impl fmt::Display for GitHubRepository {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.owner, self.name)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueState {
    Open,
    Closed,
}

#[derive(Debug, Deserialize)]
pub struct GitHubIssue {
    pub id: i64,
    pub number: u32,
    pub html_url: String,
    pub title: String,
    pub body: Option<String>,
    pub state: IssueState,
    pull_request: Option<serde::de::IgnoredAny>,
}

#[derive(Debug, Default, Serialize)]
pub struct GitHubIssuePatch<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<&'a str>,
    /// An empty string clears the GitHub body.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<IssueState>,
}

impl GitHubIssuePatch<'_> {
    pub fn is_empty(&self) -> bool {
        self.title.is_none() && self.body.is_none() && self.state.is_none()
    }
}

pub struct GitHubIssueClient {
    api: Option<GitHubApi>,
    #[cfg(test)]
    fixture_base: Option<Url>,
}

impl GitHubIssueClient {
    pub fn new() -> Result<Self, GitHostError> {
        Ok(Self {
            api: GitHubApi::from_env()?,
            #[cfg(test)]
            fixture_base: None,
        })
    }

    pub async fn get(
        &self,
        repository: &GitHubRepository,
        number: u32,
    ) -> Result<GitHubIssue, GitHostError> {
        self.request(repository, number, None).await
    }

    pub async fn update(
        &self,
        repository: &GitHubRepository,
        number: u32,
        patch: &GitHubIssuePatch<'_>,
    ) -> Result<GitHubIssue, GitHostError> {
        self.request(repository, number, Some(patch)).await
    }

    async fn request(
        &self,
        repository: &GitHubRepository,
        number: u32,
        patch: Option<&GitHubIssuePatch<'_>>,
    ) -> Result<GitHubIssue, GitHostError> {
        let method = if patch.is_some() {
            Method::PATCH
        } else {
            Method::GET
        };
        let path = [
            "repos",
            &repository.owner,
            &repository.name,
            "issues",
            &number.to_string(),
        ];
        let issue: GitHubIssue = if let Some(api) = &self.api {
            let mut request = api.request(method, self.url(&path))?;
            if let Some(patch) = patch {
                request = request.json(patch);
            }
            api.send(request).await?
        } else {
            let endpoint = path.join("/");
            let body = patch.map(serde_json::to_vec).transpose().map_err(|_| {
                GitHostError::UnexpectedOutput("Could not encode the GitHub issue update".into())
            })?;
            let response = tokio::task::spawn_blocking(move || {
                GhCli::new().api_json(method.as_str(), &endpoint, body.as_deref())
            })
            .await
            .map_err(|_| {
                GitHostError::PullRequest("GitHub CLI request did not complete".into())
            })??;
            serde_json::from_str(&response).map_err(|_| {
                GitHostError::UnexpectedOutput("Invalid GitHub issue response".into())
            })?
        };
        if issue.pull_request.is_some()
            || issue.number != number
            || issue.id <= 0
            || issue.title.trim().is_empty()
        {
            return Err(GitHostError::UnexpectedOutput(
                "GitHub did not return the requested issue".into(),
            ));
        }
        Ok(issue)
    }

    fn url(&self, segments: &[&str]) -> Url {
        #[cfg(test)]
        if let Some(mut base) = self.fixture_base.clone() {
            base.path_segments_mut()
                .expect("fixture HTTP URL")
                .pop_if_empty()
                .extend(segments);
            return base;
        }
        api_url(segments)
    }
}

#[cfg(test)]
mod tests {
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    use super::*;

    #[test]
    fn repository_input_is_canonical_and_constrained() {
        assert_eq!(
            "Acme/ISSUES.tools"
                .parse::<GitHubRepository>()
                .unwrap()
                .to_string(),
            "acme/issues.tools"
        );
        for invalid in [
            "https://github.com/acme/repo",
            "acme/repo?query",
            "acme/..",
            "acme/repo/extra",
        ] {
            assert!(invalid.parse::<GitHubRepository>().is_err());
        }
    }

    #[tokio::test]
    async fn fetches_and_patches_issue_against_local_fixture_server() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let fixture_base =
            Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let task = tokio::spawn(async move {
            let first = include_str!("fixtures/rest-issue.json");
            let second = include_str!("fixtures/rest-issue-closed.json");
            for (method, fixture) in [("GET", first), ("PATCH", second)] {
                let (mut conn, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let (header_end, body_len) = loop {
                    assert!(conn.read_buf(&mut bytes).await.unwrap() > 0);
                    if let Some(header_end) = bytes.windows(4).position(|part| part == b"\r\n\r\n")
                    {
                        let headers = std::str::from_utf8(&bytes[..header_end]).unwrap();
                        assert!(
                            headers.starts_with(&format!(
                                "{method} /repos/acme/repo/issues/42 HTTP/1.1"
                            ))
                        );
                        let body_len = headers
                            .lines()
                            .find_map(|line| line.strip_prefix("content-length: "))
                            .map_or(0, |length| length.parse().unwrap());
                        break (header_end + 4, body_len);
                    }
                };
                while bytes.len() - header_end < body_len {
                    assert!(conn.read_buf(&mut bytes).await.unwrap() > 0);
                }
                if method == "PATCH" {
                    let payload: serde_json::Value =
                        serde_json::from_slice(&bytes[header_end..]).unwrap();
                    assert_eq!(payload, serde_json::json!({"state": "closed", "body": ""}));
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{fixture}",
                    fixture.len()
                );
                conn.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let client = GitHubIssueClient {
            api: Some(GitHubApi::fixture()),
            fixture_base: Some(fixture_base),
        };
        let repository = "acme/repo".parse().unwrap();
        let issue = client.get(&repository, 42).await.unwrap();
        assert_eq!(issue.state, IssueState::Open);
        assert_eq!(
            issue.body.as_deref(),
            Some("Reproduce this condition locally.")
        );
        let patch = GitHubIssuePatch {
            body: Some(""),
            state: Some(IssueState::Closed),
            ..Default::default()
        };
        let updated = client.update(&repository, 42, &patch).await.unwrap();
        assert_eq!(updated.state, IssueState::Closed);
        assert!(updated.body.is_none());
        task.await.unwrap();
    }
}
