//! Serde projections and HTTP operations for the M2 local board API.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::issue_actions::{IssueEdit, IssueId, IssueRef, Priority, ProjectId, StatusId};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusCategory {
    Backlog,
    Unstarted,
    Started,
    Completed,
    Canceled,
}

impl StatusCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::Unstarted => "unstarted",
            Self::Started => "started",
            Self::Completed => "completed",
            Self::Canceled => "canceled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssuePriority {
    Urgent,
    High,
    Medium,
    Low,
}

impl IssuePriority {
    fn as_str(self) -> &'static str {
        match self {
            Self::Urgent => "urgent",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
        }
    }
}

impl From<Priority> for IssuePriority {
    fn from(value: Priority) -> Self {
        match value {
            Priority::Urgent => Self::Urgent,
            Priority::High => Self::High,
            Priority::Medium => Self::Medium,
            Priority::Low => Self::Low,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Issue {
    pub id: String,
    pub project_id: String,
    pub issue_number: i64,
    pub simple_id: String,
    pub status_id: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: Option<IssuePriority>,
    pub start_date: Option<String>,
    pub target_date: Option<String>,
    pub sort_order: f64,
    pub parent_issue_id: Option<String>,
    pub cycle_id: Option<String>,
    pub triage: bool,
    pub tag_ids: Vec<String>,
    pub extension_metadata: Value,
    pub external_provider: Option<String>,
    pub external_id: Option<String>,
    pub external_url: Option<String>,
}

impl Issue {
    pub fn reference(&self) -> IssueRef<'_> {
        IssueRef {
            project_id: ProjectId(&self.project_id),
            issue_id: IssueId(&self.id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Status {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub color: String,
    pub category: StatusCategory,
    pub sort_order: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Tag {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Cycle {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub start_date: String,
    pub end_date: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SavedView {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub filter: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipType {
    Blocking,
    Related,
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Relationship {
    pub id: String,
    pub project_id: String,
    pub issue_id: String,
    pub related_issue_id: String,
    pub relationship_type: RelationshipType,
}

#[derive(Debug, Serialize)]
pub struct SaveStatus<'a> {
    pub name: &'a str,
    pub color: &'a str,
    pub category: StatusCategory,
}

#[derive(Debug, Serialize)]
pub struct SaveTag<'a> {
    pub name: &'a str,
    pub color: &'a str,
}

#[derive(Debug, Serialize)]
pub struct SaveCycle<'a> {
    pub name: &'a str,
    pub start_date: &'a str,
    pub end_date: &'a str,
}

#[derive(Debug, Serialize)]
pub struct SaveView<'a> {
    pub name: &'a str,
    pub filter: &'a Value,
}

#[derive(Debug, Default, Serialize)]
pub struct CreateIssue<'a> {
    pub title: &'a str,
    pub status_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<IssuePriority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_date: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_issue_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triage: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_ids: Option<&'a [&'a str]>,
}

/// Outer `None` omits a PATCH property. `Some(None)` explicitly clears it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct IssuePatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<Option<IssuePriority>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_date: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_issue_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triage: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_ids: Option<Vec<String>>,
}

impl From<IssueEdit<'_>> for IssuePatch {
    fn from(edit: IssueEdit<'_>) -> Self {
        let mut patch = Self::default();
        match edit {
            IssueEdit::Title(value) => patch.title = Some(value.trim().into()),
            IssueEdit::Description(value) => patch.description = Some(value.map(str::to_owned)),
            IssueEdit::Priority(value) => patch.priority = Some(value.map(Into::into)),
            IssueEdit::StartDate(value) => patch.start_date = Some(value.map(str::to_owned)),
            IssueEdit::TargetDate(value) => patch.target_date = Some(value.map(str::to_owned)),
            IssueEdit::Parent(value) => {
                patch.parent_issue_id = Some(value.map(|issue| issue.issue_id.0.into()))
            }
            IssueEdit::Cycle(value) => patch.cycle_id = Some(value.map(str::to_owned)),
            IssueEdit::Triage(value) => patch.triage = Some(value),
            IssueEdit::Tags(value) => {
                patch.tag_ids = Some(value.iter().map(|id| (*id).into()).collect())
            }
        }
        patch
    }
}

#[derive(Debug, PartialEq, Serialize)]
pub struct IssueUpdate {
    pub issue_id: String,
    pub changes: IssuePatch,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct IssueFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_category: Option<StatusCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<IssuePriority>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_issue_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triage: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_on_or_after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_on_or_before: Option<String>,
}

impl IssueFilter {
    pub fn path(&self, project: ProjectId<'_>) -> String {
        fn bool_str(value: bool) -> &'static str {
            if value { "true" } else { "false" }
        }
        let mut path = project_path(project, "issues");
        let mut delimiter = '?';
        for (key, value) in [
            ("search", self.search.as_deref()),
            ("status_id", self.status_id.as_deref()),
            (
                "status_category",
                self.status_category.map(StatusCategory::as_str),
            ),
            ("priority", self.priority.map(IssuePriority::as_str)),
            ("tag_id", self.tag_id.as_deref()),
            ("parent_issue_id", self.parent_issue_id.as_deref()),
            ("root_only", self.root_only.map(bool_str)),
            ("cycle_id", self.cycle_id.as_deref()),
            ("triage", self.triage.map(bool_str)),
            ("external_provider", self.external_provider.as_deref()),
            ("external_id", self.external_id.as_deref()),
            ("start_on_or_after", self.start_on_or_after.as_deref()),
            ("target_on_or_before", self.target_on_or_before.as_deref()),
        ] {
            if let Some(value) = value {
                path.push(delimiter);
                delimiter = '&';
                path.push_str(key);
                path.push('=');
                push_component(&mut path, value);
            }
        }
        path
    }
}

fn push_component(path: &mut String, value: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            path.push(char::from(byte));
        } else {
            path.push('%');
            path.push(char::from(HEX[(byte >> 4) as usize]));
            path.push(char::from(HEX[(byte & 15) as usize]));
        }
    }
}

fn project_path(project: ProjectId<'_>, resource: &str) -> String {
    let mut path = String::from("/api/projects/");
    push_component(&mut path, project.0);
    path.push('/');
    path.push_str(resource);
    path
}

#[cfg(any(windows, target_arch = "wasm32"))]
mod http {
    use serde::de::DeserializeOwned;

    use super::*;
    use crate::Api;

    fn item_path(mut collection: String, id: &str) -> String {
        collection.push('/');
        push_component(&mut collection, id);
        collection
    }

    fn issue_path(issue: IssueRef<'_>) -> String {
        item_path(project_path(issue.project_id, "issues"), issue.issue_id.0)
    }

    fn child_issue_path(issue: IssueRef<'_>, part: &str) -> String {
        let mut path = issue_path(issue);
        path.push('/');
        path.push_str(part);
        path
    }

    macro_rules! board_resource {
        ($path:literal, $model:ty, $save:ty, $list:ident, $get:ident, $add:ident, $edit:ident, $delete:ident) => {
            pub async fn $list(&self, project: ProjectId<'_>) -> Result<Vec<$model>, String> {
                self.request("GET", &project_path(project, $path), None)
                    .await
            }

            pub async fn $get(&self, project: ProjectId<'_>, id: &str) -> Result<$model, String> {
                self.request("GET", &item_path(project_path(project, $path), id), None)
                    .await
            }

            pub async fn $add(
                &self,
                project: ProjectId<'_>,
                payload: &$save,
            ) -> Result<$model, String> {
                self.send_json("POST", &project_path(project, $path), payload)
                    .await
            }

            pub async fn $edit(
                &self,
                project: ProjectId<'_>,
                id: &str,
                payload: &$save,
            ) -> Result<$model, String> {
                self.send_json("PUT", &item_path(project_path(project, $path), id), payload)
                    .await
            }

            pub async fn $delete(&self, project: ProjectId<'_>, id: &str) -> Result<(), String> {
                self.request("DELETE", &item_path(project_path(project, $path), id), None)
                    .await
            }
        };
    }

    impl Api {
        async fn send_json<R: DeserializeOwned>(
            &self,
            method: &str,
            path: &str,
            payload: &(impl Serialize + ?Sized),
        ) -> Result<R, String> {
            let body = serde_json::to_vec(payload).map_err(|error| error.to_string())?;
            self.request(method, path, Some(body)).await
        }

        pub async fn projects(&self) -> Result<Vec<Project>, String> {
            self.request("GET", "/api/projects", None).await
        }

        pub async fn create_project(&self, name: &str) -> Result<Project, String> {
            #[derive(Serialize)]
            struct Payload<'a> {
                name: &'a str,
            }
            self.send_json("POST", "/api/projects", &Payload { name })
                .await
        }

        pub async fn issues(
            &self,
            project: ProjectId<'_>,
            filter: &IssueFilter,
        ) -> Result<Vec<Issue>, String> {
            self.request("GET", &filter.path(project), None).await
        }

        pub async fn issue(&self, issue: IssueRef<'_>) -> Result<Issue, String> {
            self.request("GET", &issue_path(issue), None).await
        }

        pub async fn create_issue(
            &self,
            project: ProjectId<'_>,
            payload: &CreateIssue<'_>,
        ) -> Result<Issue, String> {
            self.send_json("POST", &project_path(project, "issues"), payload)
                .await
        }

        pub async fn update_issue(
            &self,
            issue: IssueRef<'_>,
            changes: &IssuePatch,
        ) -> Result<Issue, String> {
            self.send_json("PATCH", &issue_path(issue), changes).await
        }

        pub async fn update_issues(
            &self,
            project: ProjectId<'_>,
            updates: &[IssueUpdate],
        ) -> Result<Vec<Issue>, String> {
            self.send_json("PATCH", &project_path(project, "issues/batch"), updates)
                .await
        }

        pub async fn delete_issue(&self, issue: IssueRef<'_>) -> Result<(), String> {
            self.request("DELETE", &issue_path(issue), None).await
        }

        pub async fn issue_children(&self, issue: IssueRef<'_>) -> Result<Vec<Issue>, String> {
            self.request("GET", &child_issue_path(issue, "children"), None)
                .await
        }

        pub async fn create_issue_child(
            &self,
            parent: IssueRef<'_>,
            payload: &CreateIssue<'_>,
        ) -> Result<Issue, String> {
            self.send_json("POST", &child_issue_path(parent, "children"), payload)
                .await
        }

        pub async fn issue_relationships(
            &self,
            issue: IssueRef<'_>,
        ) -> Result<Vec<Relationship>, String> {
            self.request("GET", &child_issue_path(issue, "relationships"), None)
                .await
        }

        pub async fn create_issue_relationship(
            &self,
            issue: IssueRef<'_>,
            related_issue_id: IssueId<'_>,
            relationship_type: RelationshipType,
        ) -> Result<Relationship, String> {
            #[derive(Serialize)]
            struct Payload<'a> {
                related_issue_id: &'a str,
                relationship_type: RelationshipType,
            }
            self.send_json(
                "POST",
                &child_issue_path(issue, "relationships"),
                &Payload {
                    related_issue_id: related_issue_id.0,
                    relationship_type,
                },
            )
            .await
        }

        pub async fn delete_issue_relationship(
            &self,
            issue: IssueRef<'_>,
            relationship_id: &str,
        ) -> Result<(), String> {
            self.request(
                "DELETE",
                &item_path(child_issue_path(issue, "relationships"), relationship_id),
                None,
            )
            .await
        }

        pub async fn reorder_statuses(
            &self,
            project: ProjectId<'_>,
            ids: &[StatusId<'_>],
        ) -> Result<Vec<Status>, String> {
            #[derive(Serialize)]
            struct Payload<'a> {
                #[serde(serialize_with = "serialize_status_ids")]
                status_ids: &'a [StatusId<'a>],
            }
            fn serialize_status_ids<S: serde::Serializer>(
                ids: &[StatusId<'_>],
                serializer: S,
            ) -> Result<S::Ok, S::Error> {
                serializer.collect_seq(ids.iter().map(|id| id.0))
            }
            self.send_json(
                "PUT",
                &project_path(project, "statuses/reorder"),
                &Payload { status_ids: ids },
            )
            .await
        }

        board_resource!(
            "statuses",
            Status,
            SaveStatus<'_>,
            statuses,
            status,
            create_status,
            update_status,
            delete_status
        );
        board_resource!(
            "tags",
            Tag,
            SaveTag<'_>,
            issue_tags,
            issue_tag,
            create_issue_tag,
            update_issue_tag,
            delete_issue_tag
        );
        board_resource!(
            "cycles",
            Cycle,
            SaveCycle<'_>,
            cycles,
            cycle,
            create_cycle,
            update_cycle,
            delete_cycle
        );
        board_resource!(
            "saved-views",
            SavedView,
            SaveView<'_>,
            saved_views,
            saved_view,
            create_saved_view,
            update_saved_view,
            delete_saved_view
        );
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn query_encodes_literals_and_combines_filters() {
        let filter = IssueFilter {
            search: Some("修订 90%_&+".into()),
            status_category: Some(StatusCategory::Started),
            root_only: Some(false),
            external_id: Some("owner/repo#7".into()),
            ..IssueFilter::default()
        };
        assert_eq!(
            filter.path(ProjectId("project")),
            "/api/projects/project/issues?search=%E4%BF%AE%E8%AE%A2%2090%25_%26%2B&status_category=started&root_only=false&external_id=owner%2Frepo%237"
        );
        assert_eq!(
            IssueFilter::default().path(ProjectId("project")),
            "/api/projects/project/issues"
        );
    }

    #[test]
    fn patch_distinguishes_unchanged_clear_and_replace() {
        let change = IssuePatch::from(IssueEdit::Description(None));
        assert_eq!(
            serde_json::to_value(change).unwrap(),
            json!({"description": null})
        );
        let change = IssuePatch::from(IssueEdit::Tags(&[]));
        assert_eq!(
            serde_json::to_value(change).unwrap(),
            json!({"tag_ids": []})
        );
        let change = IssuePatch::from(IssueEdit::Priority(Some(Priority::High)));
        assert_eq!(
            serde_json::to_value(change).unwrap(),
            json!({"priority": "high"})
        );
    }
}
