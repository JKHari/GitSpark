use crate::models::{CommitSuggestion, DiffEntry, GitIdentity, RepoSnapshot};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetworkAction {
    Fetch,
    Pull,
    Push,
    PublishBranch,
    PublishRepository,
}

impl NetworkAction {
    pub fn from_snapshot(snapshot: &RepoSnapshot) -> Self {
        // No remote configured → publish repository
        if snapshot.repo.remote_name.is_none() {
            return Self::PublishRepository;
        }
        // Ahead with no tracking → publish branch
        // (simplified: if ahead > 0 and behind == 0 and branch is new)
        if snapshot.repo.behind > 0 {
            Self::Pull
        } else if snapshot.repo.ahead > 0 {
            Self::Push
        } else {
            Self::Fetch
        }
    }

    pub fn title(self, remote_name: &str) -> String {
        match self {
            Self::Fetch => format!("Fetch {remote_name}"),
            Self::Pull => format!("Pull {remote_name}"),
            Self::Push => format!("Push {remote_name}"),
            Self::PublishBranch => "Publish branch".to_string(),
            Self::PublishRepository => "Publish repository".to_string(),
        }
    }

    pub fn pending_title(self, remote_name: &str) -> String {
        match self {
            Self::Fetch => format!("Fetching {remote_name}"),
            Self::Pull => format!("Pulling {remote_name}"),
            Self::Push => format!("Pushing {remote_name}"),
            Self::PublishBranch => "Publishing branch\u{2026}".to_string(),
            Self::PublishRepository => "Publishing repository\u{2026}".to_string(),
        }
    }

    pub fn is_available(self) -> bool {
        // Publish states may not be runnable yet
        !matches!(self, Self::PublishRepository)
    }
}

pub struct RepoState {
    pub snapshot: Option<RepoSnapshot>,
    pub identity: GitIdentity,
    pub branch_target: String,
    pub merge_target: String,
    pub new_branch_name: String,
}

impl Default for RepoState {
    fn default() -> Self {
        Self {
            snapshot: None,
            identity: GitIdentity::default(),
            branch_target: String::new(),
            merge_target: String::new(),
            new_branch_name: String::new(),
        }
    }
}

pub struct CommitState {
    pub summary: String,
    pub body: String,
    pub ai_preview: Option<CommitSuggestion>,
    pub ai_in_flight: bool,
}

impl Default for CommitState {
    fn default() -> Self {
        Self {
            summary: String::new(),
            body: String::new(),
            ai_preview: None,
            ai_in_flight: false,
        }
    }
}

pub struct NetworkState {
    pub active_action: Option<NetworkAction>,
}

impl Default for NetworkState {
    fn default() -> Self {
        Self {
            active_action: None,
        }
    }
}

pub struct SelectionState {
    pub selected_change: Option<String>,
    pub selected_commit: Option<String>,
    pub selected_commit_file: Option<String>,
    pub commit_diffs: Option<Vec<DiffEntry>>,
}

impl Default for SelectionState {
    fn default() -> Self {
        Self {
            selected_change: None,
            selected_commit: None,
            selected_commit_file: None,
            commit_diffs: None,
        }
    }
}
