use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::{env, process::Command};

use gpui::*;
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants};
use gpui_component::{h_flex, v_flex, Sizable};
use rfd::FileDialog;

use crate::ai::AiClient;
use crate::git::GitClient;
use crate::models::{
    AiProvider, AppSettings, CommitSuggestion, DiffEntry, GitIdentity, RemoteModelOption,
    RepoSnapshot,
};
use crate::storage::{push_recent_repo, save_settings};
use crate::ui::domain_state::{CommitState, NetworkAction, NetworkState, RepoState, SelectionState};
use crate::ui::theme;
use crate::ui::ui_state::{
    FilterState, MessageState, NavState, OpenRouterModelsState, SidebarTab,
};

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum RepoRefreshReason {
    Manual,
    #[allow(dead_code)]
    Focus,
    Watch,
}

enum AppEvent {
    RepoLoaded(Result<RepoSnapshot, String>),
    RepoRefreshed(PathBuf, Result<RepoSnapshot, String>, RepoRefreshReason),
    BranchSwitched(Result<RepoSnapshot, String>, String),
    BranchMerged(Result<RepoSnapshot, String>, String),
    CommitCreated(Result<RepoSnapshot, String>),
    NetworkActionCompleted(Result<RepoSnapshot, String>, String),
    AiCommitGenerated(Result<CommitSuggestion, String>),
    OpenRouterModelsLoaded(Result<Vec<RemoteModelOption>, String>),
    CommitDiffLoaded(String, Result<Vec<DiffEntry>, String>),
}

// ---------------------------------------------------------------------------
// Actions (dispatched by child views via gpui action system)
// ---------------------------------------------------------------------------

// Toolbar
#[derive(Clone)]
pub enum ToolbarAction {
    ToggleRepoSelector,
    SwitchBranch(String),
    RunNetworkAction(NetworkAction),
    FetchOrigin,
    PullOrigin,
    PushOrigin,
}

// Sidebar
#[derive(Clone)]
pub enum SidebarAction {
    OpenRepoDialog,
    OpenRepo(PathBuf),
    HideRepoSelector,
    SelectChange(String),
    DiscardChange(String),
    IgnorePath(String),
    IgnoreExtension(String),
    CopyFullPath(String),
    CopyRelativePath(String),
    RevealInFinder(String),
    OpenInEditor(String),
    OpenWithDefault(String),
    SelectCommit(String),
    GenerateAiCommit,
    ShowSettings,
    CommitAll,
}

// Settings
#[derive(Clone)]
pub enum SettingsAction {
    SaveGitConfig,
    SaveAiSettings,
    ChangeProvider(AiProvider),
    SelectOpenRouterModel(String),
    RetryOpenRouterModels,
    Close,
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

/// Sender wrapper that sets an atomic flag before sending,
/// so the poll timer can skip acquiring the app lock when idle.
#[derive(Clone)]
struct NotifySender {
    tx: Sender<AppEvent>,
    pending: Arc<AtomicBool>,
}

impl NotifySender {
    fn send(&self, event: AppEvent) {
        self.pending.store(true, Ordering::Release);
        let _ = self.tx.send(event);
    }
}

pub struct GitSparkApp {
    git: GitClient,
    pub settings: AppSettings,
    pub repo: RepoState,
    pub commit: CommitState,
    pub network: NetworkState,
    pub selection: SelectionState,
    pub nav: NavState,
    pub filters: FilterState,
    pub messages: MessageState,
    repo_watch_generation: Arc<AtomicU64>,
    watched_repo_path: Option<PathBuf>,
    event_tx: NotifySender,
    event_rx: Receiver<AppEvent>,
    // Text input state
    summary_focus: FocusHandle,
    description_focus: FocusHandle,
    summary_cursor: usize,
    description_cursor: usize,
}

impl GitSparkApp {
    pub fn new(settings: AppSettings, cx: &mut Context<Self>) -> Self {
        let (tx, event_rx) = mpsc::channel();
        let event_pending = Arc::new(AtomicBool::new(false));
        let event_tx = NotifySender {
            tx,
            pending: Arc::clone(&event_pending),
        };

        let error_message = String::new();

        let mut app = Self {
            git: GitClient::new(),
            settings: settings.clone(),
            repo: RepoState::default(),
            commit: CommitState::default(),
            network: NetworkState::default(),
            selection: SelectionState::default(),
            nav: NavState::default(),
            filters: FilterState::default(),
            messages: MessageState::new("Open a repository to get started.", error_message),
            repo_watch_generation: Arc::new(AtomicU64::new(0)),
            watched_repo_path: None,
            event_tx,
            event_rx,
            summary_focus: cx.focus_handle(),
            description_focus: cx.focus_handle(),
            summary_cursor: 0,
            description_cursor: 0,
        };

        if let Some(last_repo) = settings.recent_repos.first() {
            app.open_repo(last_repo.clone());
        }

        // Poll loop: only acquires the app lock when the atomic flag
        // indicates events are pending. Idle polls are lock-free.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(32)).await;
                if !event_pending.load(Ordering::Acquire) {
                    continue;
                }
                let _ = cx.update(|cx| {
                    let _ = this.update(cx, |app, cx| {
                        app.process_events(cx);
                    });
                });
            }
        }).detach();

        app
    }

    // ------------------------------------------------------------------
    // Event processing — drain the mpsc channel
    // ------------------------------------------------------------------

    fn process_events(&mut self, cx: &mut Context<Self>) {
        self.event_tx.pending.store(false, Ordering::Release);
        let mut had_events = false;
        while let Ok(event) = self.event_rx.try_recv() {
            had_events = true;
            match event {
                AppEvent::RepoLoaded(Ok(snapshot)) => {
                    self.adopt_snapshot(snapshot);
                    self.messages.status_message = "Repository loaded.".to_string();
                    self.messages.error_message.clear();
                }
                AppEvent::RepoLoaded(Err(err)) => {
                    self.messages.error_message =
                        format!("Failed to open repository: {err}");
                }
                AppEvent::RepoRefreshed(path, Ok(snapshot), reason) => {
                    let should_apply = self
                        .repo_path()
                        .map(PathBuf::from)
                        .map(|current_path| current_path == path)
                        .unwrap_or(false);
                    if !should_apply {
                        continue;
                    }
                    self.adopt_snapshot(snapshot);
                    if reason == RepoRefreshReason::Manual {
                        self.messages.status_message = "Repository refreshed.".to_string();
                    }
                    self.messages.error_message.clear();
                }
                AppEvent::RepoRefreshed(path, Err(err), reason) => {
                    let should_apply = self
                        .repo_path()
                        .map(PathBuf::from)
                        .map(|current_path| current_path == path)
                        .unwrap_or(false);
                    if !should_apply {
                        continue;
                    }
                    if reason == RepoRefreshReason::Manual {
                        self.messages.error_message = format!("Refresh failed: {err}");
                    } else {
                        self.messages.error_message = err;
                    }
                }
                AppEvent::BranchSwitched(Ok(snapshot), branch) => {
                    self.adopt_snapshot(snapshot);
                    self.messages.status_message =
                        format!("Switched to branch '{branch}'.");
                    self.messages.error_message.clear();
                }
                AppEvent::BranchSwitched(Err(err), _) => {
                    self.messages.error_message =
                        format!("Branch switch failed: {err}");
                }
                AppEvent::BranchMerged(Ok(snapshot), branch) => {
                    self.adopt_snapshot(snapshot);
                    self.messages.status_message = format!("Merged '{branch}'.");
                    self.messages.error_message.clear();
                }
                AppEvent::BranchMerged(Err(err), _) => {
                    self.messages.error_message = format!("Merge failed: {err}");
                }
                AppEvent::CommitCreated(Ok(snapshot)) => {
                    self.adopt_snapshot(snapshot);
                    self.commit.summary.clear();
                    self.commit.body.clear();
                    self.summary_cursor = 0;
                    self.description_cursor = 0;
                    self.commit.ai_preview = None;
                    self.messages.status_message = "Commit created.".to_string();
                    self.messages.error_message.clear();
                }
                AppEvent::CommitCreated(Err(err)) => {
                    self.messages.error_message = format!("Commit failed: {err}");
                }
                AppEvent::NetworkActionCompleted(Ok(snapshot), action_label) => {
                    self.network.active_action = None;
                    self.adopt_snapshot(snapshot);
                    self.messages.status_message =
                        format!("{action_label} complete.");
                    self.messages.error_message.clear();
                }
                AppEvent::NetworkActionCompleted(Err(err), action_label) => {
                    self.network.active_action = None;
                    self.messages.error_message =
                        format!("{action_label} failed: {err}");
                }
                AppEvent::AiCommitGenerated(Ok(suggestion)) => {
                    self.commit.ai_in_flight = false;
                    self.commit.summary = suggestion.subject.clone();
                    self.commit.body = suggestion.body.clone();
                    self.summary_cursor = self.commit.summary.len();
                    self.description_cursor = self.commit.body.len();
                    self.commit.ai_preview = Some(suggestion);
                    self.messages.status_message =
                        "Generated commit suggestion.".to_string();
                    self.messages.error_message.clear();
                }
                AppEvent::AiCommitGenerated(Err(err)) => {
                    self.commit.ai_in_flight = false;
                    self.messages.error_message =
                        format!("AI generation failed: {err}");
                }
                AppEvent::OpenRouterModelsLoaded(Ok(models)) => {
                    if self.settings.ai.provider == AiProvider::OpenRouter
                        && self.settings.ai.model.trim().is_empty()
                    {
                        if let Some(first) = models.first() {
                            self.settings.ai.model = first.id.clone();
                        }
                    }
                    self.filters.openrouter_models =
                        OpenRouterModelsState::Ready(models);
                }
                AppEvent::OpenRouterModelsLoaded(Err(err)) => {
                    self.filters.openrouter_models =
                        OpenRouterModelsState::Error(err);
                }
                AppEvent::CommitDiffLoaded(oid, Ok(diffs)) => {
                    if self.selection.selected_commit.as_deref() == Some(oid.as_str()) {
                        if let Some(first) = diffs.first() {
                            self.selection.selected_commit_file =
                                Some(first.path.clone());
                        }
                        self.selection.commit_diffs = Some(diffs);
                    }
                }
                AppEvent::CommitDiffLoaded(_, Err(err)) => {
                    self.messages.error_message =
                        format!("Failed to load commit details: {err}");
                }
            }
        }
        // Only trigger a re-render if we actually processed events.
        if had_events {
            cx.notify();
        }
    }

    // ------------------------------------------------------------------
    // Toolbar action handler
    // ------------------------------------------------------------------

    pub fn handle_toolbar_action(&mut self, action: ToolbarAction, cx: &mut Context<Self>) {
        match action {
            ToolbarAction::ToggleRepoSelector => {
                self.nav.show_repo_selector = !self.nav.show_repo_selector;
            }
            ToolbarAction::SwitchBranch(name) => {
                self.repo.branch_target = name;
                self.switch_branch(cx);
            }
            ToolbarAction::RunNetworkAction(net_action) => {
                self.run_network_action(net_action, cx);
            }
            ToolbarAction::FetchOrigin => self.fetch_origin(cx),
            ToolbarAction::PullOrigin => self.pull_origin(cx),
            ToolbarAction::PushOrigin => self.push_origin(cx),
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Sidebar action handler
    // ------------------------------------------------------------------

    pub fn handle_sidebar_action(&mut self, action: SidebarAction, cx: &mut Context<Self>) {
        match action {
            SidebarAction::OpenRepoDialog => self.open_repo_dialog(cx),
            SidebarAction::OpenRepo(path) => self.open_repo_with_notify(path, cx),
            SidebarAction::HideRepoSelector => self.nav.show_repo_selector = false,
            SidebarAction::SelectChange(path) => {
                self.selection.selected_change = Some(path);
            }
            SidebarAction::DiscardChange(path) => self.discard_change(&path),
            SidebarAction::IgnorePath(path) => self.ignore_path(&path),
            SidebarAction::IgnoreExtension(ext) => self.ignore_extension(&ext),
            SidebarAction::CopyFullPath(path) => {
                if let Some(repo_path) = self.repo_path() {
                    let full_path = repo_path.join(&path);
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        full_path.to_string_lossy().to_string(),
                    ));
                    self.messages.status_message =
                        format!("Copied absolute path for '{path}'.");
                    self.messages.error_message.clear();
                }
            }
            SidebarAction::CopyRelativePath(path) => {
                cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                self.messages.status_message =
                    format!("Copied relative path for '{path}'.");
                self.messages.error_message.clear();
            }
            SidebarAction::RevealInFinder(path) => self.reveal_in_finder(&path),
            SidebarAction::OpenInEditor(path) => self.open_in_external_editor(&path),
            SidebarAction::OpenWithDefault(path) => {
                if let Some(repo_path) = self.repo_path() {
                    let full_path = repo_path.join(&path);
                    match open::that(&full_path) {
                        Ok(_) => {
                            self.messages.status_message =
                                format!("Opened '{path}' with the default program.");
                            self.messages.error_message.clear();
                        }
                        Err(err) => {
                            self.messages.error_message = format!(
                                "Failed to open '{path}' with default program: {err}"
                            );
                        }
                    }
                }
            }
            SidebarAction::SelectCommit(oid) => self.select_commit(oid, cx),
            SidebarAction::GenerateAiCommit => self.generate_ai_commit(cx),
            SidebarAction::ShowSettings => self.nav.show_settings = true,
            SidebarAction::CommitAll => self.commit_all(cx),
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Settings action handler
    // ------------------------------------------------------------------

    pub fn handle_settings_action(&mut self, action: SettingsAction, cx: &mut Context<Self>) {
        match action {
            SettingsAction::SaveGitConfig => self.save_git_config(),
            SettingsAction::SaveAiSettings => {
                self.settings.ai.endpoint =
                    self.settings.ai.provider.default_endpoint().to_string();
                self.persist_settings();
                if self.messages.error_message.is_empty() {
                    self.messages.status_message = "AI settings saved.".to_string();
                }
            }
            SettingsAction::ChangeProvider(provider) => {
                self.settings.ai.provider = provider;
                self.settings.ai.endpoint =
                    self.settings.ai.provider.default_endpoint().to_string();
                self.filters.openrouter_model_filter.clear();
                if self.settings.ai.provider == AiProvider::OpenRouter {
                    self.ensure_openrouter_models(cx);
                }
            }
            SettingsAction::SelectOpenRouterModel(model_id) => {
                self.settings.ai.model = model_id;
            }
            SettingsAction::RetryOpenRouterModels => {
                self.filters.openrouter_models = OpenRouterModelsState::Idle;
                self.ensure_openrouter_models(cx);
            }
            SettingsAction::Close => {
                self.nav.show_settings = false;
            }
        }
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Repository operations
    // ------------------------------------------------------------------

    fn open_repo_dialog(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = FileDialog::new().pick_folder() {
            self.open_repo_with_notify(path, cx);
        }
    }

    fn open_repo_with_notify(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.open_repo(path);
        cx.notify();
    }

    fn open_repo(&mut self, path: PathBuf) {
        self.messages.status_message = "Loading repository...".to_string();
        self.messages.error_message.clear();
        self.nav.show_repo_selector = false;
        self.stop_repo_watch();
        self.add_recent_repo(path.clone());
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.open_repo(path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoLoaded(res));
        });
    }

    pub fn refresh_repo(&mut self, cx: &mut Context<Self>) {
        self.request_repo_refresh(RepoRefreshReason::Manual, cx);
    }

    fn request_repo_refresh(&mut self, reason: RepoRefreshReason, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        if reason == RepoRefreshReason::Manual {
            self.messages.status_message = "Refreshing repository...".to_string();
        }
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        let event_path = path.clone();
        thread::spawn(move || {
            let res = git.refresh_repo(&path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoRefreshed(event_path, res, reason));
        });
        cx.notify();
    }

    fn stop_repo_watch(&mut self) {
        self.repo_watch_generation.fetch_add(1, Ordering::SeqCst);
        self.watched_repo_path = None;
    }

    fn ensure_repo_watch(&mut self, repo_path: &Path) {
        if self.watched_repo_path.as_deref() == Some(repo_path) {
            return;
        }

        let path = repo_path.to_path_buf();
        let token = self.repo_watch_generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.watched_repo_path = Some(path.clone());

        let generation = Arc::clone(&self.repo_watch_generation);
        let tx = self.event_tx.clone();

        thread::spawn(move || {
            let git = GitClient::new();
            let mut last_fingerprint = git.read_watch_fingerprint(&path).ok();

            while generation.load(Ordering::SeqCst) == token {
                thread::sleep(Duration::from_millis(3000));

                if generation.load(Ordering::SeqCst) != token {
                    break;
                }

                let Ok(current_fingerprint) = git.read_watch_fingerprint(&path) else {
                    continue;
                };

                let changed = match &last_fingerprint {
                    Some(previous) => previous != &current_fingerprint,
                    None => true,
                };

                if !changed {
                    continue;
                }

                last_fingerprint = Some(current_fingerprint);
                let res = git.refresh_repo(&path).map_err(|e| e.to_string());
                let _ = tx.send(AppEvent::RepoRefreshed(
                    path.clone(),
                    res,
                    RepoRefreshReason::Watch,
                ));
            }
        });
    }

    // ------------------------------------------------------------------
    // Network operations
    // ------------------------------------------------------------------

    fn fetch_origin(&mut self, cx: &mut Context<Self>) {
        self.run_network_action(NetworkAction::Fetch, cx);
    }

    fn pull_origin(&mut self, cx: &mut Context<Self>) {
        self.run_network_action(NetworkAction::Pull, cx);
    }

    fn push_origin(&mut self, cx: &mut Context<Self>) {
        self.run_network_action(NetworkAction::Push, cx);
    }

    fn run_network_action(&mut self, action: NetworkAction, cx: &mut Context<Self>) {
        if self.network.active_action.is_some() {
            return;
        }

        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let remote_name = self
            .repo
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.repo.remote_name.clone())
            .unwrap_or_else(|| "origin".to_string());
        let action_label = action.title(&remote_name);

        self.messages.status_message = format!("{action_label}...");
        self.messages.error_message.clear();
        self.network.active_action = Some(action);

        let tx = self.event_tx.clone();
        let git = GitClient::new();
        let action_label_for_event = action_label.clone();

        thread::spawn(move || {
            let res = match action {
                NetworkAction::Fetch => git.fetch_origin(&path),
                NetworkAction::Pull => git.pull_origin(&path),
                NetworkAction::Push => git.push_origin(&path),
            }
            .map_err(|e| e.to_string());

            let _ = tx.send(AppEvent::NetworkActionCompleted(
                res,
                action_label_for_event,
            ));
        });
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Branch operations
    // ------------------------------------------------------------------

    fn switch_branch(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let target = self.repo.branch_target.trim().to_string();
        if target.is_empty() {
            self.messages.error_message = "Choose a branch first.".to_string();
            return;
        }

        self.messages.status_message = format!("Switching to '{}'...", target);
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git
                .switch_branch(&path, &target)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::BranchSwitched(res, target));
        });
        cx.notify();
    }

    pub fn merge_branch(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let target = self.repo.merge_target.trim().to_string();
        if target.is_empty() {
            self.messages.error_message = "Choose a branch to merge.".to_string();
            return;
        }

        self.messages.status_message = format!("Merging '{}'...", target);
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git
                .merge_branch(&path, &target)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::BranchMerged(res, target));
        });
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Commit operations
    // ------------------------------------------------------------------

    fn commit_all(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        if self.commit.summary.trim().is_empty() {
            self.messages.error_message = "Commit summary cannot be empty.".to_string();
            return;
        }

        let message = if self.commit.body.trim().is_empty() {
            self.commit.summary.trim().to_string()
        } else {
            format!(
                "{}\n\n{}",
                self.commit.summary.trim(),
                self.commit.body.trim()
            )
        };

        self.messages.status_message = "Creating commit...".to_string();
        self.messages.error_message.clear();
        let tx = self.event_tx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.commit_all(&path, &message).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::CommitCreated(res));
        });
        cx.notify();
    }

    // ------------------------------------------------------------------
    // AI commit generation
    // ------------------------------------------------------------------

    fn generate_ai_commit(&mut self, cx: &mut Context<Self>) {
        if self.commit.ai_in_flight {
            return;
        }

        let Some(snapshot) = &self.repo.snapshot else {
            self.messages.error_message =
                "Open a repository before generating a commit message.".to_string();
            return;
        };

        let diff = snapshot
            .diffs
            .iter()
            .filter(|entry| !entry.is_binary)
            .map(|entry| format!("FILE: {}\n{}", entry.path, entry.diff))
            .collect::<Vec<_>>()
            .join("\n\n");

        if diff.trim().is_empty() {
            self.messages.error_message =
                "No text diff available for AI commit generation.".to_string();
            return;
        }

        self.messages.status_message = "Generating AI commit suggestion...".to_string();
        self.messages.error_message.clear();
        self.commit.ai_in_flight = true;
        let tx = self.event_tx.clone();
        let ai = AiClient::new();
        let settings = self.settings.ai.clone();
        thread::spawn(move || {
            let res = ai
                .generate_commit_message(&settings, &diff)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::AiCommitGenerated(res));
        });
        cx.notify();
    }

    fn ensure_openrouter_models(&mut self, cx: &mut Context<Self>) {
        if self.settings.ai.provider != AiProvider::OpenRouter {
            return;
        }

        match self.filters.openrouter_models {
            OpenRouterModelsState::Idle | OpenRouterModelsState::Error(_) => {}
            OpenRouterModelsState::Loading | OpenRouterModelsState::Ready(_) => return,
        }

        self.filters.openrouter_models = OpenRouterModelsState::Loading;
        let tx = self.event_tx.clone();
        let ai = AiClient::new();
        thread::spawn(move || {
            let res = ai.fetch_openrouter_models().map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::OpenRouterModelsLoaded(res));
        });
        cx.notify();
    }

    // ------------------------------------------------------------------
    // Git config
    // ------------------------------------------------------------------

    fn save_git_config(&mut self) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        match self.git.write_identity(&path, &self.repo.identity) {
            Ok(()) => {
                self.messages.status_message = "Git config saved.".to_string();
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to save git config: {err}");
            }
        }
    }

    fn load_identity(&mut self, path: &Path) {
        match self.git.read_identity(path) {
            Ok(identity) => {
                self.repo.identity = identity;
            }
            Err(err) => {
                self.repo.identity = GitIdentity::default();
                self.messages.error_message =
                    format!("Could not load git config: {err}");
            }
        }
    }

    // ------------------------------------------------------------------
    // Commit diff / selection
    // ------------------------------------------------------------------

    fn load_commit_diff(&mut self, oid: String) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            return;
        };

        let tx = self.event_tx.clone();
        let git = GitClient::new();

        thread::spawn(move || {
            let res = git.get_commit_diff(&path, &oid).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::CommitDiffLoaded(oid, res));
        });
    }

    pub fn select_commit(&mut self, oid: String, cx: &mut Context<Self>) {
        let already_selected =
            self.selection.selected_commit.as_deref() == Some(oid.as_str());
        if already_selected && self.selection.commit_diffs.is_some() {
            return;
        }

        self.selection.selected_commit = Some(oid.clone());
        self.selection.selected_commit_file = None;
        self.selection.commit_diffs = None;
        self.load_commit_diff(oid);
        cx.notify();
    }

    // ------------------------------------------------------------------
    // File operations
    // ------------------------------------------------------------------

    fn discard_change(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        match self.git.discard_change(&repo_path, relative_path) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.messages.status_message =
                    format!("Discarded changes for '{}'.", relative_path);
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message = format!(
                    "Failed to discard changes for '{}': {err}",
                    relative_path
                );
            }
        }
    }

    fn ignore_path(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let pattern = relative_path.replace('\\', "/");
        match self.git.append_gitignore_pattern(&repo_path, &pattern) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.messages.status_message =
                    format!("Added '{}' to .gitignore.", relative_path);
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to ignore '{}': {err}", relative_path);
            }
        }
    }

    fn ignore_extension(&mut self, ext: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let pattern = format!("*.{ext}");
        match self.git.append_gitignore_pattern(&repo_path, &pattern) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.messages.status_message =
                    format!("Added '{}' to .gitignore.", pattern);
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to ignore '{}': {err}", pattern);
            }
        }
    }

    fn reveal_in_finder(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };
        let full_path = repo_path.join(relative_path);

        #[cfg(target_os = "macos")]
        let result = Command::new("open")
            .arg("-R")
            .arg(&full_path)
            .spawn()
            .map(|_| ());

        #[cfg(not(target_os = "macos"))]
        let result = open::that_detached(&full_path);

        match result {
            Ok(_) => {
                self.messages.status_message =
                    format!("Revealed '{}' in Finder.", relative_path);
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message =
                    format!("Failed to reveal '{}': {err}", relative_path);
            }
        }
    }

    fn open_in_external_editor(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.messages.error_message = "No repository selected.".to_string();
            return;
        };

        let full_path = repo_path.join(relative_path);
        let configured_editor = self
            .git
            .read_config_value(&repo_path, "core.editor")
            .ok()
            .flatten()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                env::var("VISUAL")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            })
            .or_else(|| {
                env::var("EDITOR")
                    .ok()
                    .filter(|value| !value.trim().is_empty())
            });

        let result = if let Some(editor_cmd) = configured_editor {
            Command::new("sh")
                .arg("-lc")
                .arg(format!(
                    "{} {}",
                    editor_cmd,
                    shell_escape(&full_path.to_string_lossy())
                ))
                .spawn()
                .map(|_| ())
        } else {
            open::that_detached(&full_path)
        };

        match result {
            Ok(_) => {
                self.messages.status_message =
                    format!("Opened '{}' in external editor.", relative_path);
                self.messages.error_message.clear();
            }
            Err(err) => {
                self.messages.error_message = format!(
                    "Failed to open '{}' in external editor: {err}",
                    relative_path
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // Settings persistence
    // ------------------------------------------------------------------

    fn add_recent_repo(&mut self, path: PathBuf) {
        push_recent_repo(&mut self.settings, path);
        self.persist_settings();
    }

    fn persist_settings(&mut self) {
        if let Err(err) = save_settings(&self.settings) {
            self.messages.error_message =
                format!("Failed to save settings: {err}");
        }
    }

    // ------------------------------------------------------------------
    // Snapshot adoption
    // ------------------------------------------------------------------

    fn adopt_snapshot(&mut self, snapshot: RepoSnapshot) {
        let previous_commit = self.selection.selected_commit.clone();
        let current_branch = snapshot.repo.current_branch.clone();
        self.selection.selected_change =
            snapshot.changes.first().map(|change| change.path.clone());
        self.repo.branch_target = current_branch;
        self.repo.merge_target = snapshot
            .branches
            .iter()
            .find(|branch| !branch.is_current && !branch.is_remote)
            .map(|branch| branch.name.clone())
            .unwrap_or_default();
        self.load_identity(&snapshot.repo.path);
        self.ensure_repo_watch(&snapshot.repo.path);
        self.repo.snapshot = Some(snapshot);

        let next_selected_commit = self.repo.snapshot.as_ref().and_then(|repo| {
            previous_commit
                .filter(|oid| repo.history.iter().any(|commit| commit.oid == *oid))
                .or_else(|| repo.history.first().map(|commit| commit.oid.clone()))
        });

        self.selection.selected_commit = next_selected_commit.clone();
        self.selection.selected_commit_file = None;
        self.selection.commit_diffs = None;

        if let Some(oid) = next_selected_commit {
            self.load_commit_diff(oid);
        }
    }

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    pub fn repo_path(&self) -> Option<&Path> {
        self.repo
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.repo.path.as_path())
    }

    pub fn selected_diff(&self) -> Option<&DiffEntry> {
        let snapshot = self.repo.snapshot.as_ref()?;
        let selected_change = self.selection.selected_change.as_ref()?;
        snapshot
            .diffs
            .iter()
            .find(|diff| &diff.path == selected_change)
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl Render for GitSparkApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Clamp cursors to valid positions (e.g. after AI fill or clear)
        self.summary_cursor = self.summary_cursor.min(self.commit.summary.len());
        self.description_cursor = self.description_cursor.min(self.commit.body.len());

        let summary_focused = self.summary_focus.is_focused(window);
        let description_focused = self.description_focus.is_focused(window);

        let mut root = v_flex()
            .size_full()
            .bg(theme::bg())
            .child(self.render_toolbar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .items_start()
                    .overflow_hidden()
                    .child(self.render_sidebar(summary_focused, description_focused, cx))
                    .child(self.render_workspace(cx)),
            )
            .child(self.render_status_bar());

        if self.nav.show_repo_selector {
            root = root.child(self.render_repo_selector_overlay(cx));
        }

        if self.nav.show_settings {
            root = root.child(self.render_settings_overlay(cx));
        }

        root
    }
}

impl GitSparkApp {
    // ------------------------------------------------------------------
    // Toolbar
    // ------------------------------------------------------------------

    fn render_toolbar(&self, cx: &mut Context<Self>) -> Div {
        use crate::ui::toolbar;

        let snapshot = self.repo.snapshot.as_ref();
        let repo_name = snapshot
            .map(|s| s.repo.name.as_str())
            .unwrap_or("Choose repository");
        let branch_name = snapshot
            .map(|s| s.repo.current_branch.as_str())
            .unwrap_or("No branch");
        let ahead = snapshot.map(|s| s.repo.ahead).unwrap_or(0);
        let behind = snapshot.map(|s| s.repo.behind).unwrap_or(0);

        let network_action = snapshot
            .map(|s| NetworkAction::from_snapshot(s))
            .unwrap_or(NetworkAction::Fetch);
        let remote_name = snapshot
            .and_then(|s| s.repo.remote_name.as_deref())
            .unwrap_or("origin");
        let network_label = if self.network.active_action.is_some() {
            network_action.pending_title(remote_name)
        } else {
            network_action.title(remote_name)
        };
        let last_fetched = snapshot.and_then(|s| s.repo.last_fetched.as_deref());

        // Repo section — click toggles repo selector
        let repo_section = toolbar::render_repo_section(repo_name)
            .on_click(cx.listener(|app, _evt, _win, cx| {
                app.handle_toolbar_action(ToolbarAction::ToggleRepoSelector, cx);
            }));

        // Branch section — placeholder click (no dropdown yet)
        let branch_section = toolbar::render_branch_section(branch_name);

        // Network section — click runs the primary network action
        let net_action = network_action;
        let network_section = toolbar::render_network_section(
            &network_label,
            ahead,
            behind,
            last_fetched,
        )
        .on_click(cx.listener(move |app, _evt, _win, cx| {
            app.handle_toolbar_action(ToolbarAction::RunNetworkAction(net_action), cx);
        }));

        h_flex()
            .w_full()
            .h(px(theme::TOOLBAR_HEIGHT))
            .flex_shrink_0()
            .bg(theme::toolbar_bg())
            .border_b_1()
            .border_color(theme::toolbar_button_border())
            .child(repo_section)
            .child(toolbar::vertical_divider())
            .child(branch_section)
            .child(toolbar::vertical_divider())
            .child(network_section)
    }

    // ------------------------------------------------------------------
    // Sidebar
    // ------------------------------------------------------------------

    fn render_sidebar(
        &self,
        summary_focused: bool,
        description_focused: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity().clone();
        let sidebar_tab = self.nav.sidebar_tab;

        let mut sidebar = crate::ui::sidebar::render_sidebar_interactive(self, view, cx);

        // Commit form with interactive handlers (only on Changes tab)
        if sidebar_tab == SidebarTab::Changes {
            let branch_name = self
                .repo
                .snapshot
                .as_ref()
                .map(|s| s.repo.current_branch.clone())
                .unwrap_or_else(|| "main".to_string());
            sidebar = sidebar.child(self.render_commit_form_interactive(
                &branch_name,
                summary_focused,
                description_focused,
                cx,
            ));
        }

        sidebar
    }

    // ------------------------------------------------------------------
    // Text input key handling
    // ------------------------------------------------------------------

    fn handle_summary_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        if ks.modifiers.secondary() {
            match ks.key.as_str() {
                "v" => {
                    if let Some(item) = cx.read_from_clipboard() {
                        if let Some(text) = item.text() {
                            let text = text.replace('\n', " ");
                            self.commit.summary.insert_str(self.summary_cursor, &text);
                            self.summary_cursor += text.len();
                            cx.notify();
                        }
                    }
                }
                "a" => {
                    self.summary_cursor = self.commit.summary.len();
                    cx.notify();
                }
                _ => {}
            }
            return;
        }
        match ks.key.as_str() {
            "backspace" => {
                if self.summary_cursor > 0 {
                    let new_pos = prev_char_boundary(&self.commit.summary, self.summary_cursor);
                    self.commit.summary.drain(new_pos..self.summary_cursor);
                    self.summary_cursor = new_pos;
                    cx.notify();
                }
            }
            "delete" => {
                if self.summary_cursor < self.commit.summary.len() {
                    let end = next_char_boundary(&self.commit.summary, self.summary_cursor);
                    self.commit.summary.drain(self.summary_cursor..end);
                    cx.notify();
                }
            }
            "left" => {
                if self.summary_cursor > 0 {
                    self.summary_cursor =
                        prev_char_boundary(&self.commit.summary, self.summary_cursor);
                    cx.notify();
                }
            }
            "right" => {
                if self.summary_cursor < self.commit.summary.len() {
                    self.summary_cursor =
                        next_char_boundary(&self.commit.summary, self.summary_cursor);
                    cx.notify();
                }
            }
            "home" => {
                self.summary_cursor = 0;
                cx.notify();
            }
            "end" => {
                self.summary_cursor = self.commit.summary.len();
                cx.notify();
            }
            _ => {
                if let Some(ref ch) = ks.key_char {
                    if !ks.modifiers.control && !ch.contains('\n') && !ch.contains('\r') {
                        self.commit.summary.insert_str(self.summary_cursor, ch);
                        self.summary_cursor += ch.len();
                        cx.notify();
                    }
                }
            }
        }
    }

    fn handle_description_key(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ks = &event.keystroke;
        if ks.modifiers.secondary() {
            match ks.key.as_str() {
                "v" => {
                    if let Some(item) = cx.read_from_clipboard() {
                        if let Some(text) = item.text() {
                            self.commit.body.insert_str(self.description_cursor, &text);
                            self.description_cursor += text.len();
                            cx.notify();
                        }
                    }
                }
                "a" => {
                    self.description_cursor = self.commit.body.len();
                    cx.notify();
                }
                _ => {}
            }
            return;
        }
        match ks.key.as_str() {
            "backspace" => {
                if self.description_cursor > 0 {
                    let new_pos =
                        prev_char_boundary(&self.commit.body, self.description_cursor);
                    self.commit.body.drain(new_pos..self.description_cursor);
                    self.description_cursor = new_pos;
                    cx.notify();
                }
            }
            "delete" => {
                if self.description_cursor < self.commit.body.len() {
                    let end = next_char_boundary(&self.commit.body, self.description_cursor);
                    self.commit.body.drain(self.description_cursor..end);
                    cx.notify();
                }
            }
            "left" => {
                if self.description_cursor > 0 {
                    self.description_cursor =
                        prev_char_boundary(&self.commit.body, self.description_cursor);
                    cx.notify();
                }
            }
            "right" => {
                if self.description_cursor < self.commit.body.len() {
                    self.description_cursor =
                        next_char_boundary(&self.commit.body, self.description_cursor);
                    cx.notify();
                }
            }
            "home" => {
                self.description_cursor = 0;
                cx.notify();
            }
            "end" => {
                self.description_cursor = self.commit.body.len();
                cx.notify();
            }
            "enter" => {
                self.commit.body.insert_str(self.description_cursor, "\n");
                self.description_cursor += 1;
                cx.notify();
            }
            _ => {
                if let Some(ref ch) = ks.key_char {
                    if !ks.modifiers.control {
                        self.commit.body.insert_str(self.description_cursor, ch);
                        self.description_cursor += ch.len();
                        cx.notify();
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Text field renderer
    // ------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn render_text_field(
        &self,
        id: &str,
        value: &str,
        placeholder: &str,
        cursor: usize,
        focused: bool,
        multiline: bool,
        focus_handle: &FocusHandle,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let is_empty = value.trim().is_empty();
        let border = if focused {
            theme::accent() // --focus-color: $blue
        } else {
            theme::surface_bg_alt() // --contrast-border in dark
        };

        // Build the display text with cursor
        let text_child: Div = if is_empty && !focused {
            // Placeholder
            div()
                .text_size(px(12.0))
                .text_color(theme::text_muted())
                .child(placeholder.to_string())
        } else if focused {
            // Editable: show text with cursor indicator
            let cursor_pos = cursor.min(value.len());
            let before = &value[..cursor_pos];
            let after = &value[cursor_pos..];

            if multiline {
                // For multiline, render with line wrapping
                let mut row = h_flex().items_start().text_size(px(12.0));
                row = row.child(
                    div()
                        .text_color(theme::text_main())
                        .child(before.to_string()),
                );
                row = row.child(
                    div()
                        .w(px(1.0))
                        .h(px(14.0))
                        .bg(theme::text_main())
                        .flex_shrink_0(),
                );
                row = row.child(
                    div()
                        .text_color(theme::text_main())
                        .child(after.to_string()),
                );
                row
            } else {
                h_flex()
                    .items_center()
                    .overflow_x_hidden()
                    .text_size(px(12.0))
                    .child(
                        div()
                            .text_color(theme::text_main())
                            .whitespace_nowrap()
                            .child(before.to_string()),
                    )
                    .child(
                        div()
                            .w(px(1.0))
                            .h(px(14.0))
                            .bg(theme::text_main())
                            .flex_shrink_0(),
                    )
                    .child(
                        div()
                            .text_color(theme::text_main())
                            .whitespace_nowrap()
                            .child(after.to_string()),
                    )
            }
        } else {
            // Has text, not focused
            if multiline {
                div()
                    .text_size(px(12.0))
                    .text_color(theme::text_main())
                    .child(value.to_string())
            } else {
                div()
                    .text_size(px(12.0))
                    .text_color(theme::text_main())
                    .overflow_x_hidden()
                    .whitespace_nowrap()
                    .child(value.to_string())
            }
        };

        let height = if multiline { px(80.0) } else { px(25.0) }; // --text-field-height: 25px

        let is_summary = id == "commit-summary-field";

        let mut field = div()
            .id(SharedString::from(id.to_string()))
            .track_focus(focus_handle)
            .key_context("text-field")
            .w_full()
            .h(height)
            .bg(theme::bg()) // --background-color (dark canvas)
            .border_1()
            .border_color(border)
            .px(px(8.0))
            .cursor_text()
            .child(text_child);

        if multiline {
            // Description: top corners rounded, bottom corners flat (action bar attaches)
            field = field
                .rounded_t(px(theme::CORNER_RADIUS))
                .rounded_b_none()
                .border_b_0()
                .py(px(6.0))
                .overflow_hidden();
        } else {
            // Summary: fully rounded
            field = field.rounded(px(theme::CORNER_RADIUS)).items_center();
        }

        if is_summary {
            field = field.on_key_down(cx.listener(Self::handle_summary_key));
        } else {
            field = field.on_key_down(cx.listener(Self::handle_description_key));
        }

        field
    }

    // ------------------------------------------------------------------
    // Commit form (interactive)
    // ------------------------------------------------------------------

    fn render_commit_form_interactive(
        &self,
        branch_name: &str,
        summary_focused: bool,
        description_focused: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Action bar buttons (below description, matching GitHub Desktop layout)
        let action_bar_btn = |id: &str, label: &str| -> Stateful<Div> {
            div()
                .id(SharedString::from(id.to_string()))
                .flex_shrink_0()
                .cursor_pointer()
                .hover(|s| s.bg(theme::hover_bg()))
                .rounded(px(3.0))
                .w(px(18.0))
                .h(px(17.0))
                .items_center()
                .justify_center()
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme::text_muted())
                        .child(label.to_string()),
                )
        };

        let sparkle_button = action_bar_btn("ai-generate-btn", "\u{2728}")
            .on_click(cx.listener(|app, _evt, _win, cx| {
                app.generate_ai_commit(cx);
            }));

        let settings_button = action_bar_btn("commit-settings-btn", "\u{2699}")
            .on_click(cx.listener(|app, _evt, _win, cx| {
                app.nav.show_settings = true;
                cx.notify();
            }));

        // Action bar — sits below the description textarea
        let action_bar = h_flex()
            .w_full()
            .h(px(26.0))
            .px(px(5.0))
            .items_center()
            .gap(px(2.0))
            .bg(theme::surface_bg())
            .border_1()
            .border_t_0()
            .border_color(theme::surface_bg_alt())
            .rounded_b(px(theme::CORNER_RADIUS))
            .child(sparkle_button)
            .child(
                div()
                    .w(px(1.0))
                    .h(px(12.0))
                    .bg(theme::surface_bg_alt())
                    .mx(px(2.0)),
            )
            .child(settings_button);

        // Summary field — editable single-line input
        let summary_field = self.render_text_field(
            "commit-summary-field",
            &self.commit.summary,
            "Summary (required)",
            self.summary_cursor,
            summary_focused,
            false,
            &self.summary_focus,
            cx,
        );

        // Description field — editable multi-line input (no bottom radius, action bar attaches)
        let description_field = self.render_text_field(
            "commit-description-field",
            &self.commit.body,
            "Description",
            self.description_cursor,
            description_focused,
            true,
            &self.description_focus,
            cx,
        );

        // Description + action bar grouped together (shared border)
        let description_group = v_flex()
            .w_full()
            .child(description_field)
            .child(action_bar);

        let commit_label = format!("Commit to {branch_name}");

        v_flex()
            .w_full()
            .border_t_1()
            .border_color(theme::toolbar_button_border())
            .bg(theme::panel_bg())
            .p(px(10.0)) // --spacing: 10px
            .gap(px(10.0))
            .child(summary_field)
            .child(description_group)
            .child(
                Button::new("commit-btn")
                    .label(commit_label)
                    .small()
                    .custom(
                        ButtonCustomVariant::new(cx)
                            .color(theme::commit_button_bg())
                            .foreground(theme::commit_button_text())
                            .hover(theme::commit_button_hover_bg())
                            .active(theme::commit_button_hover_bg()),
                    )
                    .on_click(cx.listener(|app, _evt, _win, cx| {
                        app.commit_all(cx);
                    })),
            )
    }

    // ------------------------------------------------------------------
    // Workspace
    // ------------------------------------------------------------------

    fn render_workspace(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let sidebar_tab = self.nav.sidebar_tab;

        // Determine the active file list and selected file based on tab.
        let (diffs, selected_file): (&[DiffEntry], Option<&str>) = match sidebar_tab {
            SidebarTab::Changes => {
                let diffs = self
                    .repo
                    .snapshot
                    .as_ref()
                    .map(|s| s.diffs.as_slice())
                    .unwrap_or(&[]);
                let sel = self.selection.selected_change.as_deref();
                (diffs, sel)
            }
            SidebarTab::History => {
                let diffs = self
                    .selection
                    .commit_diffs
                    .as_deref()
                    .unwrap_or(&[]);
                let sel = self.selection.selected_commit_file.as_deref();
                (diffs, sel)
            }
        };

        // Find the diff entry for the selected file.
        let selected_diff = selected_file
            .and_then(|path| diffs.iter().find(|d| d.path == path));

        // For History tab, show a file list panel on the left of the workspace.
        if sidebar_tab == SidebarTab::History && !diffs.is_empty() {
            let file_list = self.render_commit_file_list(diffs, selected_file, cx);
            h_flex()
                .flex_1()
                .h_full()
                .items_start()
                .overflow_hidden()
                .child(file_list)
                .child(crate::ui::workspace::render_workspace(
                    selected_file,
                    selected_diff,
                ))
        } else {
            h_flex()
                .flex_1()
                .h_full()
                .items_start()
                .overflow_hidden()
                .child(crate::ui::workspace::render_workspace(
                    selected_file,
                    selected_diff,
                ))
        }
    }

    fn render_commit_file_list(
        &self,
        diffs: &[DiffEntry],
        selected_file: Option<&str>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let view = cx.entity().clone();
        let diffs_snapshot: Vec<DiffEntry> = diffs.iter().cloned().collect();
        let sel = selected_file.map(String::from);

        let file_list = uniform_list(
            "commit-file-list",
            diffs_snapshot.len(),
            move |range, _win, _cx| {
                let sel = sel.clone();
                range
                    .map(|ix| {
                        let entry = &diffs_snapshot[ix];
                        let is_selected =
                            sel.as_deref() == Some(entry.path.as_str());
                        let bg = if is_selected {
                            theme::with_alpha(theme::accent_muted(), 0.2)
                        } else {
                            gpui::transparent_black()
                        };
                        let text_color = if is_selected {
                            gpui::white().into()
                        } else {
                            theme::text_main()
                        };

                        let path = entry.path.clone();
                        let vh = view.clone();

                        h_flex()
                            .id(SharedString::from(format!(
                                "commit-file-{}",
                                entry.path
                            )))
                            .w_full()
                            .h(px(28.0))
                            .px(px(10.0))
                            .items_center()
                            .bg(bg)
                            .border_b_1()
                            .border_color(theme::border())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::hover_bg()))
                            .on_click(move |_evt, _win, cx| {
                                let path = path.clone();
                                vh.update(cx, |app, cx| {
                                    app.selection.selected_commit_file =
                                        Some(path);
                                    cx.notify();
                                });
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_x_hidden()
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(text_color)
                                            .whitespace_nowrap()
                                            .child(entry.path.clone()),
                                    ),
                            )
                    })
                    .collect()
            },
        )
        .w_full()
        .with_sizing_behavior(ListSizingBehavior::Infer);

        v_flex()
            .w(px(240.0))
            .h_full()
            .items_start()
            .bg(theme::panel_bg())
            .border_r_1()
            .border_color(theme::border())
            .child(
                h_flex()
                    .w_full()
                    .h(px(32.0))
                    .px(px(10.0))
                    .items_center()
                    .bg(theme::surface_bg_muted())
                    .border_b_1()
                    .border_color(theme::border())
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::text_muted())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!(
                                "{} changed files",
                                diffs.len()
                            )),
                    ),
            )
            .child(
                div()
                    .id("commit-file-list-viewport")
                    .w_full()
                    .flex_1()
                    .overflow_hidden()
                    .child(file_list),
            )
    }

    // ------------------------------------------------------------------
    // Status bar
    // ------------------------------------------------------------------

    fn render_status_bar(&self) -> impl IntoElement {
        crate::ui::status_bar::render_status_bar(
            &self.messages.status_message,
            &self.messages.error_message,
        )
    }

    // ------------------------------------------------------------------
    // Repo selector overlay
    // ------------------------------------------------------------------

    fn render_repo_selector_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let recent_repos = self.settings.recent_repos.clone();

        // Backdrop — click dismisses
        let backdrop = div()
            .id("repo-selector-backdrop")
            .absolute()
            .top(px(theme::TOOLBAR_HEIGHT))
            .left_0()
            .size_full()
            .on_click(cx.listener(|app, _evt, _win, cx| {
                app.nav.show_repo_selector = false;
                cx.notify();
            }));

        // Dropdown panel
        let mut dropdown = v_flex()
            .id("repo-selector-dropdown")
            .absolute()
            .top(px(theme::TOOLBAR_HEIGHT))
            .left_0()
            .w(px(280.0))
            .max_h(px(400.0))
            .overflow_y_scroll()
            .bg(theme::panel_bg())
            .border_1()
            .border_color(theme::border())
            .rounded_b(px(theme::CORNER_RADIUS));

        for repo_path in &recent_repos {
            let display_name = repo_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| repo_path.to_string_lossy().to_string());
            let full_path = repo_path.to_string_lossy().to_string();
            let path_clone = repo_path.clone();

            dropdown = dropdown.child(
                div()
                    .id(SharedString::from(format!("repo-{full_path}")))
                    .w_full()
                    .px(px(12.0))
                    .py(px(8.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::hover_bg()))
                    .border_b_1()
                    .border_color(theme::border())
                    .on_click(cx.listener(move |app, _evt, _win, cx| {
                        app.open_repo_with_notify(path_clone.clone(), cx);
                    }))
                    .child(
                        v_flex()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme::text_main())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(display_name),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(theme::text_muted())
                                    .child(full_path.clone()),
                            ),
                    ),
            );
        }

        // "Open Folder..." row
        dropdown = dropdown.child(
            div()
                .w_full()
                .px(px(8.0))
                .py(px(4.0))
                .child(
                    h_flex()
                        .id("repo-open-folder")
                        .items_center()
                        .gap(px(6.0))
                        .px(px(8.0))
                        .py(px(4.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::surface_bg_alt()))
                        .child(
                            div()
                                .text_size(px(14.0))
                                .text_color(theme::text_muted())
                                .child("\u{1F4C2}"),
                        )
                        .child(
                            div()
                                .text_size(px(theme::FONT_SIZE))
                                .text_color(theme::text_main())
                                .child("Open Folder\u{2026}"),
                        )
                        .on_click(cx.listener(|app, _evt, _win, cx| {
                            app.open_repo_dialog(cx);
                        })),
                ),
        );

        div().size_full().absolute().top_0().left_0().child(backdrop).child(dropdown)
    }

    // ------------------------------------------------------------------
    // Settings overlay
    // ------------------------------------------------------------------

    fn render_settings_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::ui::ui_state::SettingsSection;

        let section = self.nav.settings_section;

        // Backdrop
        let backdrop = div()
            .id("settings-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(theme::with_alpha(theme::bg(), 0.6))
            .on_click(cx.listener(|app, _evt, _win, cx| {
                app.nav.show_settings = false;
                cx.notify();
            }));

        // Left nav
        let git_nav = div()
            .id("settings-nav-git")
            .w_full()
            .px(px(12.0))
            .py(px(8.0))
            .cursor_pointer()
            .rounded(px(4.0))
            .bg(if section == SettingsSection::Git {
                theme::surface_bg()
            } else {
                gpui::transparent_black()
            })
            .hover(|s| s.bg(theme::hover_bg()))
            .on_click(cx.listener(|app, _evt, _win, cx| {
                app.nav.settings_section = SettingsSection::Git;
                cx.notify();
            }))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(if section == SettingsSection::Git {
                        theme::text_main()
                    } else {
                        theme::text_muted()
                    })
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Git"),
            );

        let ai_nav = div()
            .id("settings-nav-ai")
            .w_full()
            .px(px(12.0))
            .py(px(8.0))
            .cursor_pointer()
            .rounded(px(4.0))
            .bg(if section == SettingsSection::Ai {
                theme::surface_bg()
            } else {
                gpui::transparent_black()
            })
            .hover(|s| s.bg(theme::hover_bg()))
            .on_click(cx.listener(|app, _evt, _win, cx| {
                app.nav.settings_section = SettingsSection::Ai;
                cx.notify();
            }))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(if section == SettingsSection::Ai {
                        theme::text_main()
                    } else {
                        theme::text_muted()
                    })
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("AI"),
            );

        let left_nav = v_flex()
            .w(px(140.0))
            .h_full()
            .p(px(8.0))
            .gap(px(4.0))
            .border_r_1()
            .border_color(theme::border())
            .child(git_nav)
            .child(ai_nav);

        // Right content
        let right_content: AnyElement = match section {
            SettingsSection::Git => self.render_settings_git().into_any_element(),
            SettingsSection::Ai => self.render_settings_ai().into_any_element(),
        };

        // Close button
        let close_button = div()
            .absolute()
            .top(px(8.0))
            .right(px(8.0))
            .child(
                div()
                    .id("settings-close")
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::surface_bg_alt()))
                    .rounded(px(4.0))
                    .px(px(6.0))
                    .py(px(2.0))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .text_color(theme::text_muted())
                            .child("\u{2715}"),
                    )
                    .on_click(cx.listener(|app, _evt, _win, cx| {
                        app.nav.show_settings = false;
                        cx.notify();
                    })),
            );

        // Panel
        let panel = div()
            .absolute()
            .top(px(80.0))
            .left(px(80.0))
            .right(px(80.0))
            .bottom(px(80.0))
            .bg(theme::panel_bg())
            .border_1()
            .border_color(theme::border())
            .rounded(px(theme::CORNER_RADIUS))
            .overflow_hidden()
            .child(
                h_flex()
                    .size_full()
                    .child(left_nav)
                    .child(right_content),
            )
            .child(close_button);

        div().size_full().absolute().top_0().left_0().child(backdrop).child(panel)
    }

    fn render_settings_git(&self) -> impl IntoElement {
        let user_name = self.repo.identity.user_name.clone();
        let user_email = self.repo.identity.user_email.clone();

        v_flex()
            .flex_1()
            .p(px(20.0))
            .gap(px(16.0))
            .child(
                div()
                    .text_size(px(16.0))
                    .text_color(theme::text_main())
                    .font_weight(FontWeight::BOLD)
                    .child("Git Configuration"),
            )
            .child(self.render_settings_field("User Name", &user_name))
            .child(self.render_settings_field("User Email", &user_email))
    }

    fn render_settings_ai(&self) -> impl IntoElement {
        let provider = format!("{:?}", self.settings.ai.provider);
        let model = self.settings.ai.model.clone();
        let endpoint = self.settings.ai.endpoint.clone();

        v_flex()
            .flex_1()
            .p(px(20.0))
            .gap(px(16.0))
            .child(
                div()
                    .text_size(px(16.0))
                    .text_color(theme::text_main())
                    .font_weight(FontWeight::BOLD)
                    .child("AI Configuration"),
            )
            .child(self.render_settings_field("Provider", &provider))
            .child(self.render_settings_field("Model", &model))
            .child(self.render_settings_field("Endpoint", &endpoint))
    }

    fn render_settings_field(&self, label: &str, value: &str) -> impl IntoElement {
        let display_value = if value.is_empty() {
            "(not set)".to_string()
        } else {
            value.to_string()
        };

        v_flex()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme::text_muted())
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(label.to_string()),
            )
            .child(
                div()
                    .w_full()
                    .h(px(32.0))
                    .rounded(px(4.0))
                    .bg(theme::surface_bg())
                    .border_1()
                    .border_color(theme::border())
                    .px(px(10.0))
                    .items_center()
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(if value.is_empty() {
                                theme::text_muted()
                            } else {
                                theme::text_main()
                            })
                            .child(display_value),
                    ),
            )
    }
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

// ---------------------------------------------------------------------------
// Text input helpers
// ---------------------------------------------------------------------------

fn prev_char_boundary(s: &str, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let mut p = pos - 1;
    while p > 0 && !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

fn next_char_boundary(s: &str, pos: usize) -> usize {
    if pos >= s.len() {
        return s.len();
    }
    let mut p = pos + 1;
    while p < s.len() && !s.is_char_boundary(p) {
        p += 1;
    }
    p
}
