use std::rc::Rc;

use futures_util::{future::join_all, join};
use nsh_core::prelude::*;
use serde_json::{Value, json};

use crate::api::Api;

#[path = "workspace_lifecycle_data.rs"]
mod data;
#[path = "workspace_lifecycle_view.rs"]
mod ui;

use data::*;

#[derive(Clone)]
pub struct LifecycleContext {
    api: Api,
    selected: Signal<Option<String>>,
    issue: Option<Signal<Option<String>>>,
}

impl LifecycleContext {
    pub fn new(api: Api, selected: Signal<Option<String>>) -> Self {
        Self {
            api,
            selected,
            issue: None,
        }
    }

    pub fn for_issue(mut self, issue: Signal<Option<String>>) -> Self {
        self.issue = Some(issue);
        self
    }
}

#[derive(Clone, PartialEq)]
struct DraftRepo {
    choice: RepoChoice,
    branches: Vec<Branch>,
}

#[derive(Clone, Default, PartialEq)]
struct Draft {
    issue: Option<String>,
    name: String,
    prompt: String,
    path: String,
    repos: Vec<DraftRepo>,
    profiles: Vec<ProfileChoice>,
    profile: Option<usize>,
    catalog: Vec<Repo>,
    add_repo: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum GitFormKind {
    Rebase,
    Target,
    PullRequest,
}

#[derive(Clone, PartialEq)]
struct GitForm {
    kind: GitFormKind,
    workspace: String,
    repo: String,
    source: String,
    old_target: String,
    target: String,
    branches: Vec<Branch>,
    title: String,
    body: String,
    draft: bool,
}

#[derive(Clone, Copy)]
struct State {
    selected: Signal<Option<String>>,
    issue: Option<Signal<Option<String>>>,
    rows: Signal<Vec<Workspace>>,
    git: Signal<Vec<GitStatus>>,
    repos: Signal<Vec<Repo>>,
    processes: Signal<Option<Processes>>,
    notice: Signal<Option<(bool, String)>>,
    status_notice: Signal<Option<String>>,
    process_notice: Signal<Option<String>>,
    busy: Signal<bool>,
    loading: Signal<bool>,
    revision: Signal<u64>,
    git_revision: Signal<u64>,
    show_archived: Signal<bool>,
    create: Signal<Option<Draft>>,
    git_form: Signal<Option<GitForm>>,
    returned_url: Signal<Option<String>>,
}

impl State {
    fn new(cx: LifecycleContext) -> Self {
        Self {
            selected: cx.selected,
            issue: cx.issue,
            rows: signal(Vec::new()),
            git: signal(Vec::new()),
            repos: signal(Vec::new()),
            processes: signal(None),
            notice: signal(None),
            status_notice: signal(None),
            process_notice: signal(None),
            busy: signal(false),
            loading: signal(true),
            revision: signal(0),
            git_revision: signal(0),
            show_archived: signal(false),
            create: signal(None),
            git_form: signal(None),
            returned_url: signal(None),
        }
    }

    fn selected_row(self) -> Option<Workspace> {
        let id = self.selected.get()?;
        let issue = self.issue.and_then(|issue| issue.get());
        self.rows.with(|rows| {
            rows.iter()
                .find(|row| {
                    row.id == id
                        && issue
                            .as_ref()
                            .is_none_or(|id| row.local_issue_id.as_ref() == Some(id))
                })
                .cloned()
        })
    }

    fn selected_status(self, repo: &str) -> Option<GitStatus> {
        self.git
            .with(|rows| rows.iter().find(|row| row.repo_id == repo).cloned())
    }

    fn edit(self, f: impl FnOnce(&mut Draft)) {
        self.create.update(|draft| {
            if let Some(draft) = draft {
                f(draft);
            }
        });
    }

    fn edit_git(self, f: impl FnOnce(&mut GitForm)) {
        self.git_form.update(|form| {
            if let Some(form) = form {
                f(form);
            }
        });
    }

    fn begin(self) -> bool {
        if self.busy.get_untracked() {
            return false;
        }
        self.busy.set(true);
        self.notice.set(None);
        true
    }

    fn finish(self, result: Result<String, String>) {
        self.busy.set(false);
        self.notice.set(Some(match result {
            Ok(message) => (false, message),
            Err(message) => (true, message),
        }));
    }

    fn refresh_git(self) {
        self.git_revision.update(|revision| *revision += 1);
    }
}

pub fn workspace_lifecycle(cx: LifecycleContext) -> impl IntoElement {
    let api = Rc::new(cx.api.clone());
    let state = State::new(cx);
    let list_api = api.clone();
    effect(move || {
        state.revision.get();
        let api = list_api.clone();
        state.loading.set(true);
        let task = spawn(async move {
            match get::<Vec<Workspace>>(&api, "/api/workspaces").await {
                Ok(rows) => {
                    if let Some(id) = state.selected.get_untracked()
                        && let Some(row) = rows.iter().find(|row| row.id == id)
                    {
                        state.show_archived.set(row.archived);
                    }
                    state.rows.set(rows);
                }
                Err(error) => state.notice.set(Some((true, error))),
            }
            state.loading.set(false);
        });
        cleanup(move || task.cancel());
    });

    let workspace = computed(move || state.selected_row().map(|row| (row.id, row.archived)));
    let repo_api = api.clone();
    effect(move || {
        let active = workspace.get();
        state.git_revision.get();
        state.git.set(Vec::new());
        state.repos.set(Vec::new());
        state.status_notice.set(None);
        let Some((id, false)) = active else { return };
        state.status_notice.set(Some("正在读取 Git 状态…".into()));
        let api = repo_api.clone();
        let task = spawn(async move {
            let repo_path = format!("/api/workspaces/{id}/repos");
            let git_path = format!("/api/workspaces/{id}/git/status");
            let (repos, git) = join!(
                get::<Vec<Repo>>(&api, &repo_path),
                get::<Vec<GitStatus>>(&api, &git_path),
            );
            let error = repos.as_ref().err().or_else(|| git.as_ref().err()).cloned();
            if let Ok(repos) = repos {
                state.repos.set(repos);
            }
            if let Ok(git) = git {
                state.git.set(git);
            }
            state.status_notice.set(error);
        });
        cleanup(move || task.cancel());
    });

    let process_api = api.clone();
    effect(move || {
        let active = workspace.get();
        state.revision.get();
        state.processes.set(None);
        state.process_notice.set(None);
        let Some((id, false)) = active else { return };
        let api = process_api.as_ref().clone();
        let task = spawn(async move {
            if let Err(error) = watch_processes(api, id, state.processes).await {
                state.processes.set(None);
                state.process_notice.set(Some(error));
            }
        });
        cleanup(move || task.cancel());
    });
    ui::panel(api, state)
}

fn open_create(api: Rc<Api>, state: State) {
    if !state.begin() {
        return;
    }
    let issue = state.issue.and_then(|issue| issue.get());
    state.create.set(Some(Draft {
        issue,
        ..Draft::default()
    }));
    spawn(async move {
        let (repos, info) = join!(
            get::<Vec<Repo>>(&api, "/api/repos"),
            get::<SystemInfo>(&api, "/api/info"),
        );
        let error = repos
            .as_ref()
            .err()
            .or_else(|| info.as_ref().err())
            .cloned();
        state.edit(|draft| {
            if let Ok(repos) = repos {
                draft.catalog = repos;
            }
            if let Ok(info) = info {
                (draft.profiles, draft.profile) = info.profiles();
            }
        });
        state.busy.set(false);
        if let Some(error) = error {
            state.notice.set(Some((true, error)));
        }
    });
}

fn add_repo(api: Rc<Api>, state: State, register: bool) {
    let selection = state.create.with(|draft| {
        let draft = draft.as_ref()?;
        if register {
            let path = draft.path.trim();
            (!path.is_empty()).then(|| path.to_owned())
        } else {
            draft.add_repo.clone()
        }
    });
    let Some(selection) = selection else { return };
    if !state.begin() {
        return;
    }
    spawn(async move {
        let result = async {
            let repo: Repo = if register {
                send(&api, "POST", "/api/repos", json!({"path": selection})).await?
            } else {
                state
                    .create
                    .with(|draft| {
                        draft
                            .as_ref()?
                            .catalog
                            .iter()
                            .find(|repo| repo.id == selection)
                            .cloned()
                    })
                    .ok_or("仓库不在列表中，请刷新。")?
            };
            if state.create.with(|draft| {
                draft.as_ref().is_some_and(|draft| {
                    draft
                        .repos
                        .iter()
                        .any(|entry| entry.choice.repo_id == repo.id)
                })
            }) {
                return Err("此仓库已加入工作区。".into());
            }
            let branches: Vec<Branch> =
                get(&api, &format!("/api/repos/{}/branches", repo.id)).await?;
            let target_branch = repo
                .default_target_branch
                .as_ref()
                .filter(|name| branches.iter().any(|branch| &branch.name == *name))
                .cloned()
                .or_else(|| {
                    branches
                        .iter()
                        .find(|branch| branch.is_current)
                        .map(|branch| branch.name.clone())
                })
                .unwrap_or_default();
            state.edit(|draft| {
                let repo_id = repo.id.clone();
                if !draft.catalog.iter().any(|entry| entry.id == repo_id) {
                    draft.catalog.push(repo);
                }
                draft.repos.push(DraftRepo {
                    choice: RepoChoice {
                        repo_id,
                        target_branch,
                    },
                    branches,
                });
                draft.path.clear();
                draft.add_repo = None;
            });
            Ok("已添加本地仓库。".into())
        }
        .await;
        state.finish(result);
    });
}

fn start_workspace(api: Rc<Api>, state: State) {
    let payload = state.create.with(|draft| {
        let draft = draft.as_ref()?;
        let profile = draft.profiles.get(draft.profile?)?;
        let prompt = draft.prompt.trim();
        if prompt.is_empty()
            || draft.repos.is_empty()
            || draft
                .repos
                .iter()
                .any(|entry| entry.choice.target_branch.is_empty())
        {
            return None;
        }
        let repos: Vec<_> = draft.repos.iter().map(|entry| &entry.choice).collect();
        let name = draft.name.trim();
        Some(json!({
            "name": (!name.is_empty()).then_some(name), "repos": repos,
            "local_issue_id": draft.issue, "executor_config": profile, "prompt": prompt,
        }))
    });
    let Some(payload) = payload else { return };
    if !state.begin() {
        return;
    }
    spawn(async move {
        let result = send::<Started>(&api, "POST", "/api/workspaces/start", payload).await;
        state.revision.update(|revision| *revision += 1);
        state.finish(result.map(|created| {
            state.create.set(None);
            state.show_archived.set(false);
            state.selected.set(Some(created.workspace.id.clone()));
            state.rows.update(|rows| {
                rows.retain(|row| row.id != created.workspace.id);
                rows.insert(0, created.workspace);
            });
            "工作区已创建，首轮 Agent 已提交。".into()
        }));
    });
}

#[derive(Clone, Copy)]
enum WorkspaceAction {
    Archive,
    Restore,
    OpenEditor,
    StartDev,
    StopDev,
}

fn workspace_action(api: Rc<Api>, state: State, action: WorkspaceAction) {
    let Some(workspace) = state.selected_row() else {
        return;
    };
    if !state.begin() {
        return;
    }
    let base = format!("/api/workspaces/{}", workspace.id);
    spawn(async move {
        let result: Result<String, String> = async {
            match action {
                WorkspaceAction::Archive | WorkspaceAction::Restore => {
                    let archived = matches!(action, WorkspaceAction::Archive);
                    let _: Value = send(&api, "PUT", &base, json!({"archived": archived})).await?;
                    state.rows.update(|rows| {
                        if let Some(row) = rows.iter_mut().find(|row| row.id == workspace.id) {
                            row.archived = archived;
                        }
                    });
                    state.show_archived.set(archived);
                    Ok(if archived {
                        "工作区已归档。"
                    } else {
                        "工作区已恢复。"
                    }
                    .into())
                }
                WorkspaceAction::OpenEditor => {
                    let response: EditorResponse = send(
                        &api,
                        "POST",
                        &format!("{base}/integration/editor/open"),
                        json!({}),
                    )
                    .await?;
                    state.returned_url.set(response.url.clone());
                    Ok(if response.url.is_some() {
                        "编辑器链接已就绪。"
                    } else {
                        "已打开配置的编辑器。"
                    }
                    .into())
                }
                WorkspaceAction::StartDev => {
                    let processes: Vec<Process> = api
                        .request("POST", &format!("{base}/execution/dev-server/start"), None)
                        .await?;
                    state.processes.update(|status| {
                        if let Some(status) = status {
                            status.running_dev.extend(
                                processes
                                    .into_iter()
                                    .filter(Process::running_dev)
                                    .map(|process| process.id),
                            );
                        }
                    });
                    Ok("已启动开发服务器。".into())
                }
                WorkspaceAction::StopDev => {
                    let ids = state
                        .processes
                        .with(|status| status.as_ref().map(|status| status.running_dev.clone()))
                        .ok_or("开发服务器状态尚未就绪。")?;
                    let results = join_all(ids.iter().map(|id| {
                        let api = &api;
                        async move {
                            api.request::<()>(
                                "POST",
                                &format!("/api/execution-processes/{id}/stop"),
                                None,
                            )
                            .await
                        }
                    }))
                    .await;
                    for result in results {
                        result?;
                    }
                    Ok("已停止开发服务器。".into())
                }
            }
        }
        .await;
        state.revision.update(|revision| *revision += 1);
        state.finish(result);
    });
}

fn open_git_form(api: Rc<Api>, state: State, repo_id: String, kind: GitFormKind) {
    let Some(workspace) = state.selected_row() else {
        return;
    };
    let Some(git) = state.selected_status(&repo_id) else {
        return;
    };
    if !state.begin() {
        return;
    }
    spawn(async move {
        let result = async {
            let branches: Vec<Branch> =
                get(&api, &format!("/api/repos/{repo_id}/branches")).await?;
            state.git_form.set(Some(GitForm {
                kind,
                repo: repo_id,
                workspace: workspace.id,
                source: workspace.branch,
                target: git.target_branch_name.clone(),
                old_target: git.target_branch_name,
                title: workspace.name.unwrap_or_default(),
                branches,
                body: String::new(),
                draft: false,
            }));
            Ok(())
        }
        .await;
        state.busy.set(false);
        if let Err(error) = result {
            state.notice.set(Some((true, error)));
        }
    });
}

fn submit_git_form(api: Rc<Api>, state: State) {
    let Some(form) = state.git_form.get() else {
        return;
    };
    if state
        .selected_row()
        .is_none_or(|row| row.id != form.workspace || row.archived)
    {
        state.git_form.set(None);
        return;
    }
    if !state.begin() {
        return;
    }
    let (method, endpoint, payload, success) = match form.kind {
        GitFormKind::Rebase => (
            "POST",
            "git/rebase",
            json!({"repo_id": form.repo, "old_base_branch": form.old_target, "new_base_branch": form.target}),
            "Rebase 已完成。",
        ),
        GitFormKind::Target => (
            "PUT",
            "git/target-branch",
            json!({"repo_id": form.repo, "new_target_branch": form.target}),
            "目标分支已更新。",
        ),
        GitFormKind::PullRequest => (
            "POST",
            "pull-requests",
            json!({"repo_id": form.repo, "target_branch": form.target, "title": form.title.trim(), "body": form.body, "draft": form.draft}),
            "PR 已创建。",
        ),
    };
    spawn(async move {
        let path = format!("/api/workspaces/{}/{endpoint}", form.workspace);
        let result = api
            .request_with_error::<Value, Value>(method, &path, Some(json_bytes(&payload)))
            .await;
        let has_conflict = result
            .as_ref()
            .err()
            .and_then(|error| error.data.as_ref())
            .is_some_and(|data| {
                matches!(
                    data["type"].as_str(),
                    Some("merge_conflicts" | "rebase_in_progress")
                )
            });
        if result.is_ok() || has_conflict {
            state.git_form.set(None);
        }
        state.refresh_git();
        state.finish(
            result
                .map(|response| {
                    if form.kind == GitFormKind::PullRequest {
                        state.returned_url.set(response.as_str().map(str::to_owned));
                    }
                    success.into()
                })
                .map_err(action_error),
        );
    });
}

#[derive(Clone, Copy)]
enum GitAction {
    Merge,
    Abort,
    Continue,
}

fn git_action(api: Rc<Api>, state: State, id: String, action: GitAction) {
    let Some(workspace) = state.selected_row() else {
        return;
    };
    let Some(git) = state.selected_status(&id) else {
        return;
    };
    if !state.begin() {
        return;
    }
    let confirmed_target = git.target_branch_name.clone();
    let (endpoint, label, description, success) = match action {
        GitAction::Merge => (
            "merge",
            "合并分支",
            format!(
                "将 {} 合并到本地分支 {}？",
                workspace.branch, git.target_branch_name
            ),
            "已合并到目标分支。",
        ),
        GitAction::Abort => (
            "conflicts/abort",
            "中止冲突操作",
            format!(
                "中止 {} 中的 Git 操作，丢弃当前冲突解决进度？",
                git.repo_name
            ),
            "已中止冲突操作。",
        ),
        GitAction::Continue => (
            "rebase/continue",
            "继续 Rebase",
            format!("确认已在 {} 中解决并暂存全部冲突文件。", git.repo_name),
            "已继续 Rebase。",
        ),
    };
    let confirmation = nsh_base::alert_dialog()
        .title(label)
        .description(description)
        .cancel("cancel", "取消")
        .action("confirm", label)
        .close_on_escape(true)
        .show();
    spawn(async move {
        match confirmation.await {
            Ok(answer) if answer.action.as_deref() == Some("confirm") => {}
            Ok(_) => {
                state.busy.set(false);
                return;
            }
            Err(error) => {
                state.finish(Err(error));
                return;
            }
        }
        let base = format!("/api/workspaces/{}/git", workspace.id);
        let result = async {
            let latest: Vec<GitStatus> = get(&api, &format!("{base}/status")).await?;
            let git = latest
                .iter()
                .find(|repo| repo.repo_id == id)
                .ok_or("仓库已不在工作区。")?;
            if matches!(action, GitAction::Merge)
                && let Some(reason) = git.merge_block()
            {
                return Err(reason.into());
            }
            if matches!(action, GitAction::Merge) && git.target_branch_name != confirmed_target {
                return Err("目标分支已变化，请重新确认合并。".into());
            }
            if matches!(action, GitAction::Continue) && !git.is_rebase_in_progress {
                return Err("当前没有进行中的 Rebase。".into());
            }
            if matches!(action, GitAction::Abort) && !git.conflicted() {
                return Err("当前没有可中止的冲突操作。".into());
            }
            api.request_with_error::<(), Value>(
                "POST",
                &format!("{base}/{endpoint}"),
                Some(json_bytes(&json!({"repo_id": id}))),
            )
            .await
            .map_err(action_error)?;
            Ok(success.into())
        }
        .await;
        if result.is_ok() && matches!(action, GitAction::Merge) {
            state.revision.update(|revision| *revision += 1);
        }
        state.refresh_git();
        state.finish(result);
    });
}
