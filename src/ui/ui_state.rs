use crate::models::RemoteModelOption;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    Changes,
    History,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    Git,
    Ai,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MainTab {
    Workspace,
}

#[derive(Clone)]
pub enum OpenRouterModelsState {
    Idle,
    Loading,
    Ready(Vec<RemoteModelOption>),
    Error(String),
}

pub struct NavState {
    pub main_tab: MainTab,
    pub sidebar_tab: SidebarTab,
    pub show_settings: bool,
    pub show_repo_selector: bool,
    pub show_branch_selector: bool,
    pub show_network_dropdown: bool,
    pub settings_section: SettingsSection,
}

impl Default for NavState {
    fn default() -> Self {
        Self {
            main_tab: MainTab::Workspace,
            sidebar_tab: SidebarTab::Changes,
            show_settings: false,
            show_repo_selector: false,
            show_branch_selector: false,
            show_network_dropdown: false,
            settings_section: SettingsSection::Git,
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct ChangeFilterOptions {
    pub included_in_commit: bool,
    pub excluded_from_commit: bool,
    pub new_files: bool,
    pub modified_files: bool,
    pub deleted_files: bool,
}

impl ChangeFilterOptions {
    pub fn active_count(self) -> usize {
        [
            self.included_in_commit,
            self.excluded_from_commit,
            self.new_files,
            self.modified_files,
            self.deleted_files,
        ]
        .into_iter()
        .filter(|active| *active)
        .count()
    }
}

pub struct FilterState {
    pub filter_text: String,
    pub change_filters: ChangeFilterOptions,
    pub repo_filter_text: String,
    pub openrouter_model_filter: String,
    pub openrouter_models: OpenRouterModelsState,
}

impl Default for FilterState {
    fn default() -> Self {
        Self {
            filter_text: String::new(),
            change_filters: ChangeFilterOptions::default(),
            repo_filter_text: String::new(),
            openrouter_model_filter: String::new(),
            openrouter_models: OpenRouterModelsState::Idle,
        }
    }
}

pub struct MessageState {
    pub status_message: String,
    pub error_message: String,
}

impl MessageState {
    pub fn new(status: &str, error: String) -> Self {
        Self {
            status_message: status.to_string(),
            error_message: error,
        }
    }
}

impl Default for MessageState {
    fn default() -> Self {
        Self {
            status_message: "Open a repository to get started.".to_string(),
            error_message: String::new(),
        }
    }
}
