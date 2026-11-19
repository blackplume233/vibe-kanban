use std::{
    sync::{LazyLock, Mutex},
    time::{Duration, Instant},
};

use chrono::Utc;
use reqwest::{
    Client, RequestBuilder, StatusCode,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use serde::de::DeserializeOwned;
use url::Url;

use crate::GitHostError;

const DEFAULT_RATE_LIMIT_DELAY: Duration = Duration::from_secs(60);
static CLIENT: LazyLock<Result<Client, reqwest::Error>> = LazyLock::new(|| {
    Client::builder()
        .user_agent(concat!("vibe-kanban/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
});
static RATE_LIMIT_UNTIL: Mutex<Option<Instant>> = Mutex::new(None);

pub(super) struct GitHubApi(HeaderValue);

impl GitHubApi {
    pub(super) fn from_env() -> Result<Option<Self>, GitHostError> {
        for name in ["VK_GITHUB_TOKEN", "GITHUB_TOKEN"] {
            if let Ok(token) = std::env::var(name)
                && !token.trim().is_empty()
            {
                let mut header = HeaderValue::from_str(&format!("Bearer {}", token.trim()))
                    .map_err(|_| {
                        GitHostError::AuthFailed(format!("Invalid {name} header value"))
                    })?;
                header.set_sensitive(true);
                return Ok(Some(Self(header)));
            }
        }
        Ok(None)
    }

    #[cfg(test)]
    pub(super) fn fixture() -> Self {
        Self(HeaderValue::from_static("fixture"))
    }

    pub(super) fn request(
        &self,
        method: reqwest::Method,
        url: Url,
    ) -> Result<RequestBuilder, GitHostError> {
        let client = CLIENT.as_ref().map_err(|_| {
            GitHostError::PullRequest("Failed to initialize GitHub HTTP client".into())
        })?;
        Ok(client
            .request(method, url)
            .header(AUTHORIZATION, self.0.clone())
            .header("accept", "application/vnd.github+json"))
    }

    pub(super) async fn send<T: DeserializeOwned>(
        &self,
        request: RequestBuilder,
    ) -> Result<T, GitHostError> {
        check_rate_limit()?;
        let response = request
            .send()
            .await
            .map_err(|_| GitHostError::PullRequest("GitHub HTTP request failed".into()))?;
        let status = response.status();
        let retry_after = retry_after(response.headers(), Utc::now().timestamp());
        if let Some(delay) = retry_after {
            defer_requests(delay);
        }
        if !status.is_success() {
            if status == StatusCode::TOO_MANY_REQUESTS
                || (status == StatusCode::FORBIDDEN && retry_after.is_some())
            {
                return Err(rate_limited(
                    retry_after.unwrap_or(DEFAULT_RATE_LIMIT_DELAY),
                ));
            }
            return Err(match status {
                StatusCode::UNAUTHORIZED => {
                    GitHostError::AuthFailed("GitHub rejected the configured token".into())
                }
                StatusCode::FORBIDDEN => {
                    GitHostError::InsufficientPermissions("GitHub rejected the request".into())
                }
                StatusCode::NOT_FOUND => {
                    GitHostError::RepoNotFoundOrNoAccess("GitHub resource unavailable".into())
                }
                _ => GitHostError::PullRequest(format!("GitHub returned HTTP {status}")),
            });
        }
        response
            .json()
            .await
            .map_err(|_| GitHostError::UnexpectedOutput("Invalid GitHub API response".into()))
    }
}

pub(super) fn api_url(segments: &[&str]) -> Url {
    let mut url = Url::parse("https://api.github.com/").expect("constant GitHub API URL");
    url.path_segments_mut()
        .expect("HTTP URL")
        .pop_if_empty()
        .extend(segments);
    url
}

pub(super) fn check_rate_limit() -> Result<(), GitHostError> {
    if let Some(retry_after) = RATE_LIMIT_UNTIL
        .lock()
        .unwrap()
        .and_then(|t| t.checked_duration_since(Instant::now()))
    {
        return Err(GitHostError::RateLimited { retry_after });
    }
    Ok(())
}

pub(super) fn defer_requests(delay: Duration) {
    if let Some(until) = Instant::now().checked_add(delay) {
        let mut current = RATE_LIMIT_UNTIL.lock().unwrap();
        *current = Some(current.map_or(until, |previous| previous.max(until)));
    }
}

pub(super) fn rate_limited(delay: Duration) -> GitHostError {
    defer_requests(delay);
    GitHostError::RateLimited { retry_after: delay }
}

pub(super) fn retry_after(headers: &HeaderMap, now: i64) -> Option<Duration> {
    let seconds = |key| headers.get(key)?.to_str().ok()?.parse::<u64>().ok();
    let reset = if seconds("x-ratelimit-remaining") == Some(0) {
        Some(
            seconds("x-ratelimit-reset")
                .and_then(|v| i64::try_from(v).ok())
                .map_or(DEFAULT_RATE_LIMIT_DELAY.as_secs(), |reset| {
                    reset.saturating_sub(now).max(1) as u64
                }),
        )
    } else {
        None
    };
    reset
        .into_iter()
        .chain(seconds("retry-after"))
        .max()
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn honors_later_of_retry_after_and_primary_reset() {
        let mut headers = HeaderMap::new();
        headers.insert("x-ratelimit-remaining", HeaderValue::from_static("0"));
        headers.insert("x-ratelimit-reset", HeaderValue::from_static("1130"));
        headers.insert("retry-after", HeaderValue::from_static("60"));
        assert_eq!(retry_after(&headers, 1000), Some(Duration::from_secs(130)));
        headers.insert("x-ratelimit-remaining", HeaderValue::from_static("1"));
        assert_eq!(retry_after(&headers, 1000), Some(Duration::from_secs(60)));
    }
}
