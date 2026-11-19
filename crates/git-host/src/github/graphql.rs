use std::{collections::HashMap, fmt::Write, time::Duration};

use chrono::{DateTime, Utc};
use db::models::merge::MergeStatus;
use reqwest::Method;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use super::{
    cli::{GhCli, GitHubRepoInfo},
    http::{GitHubApi, api_url, check_rate_limit, defer_requests, rate_limited},
    rest,
};
use crate::{FailedPrCheck, GitHostError, PrCheckSummary, PullRequestDetail, UnifiedPrComment};

// Keep each query well below GitHub's node and timeout limits, even with reviews enabled.
pub const PR_BATCH_SIZE: usize = 20;

#[derive(Debug)]
pub struct GitHubPrSnapshot {
    pub detail: PullRequestDetail,
    pub review_comments: Vec<UnifiedPrComment>,
}

#[derive(Serialize)]
struct Query {
    query: String,
    variables: HashMap<String, Value>,
}

const DETAIL: &str = "number url state mergedAt mergeCommit { oid } title baseRefName headRefName";
const CONTEXTS: &str = "pageInfo { hasNextPage endCursor } nodes {
    __typename
    ... on CheckRun { name status conclusion detailsUrl }
    ... on StatusContext { context state targetUrl }
}";
const REVIEWS: &str = "reviews(last: 20) {
    pageInfo { hasPreviousPage }
    nodes { state comments(last: 20) {
        pageInfo { hasPreviousPage }
        nodes { fullDatabaseId body createdAt url author { login } authorAssociation path line diffHunk }
    } }
}";
const RATE_LIMIT: &str = "rateLimit { remaining resetAt }";

fn batch_query(prs: &[(GitHubRepoInfo, i64)], reviews: bool) -> Query {
    let mut declaration = String::from("query(");
    let mut body = String::new();
    let mut variables = HashMap::with_capacity(prs.len() * 3);
    for (i, (repo, number)) in prs.iter().enumerate() {
        if i > 0 {
            declaration.push_str(", ");
        }
        write!(
            declaration,
            "$owner{i}: String!, $repo{i}: String!, $num{i}: Int!"
        )
        .unwrap();
        variables.insert(format!("owner{i}"), repo.owner.clone().into());
        variables.insert(format!("repo{i}"), repo.repo_name.clone().into());
        variables.insert(format!("num{i}"), (*number).into());
        write!(body, "p{i}: repository(owner: $owner{i}, name: $repo{i}) {{ pullRequest(number: $num{i}) {{ {DETAIL}
            commits(last: 1) {{ nodes {{ commit {{ id statusCheckRollup {{ contexts(first: 100) {{ {CONTEXTS} }} }} }} }} }}
            {} }} }}\n", if reviews { REVIEWS } else { "" }).unwrap();
    }
    write!(declaration, ") {{ {RATE_LIMIT}\n{body} }}").unwrap();
    Query {
        query: declaration,
        variables,
    }
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: Option<T>,
    #[serde(default)]
    errors: Vec<QueryError>,
}

#[derive(Deserialize)]
struct QueryError {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    path: Vec<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RateLimit {
    remaining: u32,
    reset_at: DateTime<Utc>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchData {
    rate_limit: Option<RateLimit>,
    #[serde(flatten)]
    repositories: HashMap<String, Option<Repository>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Repository {
    pull_request: Option<PullRequest>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullRequest {
    number: i64,
    url: String,
    state: String,
    merged_at: Option<DateTime<Utc>>,
    merge_commit: Option<MergeCommit>,
    title: String,
    base_ref_name: String,
    head_ref_name: String,
    commits: Option<Nodes<PrCommit>>,
    reviews: Option<PreviousPage<Review>>,
}

#[derive(Deserialize)]
struct MergeCommit {
    oid: String,
}

#[derive(Deserialize)]
struct Nodes<T> {
    nodes: Vec<T>,
}

#[derive(Deserialize)]
struct PrCommit {
    commit: Commit,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Commit {
    id: String,
    status_check_rollup: Option<Rollup>,
}

#[derive(Deserialize)]
struct Rollup {
    contexts: ContextPage,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContextPage {
    page_info: NextPageInfo,
    nodes: Vec<CheckContext>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NextPageInfo {
    has_next_page: bool,
    end_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviousPage<T> {
    page_info: PreviousPageInfo,
    nodes: Vec<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviousPageInfo {
    has_previous_page: bool,
}

#[derive(Deserialize)]
struct Review {
    state: String,
    comments: PreviousPage<ReviewComment>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviewComment {
    full_database_id: Option<DatabaseId>,
    body: String,
    created_at: DateTime<Utc>,
    url: String,
    author: Option<Author>,
    author_association: Option<String>,
    path: String,
    line: Option<i64>,
    diff_hunk: Option<String>,
}

#[derive(Deserialize)]
struct Author {
    login: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DatabaseId {
    Number(i64),
    Text(String),
}

impl ReviewComment {
    fn into_unified(self) -> Option<UnifiedPrComment> {
        Some(UnifiedPrComment::Review {
            id: match self.full_database_id? {
                DatabaseId::Number(id) => id,
                DatabaseId::Text(id) => id.parse().ok()?,
            },
            author: self
                .author
                .map_or_else(|| "unknown".into(), |author| author.login),
            author_association: self.author_association,
            body: self.body,
            created_at: self.created_at,
            url: Some(self.url),
            path: self.path,
            line: self.line,
            side: None,
            diff_hunk: self.diff_hunk,
        })
    }
}

#[derive(Deserialize)]
#[serde(tag = "__typename", rename_all_fields = "camelCase")]
pub(super) enum CheckContext {
    CheckRun {
        name: String,
        status: String,
        conclusion: Option<String>,
        details_url: Option<String>,
    },
    StatusContext {
        context: String,
        state: String,
        target_url: Option<String>,
    },
}

impl CheckContext {
    pub(super) fn add_to_summary(self, summary: &mut PrCheckSummary) {
        summary.total += 1;
        let (name, url, state) = match self {
            Self::CheckRun {
                name,
                status,
                conclusion,
                details_url,
            } => {
                let state = if status == "COMPLETED" {
                    conclusion
                } else {
                    None
                };
                (name, details_url, state)
            }
            Self::StatusContext {
                context,
                state,
                target_url,
            } => (context, target_url, Some(state)),
        };
        match state.as_deref() {
            Some("SUCCESS" | "NEUTRAL" | "SKIPPED") => summary.passed += 1,
            None | Some("EXPECTED" | "PENDING") => summary.pending += 1,
            _ => {
                summary.failed += 1;
                summary.failures.push(FailedPrCheck { name, url });
            }
        }
    }
}

impl PullRequest {
    fn into_parts(
        self,
    ) -> (
        PullRequestDetail,
        Option<Commit>,
        Option<PreviousPage<Review>>,
    ) {
        let detail = PullRequestDetail {
            number: self.number,
            url: self.url,
            status: match self.state.as_str() {
                "OPEN" => MergeStatus::Open,
                "CLOSED" => MergeStatus::Closed,
                "MERGED" => MergeStatus::Merged,
                _ => MergeStatus::Unknown,
            },
            merged_at: self.merged_at,
            merge_commit_sha: self.merge_commit.map(|c| c.oid),
            title: self.title,
            base_branch: self.base_ref_name,
            head_branch: self.head_ref_name,
            checks: None,
        };
        (
            detail,
            self.commits
                .and_then(|commits| commits.nodes.into_iter().next())
                .map(|node| node.commit),
            self.reviews,
        )
    }
}

async fn execute<T: DeserializeOwned + Send>(
    api: Option<&GitHubApi>,
    query: Query,
) -> Result<Envelope<T>, GitHostError> {
    check_rate_limit()?;
    let envelope: Envelope<T> = if let Some(api) = api {
        api.send(
            api.request(Method::POST, api_url(&["graphql"]))?
                .json(&query),
        )
        .await?
    } else {
        let json = serde_json::to_vec(&query)
            .map_err(|_| GitHostError::PullRequest("Could not serialize GitHub query".into()))?;
        let response = tokio::task::spawn_blocking(move || GhCli::new().graphql(&json))
            .await
            .map_err(|_| {
                GitHostError::PullRequest("GitHub CLI request did not complete".into())
            })??;
        serde_json::from_str(&response)
            .map_err(|_| GitHostError::UnexpectedOutput("Invalid GitHub GraphQL response".into()))?
    };
    if envelope
        .errors
        .iter()
        .any(|error| error.kind.as_deref() == Some("RATE_LIMITED"))
    {
        return Err(check_rate_limit()
            .err()
            .unwrap_or_else(|| rate_limited(Duration::from_secs(60))));
    }
    if envelope.errors.iter().any(|error| error.path.is_empty()) {
        return Err(GitHostError::PullRequest(
            "GitHub GraphQL rejected the query".into(),
        ));
    }
    Ok(envelope)
}

fn apply_rate_limit(limit: Option<RateLimit>) {
    if let Some(limit) = limit
        && limit.remaining == 0
    {
        let delay = limit
            .reset_at
            .signed_duration_since(Utc::now())
            .to_std()
            .unwrap_or_default();
        defer_requests(delay.saturating_add(Duration::from_secs(1)));
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommitPage {
    node: Option<Commit>,
    rate_limit: Option<RateLimit>,
}

async fn summarize_checks(
    api: Option<&GitHubApi>,
    commit: Option<Commit>,
) -> Result<PrCheckSummary, GitHostError> {
    let mut summary = PrCheckSummary::default();
    let Some(Commit {
        id,
        status_check_rollup: Some(rollup),
    }) = commit
    else {
        return Ok(summary);
    };
    let mut page = rollup.contexts;
    loop {
        for context in page.nodes {
            context.add_to_summary(&mut summary);
        }
        if !page.page_info.has_next_page {
            return Ok(summary);
        }
        let cursor = page.page_info.end_cursor.ok_or_else(incomplete_checks)?;
        let query = Query {
            query: format!(
                "query($id: ID!, $cursor: String!) {{ {RATE_LIMIT} node(id: $id) {{ ... on Commit {{ id statusCheckRollup {{ contexts(first: 100, after: $cursor) {{ {CONTEXTS} }} }} }} }} }}"
            ),
            variables: HashMap::from([
                ("id".into(), id.clone().into()),
                ("cursor".into(), cursor.into()),
            ]),
        };
        let envelope = execute::<CommitPage>(api, query).await?;
        let data = envelope.data.ok_or_else(incomplete_checks)?;
        apply_rate_limit(data.rate_limit);
        page = data
            .node
            .and_then(|commit| commit.status_check_rollup)
            .map(|rollup| rollup.contexts)
            .ok_or_else(incomplete_checks)?;
    }
}

fn incomplete_checks() -> GitHostError {
    GitHostError::UnexpectedOutput("GitHub did not return a complete page of CI checks".into())
}

async fn snapshot(
    api: Option<&GitHubApi>,
    request: &(GitHubRepoInfo, i64),
    pr: PullRequest,
) -> Result<GitHubPrSnapshot, GitHostError> {
    let (mut detail, commit, reviews) = pr.into_parts();
    detail.checks = Some(summarize_checks(api, commit).await?);
    let mut review_comments = Vec::new();
    if matches!(detail.status, MergeStatus::Open)
        && let Some(reviews) = reviews
    {
        review_comments = if reviews.page_info.has_previous_page
            || reviews
                .nodes
                .iter()
                .any(|r| r.comments.page_info.has_previous_page)
        {
            rest::review_comments(api, &request.0, request.1).await?
        } else {
            reviews
                .nodes
                .into_iter()
                .filter(|review| review.state != "PENDING")
                .flat_map(|r| r.comments.nodes)
                .filter_map(ReviewComment::into_unified)
                .collect()
        };
    }
    Ok(GitHubPrSnapshot {
        detail,
        review_comments,
    })
}

pub(super) async fn batch(
    api: Option<&GitHubApi>,
    requests: &[(GitHubRepoInfo, i64)],
    include_reviews: bool,
) -> Result<Vec<Result<GitHubPrSnapshot, GitHostError>>, GitHostError> {
    if requests.is_empty() {
        return Ok(Vec::new());
    }
    let response = execute::<BatchData>(api, batch_query(requests, include_reviews)).await?;
    let mut data = response.data.ok_or_else(|| {
        GitHostError::PullRequest("GitHub GraphQL response did not contain data".into())
    })?;
    apply_rate_limit(data.rate_limit);
    let mut results = Vec::with_capacity(requests.len());
    for (i, request) in requests.iter().enumerate() {
        let alias = format!("p{i}");
        let result = match data
            .repositories
            .remove(&alias)
            .flatten()
            .and_then(|repo| repo.pull_request)
        {
            Some(pr)
                if !response
                    .errors
                    .iter()
                    .any(|err| err.path.first().and_then(Value::as_str) == Some(&alias)) =>
            {
                snapshot(api, request, pr).await
            }
            _ => Err(GitHostError::RepoNotFoundOrNoAccess(format!(
                "GitHub PR {}/{}#{} unavailable",
                request.0.owner, request.0.repo_name, request.1
            ))),
        };
        results.push(result);
    }
    Ok(results)
}

pub(super) async fn list_prs_for_branch(
    api: &GitHubApi,
    repo: &GitHubRepoInfo,
    branch: &str,
) -> Result<Vec<PullRequestDetail>, GitHostError> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Data {
        rate_limit: Option<RateLimit>,
        repository: Option<BranchRepository>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct BranchRepository {
        pull_requests: Nodes<PullRequest>,
    }

    let query = Query {
        query: format!(
            "query($owner: String!, $name: String!, $branch: String!) {{ {RATE_LIMIT} repository(owner: $owner, name: $name) {{ pullRequests(first: 30, headRefName: $branch, orderBy: {{field: CREATED_AT, direction: DESC}}) {{ nodes {{ {DETAIL} }} }} }} }}"
        ),
        variables: HashMap::from([
            ("owner".into(), repo.owner.clone().into()),
            ("name".into(), repo.repo_name.clone().into()),
            ("branch".into(), branch.into()),
        ]),
    };
    let data = execute::<Data>(Some(api), query)
        .await?
        .data
        .ok_or_else(|| GitHostError::UnexpectedOutput("No GitHub branch PR data".into()))?;
    apply_rate_limit(data.rate_limit);
    let repo = data.repository.ok_or_else(|| {
        GitHostError::RepoNotFoundOrNoAccess("GitHub repository unavailable".into())
    })?;
    Ok(repo
        .pull_requests
        .nodes
        .into_iter()
        .map(|pr| pr.into_parts().0)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_one_parameterized_batch_with_optional_reviews() {
        let prs = [
            rest::public_pr("https://github.com/a/b/pull/12").unwrap(),
            rest::public_pr("https://github.com/c/d/pull/34").unwrap(),
        ];
        let query = batch_query(&prs, true);
        assert!(
            query
                .query
                .contains("p0: repository(owner: $owner0, name: $repo0)")
        );
        assert!(
            query
                .query
                .contains("p1: repository(owner: $owner1, name: $repo1)")
        );
        assert_eq!(query.variables["owner1"], "c");
        assert_eq!(query.variables["num0"], 12);
        assert_eq!(query.query.matches("reviews(last: 20)").count(), 2);
        assert!(!batch_query(&prs, false).query.contains("reviews(last:"));
        assert_eq!(query.query.matches("rateLimit").count(), 1);
    }

    #[tokio::test]
    async fn parses_mixed_check_states_review_comments_and_partial_results() {
        let mut fixture: Envelope<BatchData> =
            serde_json::from_str(include_str!("fixtures/graphql-batch.json")).unwrap();
        assert_eq!(fixture.errors[0].path[0], "p1");
        let mut repositories = fixture.data.take().unwrap().repositories;
        let request = rest::public_pr("https://github.com/example/project/pull/12").unwrap();
        let pr = repositories
            .remove("p0")
            .flatten()
            .unwrap()
            .pull_request
            .unwrap();
        let outcome = snapshot(None, &request, pr).await.unwrap();
        let checks = outcome.detail.checks.unwrap();
        assert_eq!(
            (checks.total, checks.passed, checks.failed, checks.pending),
            (5, 2, 2, 1)
        );
        assert_eq!(checks.failures[0].name, "Windows tests");
        assert_eq!(
            checks.failures[0].url.as_deref(),
            Some("https://github.com/example/project/actions/runs/42")
        );
        assert_eq!(checks.failures[1].name, "external/coverage");
        assert!(matches!(
            outcome.review_comments.as_slice(),
            [UnifiedPrComment::Review { id: 1024, .. }]
        ));
        assert!(repositories["p1"].is_none());
    }
}
