//! Bounded, platform-independent command palette ranking.

use std::{cmp::Ordering, collections::BinaryHeap};

use crate::issue_actions::{
    ACTIONS, Action, ActionContext, ActionId, EntityId, IssueRef, ProjectId, Unavailable,
    WorkspaceId,
};

pub const MAX_RESULTS: usize = 50;

/// A view of an existing API record. The inventory supplies one row per ID.
#[derive(Debug, Clone, Copy)]
pub enum SearchEntity<'a> {
    Issue {
        id: IssueRef<'a>,
        title: &'a str,
        short_id: &'a str,
    },
    Project {
        id: ProjectId<'a>,
        name: &'a str,
    },
    Workspace {
        id: WorkspaceId<'a>,
        name: Option<&'a str>,
        branch: &'a str,
    },
}

impl<'a> SearchEntity<'a> {
    pub fn id(&self) -> EntityId<'a> {
        match *self {
            Self::Issue { id, .. } => EntityId::Issue(id),
            Self::Project { id, .. } => EntityId::Project(id),
            Self::Workspace { id, .. } => EntityId::Workspace(id),
        }
    }

    pub fn label(&self) -> &'a str {
        match *self {
            Self::Issue { title, .. } => title,
            Self::Project { name, .. } => name,
            Self::Workspace { name, branch, .. } => name.unwrap_or(branch),
        }
    }

    pub fn open_action(&self) -> Action<'a> {
        Action::OpenEntity(self.id())
    }

    fn rank(&self, query: &Query) -> Option<FieldRank> {
        let primary = query.rank(self.label()).map(|rank| (rank, false));
        let secondary = match *self {
            Self::Issue { short_id, .. } => query.rank(short_id),
            Self::Workspace {
                name: Some(name),
                branch,
                ..
            } if name != branch => query.rank(branch),
            _ => None,
        };
        primary
            .into_iter()
            .chain(secondary.map(|rank| (rank, true)))
            .min()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SearchId<'a> {
    Action(ActionId),
    Entity(EntityId<'a>),
}

#[derive(Debug, Clone, Copy)]
pub enum SearchEntry<'a> {
    Action(ActionId),
    Entity(SearchEntity<'a>),
}

impl<'a> SearchEntry<'a> {
    pub fn id(&self) -> SearchId<'a> {
        match self {
            Self::Action(id) => SearchId::Action(*id),
            Self::Entity(entity) => SearchId::Entity(entity.id()),
        }
    }

    pub fn label(&self) -> &'a str {
        match self {
            Self::Action(id) => id.spec().label,
            Self::Entity(entity) => entity.label(),
        }
    }

    pub fn action_id(&self) -> ActionId {
        match self {
            Self::Action(id) => *id,
            Self::Entity(entity) => entity.open_action().id(),
        }
    }
}

#[derive(Debug)]
pub struct SearchHit<'a> {
    pub entry: SearchEntry<'a>,
    pub availability: Result<(), Unavailable>,
    rank: FieldRank,
}

impl SearchHit<'_> {
    fn sort_key(&self) -> (FieldRank, bool, SearchId<'_>) {
        (self.rank, self.availability.is_err(), self.entry.id())
    }
}

impl Ord for SearchHit<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

impl PartialOrd for SearchHit<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for SearchHit<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for SearchHit<'_> {}

/// Orders exact, prefix, contiguous, then gapped matches. An empty query shows
/// palette actions only. Disabled actions carry the same reason as dispatch.
/// Normalizes the query once, streams each field, and retains only the best hits.
pub fn search<'a>(
    query: &str,
    entities: impl IntoIterator<Item = SearchEntity<'a>>,
    context: ActionContext,
    limit: usize,
) -> Vec<SearchHit<'a>> {
    let limit = limit.min(MAX_RESULTS);
    if limit == 0 {
        return Vec::new();
    }
    let query = Query::new(query);
    let mut best = BinaryHeap::with_capacity(limit);
    let mut offer = |entry: SearchEntry<'a>, rank| {
        let hit = SearchHit {
            entry,
            availability: entry.action_id().availability(context),
            rank,
        };
        if best.len() < limit {
            best.push(hit);
        } else if let Some(mut worst) = best.peek_mut()
            && hit < *worst
        {
            *worst = hit;
        }
    };

    for spec in ACTIONS.iter().filter(|spec| spec.in_palette) {
        let rank = if query.is_empty() {
            Some((TextRank::EXACT, false))
        } else {
            query
                .rank(spec.label)
                .map(|rank| (rank, false))
                .into_iter()
                .chain(
                    spec.aliases
                        .iter()
                        .copied()
                        .chain([spec.key])
                        .filter_map(|alias| query.rank(alias).map(|rank| (rank, true))),
                )
                .min()
        };
        if let Some(rank) = rank {
            offer(SearchEntry::Action(spec.id), rank);
        }
    }
    if !query.is_empty() {
        for entity in entities {
            if let Some(rank) = entity.rank(&query) {
                offer(SearchEntry::Entity(entity), rank);
            }
        }
    }
    best.into_sorted_vec()
}

type FieldRank = (TextRank, bool);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchKind {
    Exact,
    Prefix,
    Contiguous,
    Subsequence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TextRank {
    kind: MatchKind,
    gaps: usize,
    start: usize,
}

impl TextRank {
    const EXACT: Self = Self {
        kind: MatchKind::Exact,
        gaps: 0,
        start: 0,
    };
}

struct Query(Vec<QueryChar>);

struct QueryChar {
    value: char,
    fallback: usize,
}

/// Stream Unicode scalar lowercase expansions. Whitespace in either string is
/// ignored; accents and punctuation retain their literal meaning.
fn search_chars(text: &str) -> impl Iterator<Item = char> + '_ {
    text.chars()
        .filter(|ch| !ch.is_whitespace())
        .flat_map(char::to_lowercase)
}

impl Query {
    fn new(input: &str) -> Self {
        let mut chars: Vec<QueryChar> = Vec::new();
        let mut prefix = 0;
        for value in search_chars(input) {
            if !chars.is_empty() {
                while prefix > 0 && chars[prefix].value != value {
                    prefix = chars[prefix - 1].fallback;
                }
                if chars[prefix].value == value {
                    prefix += 1;
                }
            }
            chars.push(QueryChar {
                value,
                fallback: prefix,
            });
        }
        Self(chars)
    }

    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    fn rank(&self, field: &str) -> Option<TextRank> {
        let query = &self.0;
        let mut chars = search_chars(field).enumerate();
        let mut prefix = 0;
        let mut matched = 0;
        let mut first = 0;
        let mut fuzzy = None;

        while let Some((index, value)) = chars.next() {
            // KMP finds the first contiguous match, including after a gapped hit.
            while prefix > 0 && query[prefix].value != value {
                prefix = query[prefix - 1].fallback;
            }
            if query[prefix].value == value {
                prefix += 1;
                if prefix == query.len() {
                    let start = index + 1 - prefix;
                    let kind = if start > 0 {
                        MatchKind::Contiguous
                    } else if chars.next().is_some() {
                        MatchKind::Prefix
                    } else {
                        MatchKind::Exact
                    };
                    return Some(TextRank {
                        kind,
                        gaps: 0,
                        start,
                    });
                }
            }

            if let Some(expected) = query.get(matched)
                && expected.value == value
            {
                if matched == 0 {
                    first = index;
                }
                matched += 1;
                if matched == query.len() {
                    fuzzy = Some(TextRank {
                        kind: MatchKind::Subsequence,
                        gaps: index + 1 - first - matched,
                        start: first,
                    });
                }
            }
        }
        fuzzy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::issue_actions::{ActionScope, InputState, IssueId};

    const READY: ActionContext = ActionContext {
        scope: ActionScope::Issue,
        input: InputState::Ready,
        connected: true,
        mutation_pending: false,
    };

    fn issue(
        id: &'static str,
        title: &'static str,
        short_id: &'static str,
    ) -> SearchEntity<'static> {
        SearchEntity::Issue {
            id: IssueRef {
                project_id: ProjectId("project"),
                issue_id: IssueId(id),
            },
            title,
            short_id,
        }
    }

    #[test]
    fn searches_issues_projects_workspaces_and_actions_together() {
        let issue = issue("issue", "Agent 复盘", "VO-42");
        let project = SearchEntity::Project {
            id: ProjectId("tools"),
            name: "Agent 工具",
        };
        let workspace = SearchEntity::Workspace {
            id: WorkspaceId("work"),
            name: Some("日志"),
            branch: "agent/repair",
        };
        let hits = search("aGeNt", [issue, project, workspace], READY, MAX_RESULTS);
        assert_eq!(
            hits.iter().map(|hit| hit.entry.id()).collect::<Vec<_>>(),
            [
                SearchId::Entity(issue.id()),
                SearchId::Entity(project.id()),
                SearchId::Entity(workspace.id()),
                SearchId::Action(ActionId::StartWorkspace),
            ]
        );
        assert_eq!(hits[0].entry.label(), "Agent 复盘");
        let hits = search("vo42", [issue, project, workspace], READY, 1);
        assert_eq!(hits[0].entry.id(), SearchId::Entity(issue.id()));
        assert_eq!(hits[0].entry.action_id(), ActionId::OpenIssue);
        assert_eq!(issue.open_action(), Action::OpenEntity(issue.id()));
        let hits = search("change status", [], READY, 1);
        assert_eq!(hits[0].entry.id(), SearchId::Action(ActionId::MoveIssue));
    }

    #[test]
    fn unicode_lowercase_and_gapped_scalar_ordering() {
        let candidates = [
            issue("wide", "修复很难的日志", "VO-1"),
            issue("tight", "修复日志", "VO-2"),
            issue("exact", "修日", "VO-3"),
            issue("greek", "ΩΜΈΓΑ", "VO-4"),
            issue("dotted-i", "İzmir", "VO-5"),
            issue("reversed", "日志修复", "VO-6"),
        ];
        let hits = search("修　日", candidates, READY, MAX_RESULTS);
        assert_eq!(
            hits.iter().map(|hit| hit.entry.label()).collect::<Vec<_>>(),
            ["修日", "修复日志", "修复很难的日志"]
        );
        assert_eq!(search("ωγ", candidates, READY, 1)[0].entry.label(), "ΩΜΈΓΑ");
        assert_eq!(
            search("i\u{307}z", candidates, READY, 1)[0].entry.label(),
            "İzmir"
        );
    }

    #[test]
    fn exact_prefix_and_later_contiguous_beat_gapped_matches() {
        let candidates = [
            issue("gap", "A B-X A B", "VO-1"),
            issue("later", "A B-X A B A B", "VO-2"),
            issue("prefix", "ABAB etc", "VO-3"),
            issue("exact", "abab", "VO-4"),
            issue("fallback", "abaababc", "VO-5"),
        ];
        let hits = search("abab", candidates, READY, MAX_RESULTS);
        assert_eq!(
            hits.iter().map(|hit| hit.entry.label()).collect::<Vec<_>>(),
            ["abab", "ABAB etc", "abaababc", "A B-X A B A B", "A B-X A B"]
        );
    }

    #[test]
    fn limit_is_hard_and_ties_are_stable_across_input_order() {
        let ids: Vec<_> = (0..5_000).map(|i| format!("{i:04}")).collect();
        let candidates = ids.iter().map(|id| SearchEntity::Project {
            id: ProjectId(id),
            name: "匹配",
        });
        let hits = search("匹配", candidates.clone().rev(), READY, usize::MAX);
        assert_eq!(hits.len(), MAX_RESULTS);
        for (hit, id) in hits.iter().zip(&ids) {
            assert_eq!(
                hit.entry.id(),
                SearchId::Entity(EntityId::Project(ProjectId(id)))
            );
        }
        assert_eq!(search("匹配", candidates, READY, 3), hits[..3]);
    }

    #[test]
    fn empty_and_zero_limit_skip_entity_iteration() {
        let unvisited = || {
            std::iter::from_fn(|| -> Option<SearchEntity<'static>> { panic!("inventory visited") })
        };
        assert!(search("修复", unvisited(), READY, 0).is_empty());
        let offline = ActionContext {
            connected: false,
            ..READY
        };
        let hits = search("　 ", unvisited(), offline, MAX_RESULTS);
        assert_eq!(hits.len(), 4);
        for hit in hits {
            assert!(matches!(hit.entry, SearchEntry::Action(_)));
            assert_eq!(hit.availability, Err(Unavailable::Disconnected));
            assert_eq!(
                hit.availability,
                hit.entry.action_id().availability(offline)
            );
        }
        assert!(search("无匹配内容", [], READY, MAX_RESULTS).is_empty());
    }
}
