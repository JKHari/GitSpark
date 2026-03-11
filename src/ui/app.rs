use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::{env, process::Command};

use eframe::egui::{self, Align, Align2, Color32, RichText, Stroke, Vec2};
use egui_phosphor::regular as icons;
use rfd::FileDialog;

use crate::ai::AiClient;
use crate::git::GitClient;
use crate::models::{AppSettings, CommitSuggestion, DiffEntry, GitIdentity, RepoSnapshot};
use crate::storage::{load_settings, push_recent_repo, save_settings};
use crate::ui::components::buttons::{compact_action_button, icon_button, tab_button};
use crate::ui::components::diff::render_diff_text;
use crate::ui::theme::{
    ACCENT_MUTED, BG, BORDER, DANGER, DIFF_BG, PANEL_BG, SUCCESS, SURFACE_BG, SURFACE_BG_MUTED,
    TEXT_MAIN, TEXT_MUTED, WARNING, configure_visuals,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum MainTab {
    Workspace,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SidebarTab {
    Changes,
    History,
}

enum AppEvent {
    RepoLoaded(Result<RepoSnapshot, String>),
    RepoRefreshed(Result<RepoSnapshot, String>),
    BranchSwitched(Result<RepoSnapshot, String>, String),
    BranchMerged(Result<RepoSnapshot, String>, String),
    CommitCreated(Result<RepoSnapshot, String>),
    AiCommitGenerated(Result<CommitSuggestion, String>),
    CommitDiffLoaded(String, Result<Vec<DiffEntry>, String>),
}

pub struct RustTopApp {
    ctx: egui::Context,
    git: GitClient,
    settings: AppSettings,
    show_settings: bool,
    current_repo: Option<RepoSnapshot>,
    repo_identity: GitIdentity,
    selected_recent_repo: Option<usize>,
    selected_change: Option<String>,
    selected_commit: Option<String>,
    commit_diffs: Option<Vec<DiffEntry>>,
    selected_commit_file: Option<String>,
    branch_target: String,
    merge_target: String,
    commit_summary: String,
    commit_body: String,
    ai_preview: Option<CommitSuggestion>,
    status_message: String,
    error_message: String,
    main_tab: MainTab,
    sidebar_tab: SidebarTab,
    filter_text: String,
    event_tx: Sender<AppEvent>,
    event_rx: Receiver<AppEvent>,
}

impl RustTopApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        cc.egui_ctx.set_fonts(fonts);

        configure_visuals(&cc.egui_ctx);

        let (settings, error_message) = match load_settings() {
            Ok(settings) => (settings, String::new()),
            Err(err) => (AppSettings::default(), err.to_string()),
        };

        let (event_tx, event_rx) = mpsc::channel();
        let mut app = Self {
            ctx: cc.egui_ctx.clone(),
            git: GitClient::new(),
            settings: settings.clone(),
            show_settings: false,
            current_repo: None,
            repo_identity: GitIdentity::default(),
            selected_recent_repo: None,
            selected_change: None,
            selected_commit: None,
            commit_diffs: None,
            selected_commit_file: None,
            branch_target: String::new(),
            merge_target: String::new(),
            commit_summary: String::new(),
            commit_body: String::new(),
            ai_preview: None,
            status_message: "Open a repository to get started.".to_string(),
            error_message,
            main_tab: MainTab::Workspace,
            sidebar_tab: SidebarTab::Changes,
            filter_text: String::new(),
            event_tx,
            event_rx,
        };

        if let Some(last_repo) = settings.recent_repos.first() {
            app.open_repo(last_repo.clone());
        }

        app
    }

    fn open_repo_dialog(&mut self) {
        if let Some(path) = FileDialog::new().pick_folder() {
            self.open_repo(path);
        }
    }

    fn open_repo(&mut self, path: PathBuf) {
        self.status_message = "Loading repository...".to_string();
        self.error_message.clear();
        self.add_recent_repo(path.clone());
        let tx = self.event_tx.clone();
        let ctx = self.ctx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.open_repo(path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoLoaded(res));
            ctx.request_repaint();
        });
    }

    fn refresh_repo(&mut self) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
            return;
        };

        self.status_message = "Refreshing repository...".to_string();
        self.error_message.clear();
        let tx = self.event_tx.clone();
        let ctx = self.ctx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.refresh_repo(&path).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::RepoRefreshed(res));
            ctx.request_repaint();
        });
    }

    fn switch_branch(&mut self) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
            return;
        };

        let target = self.branch_target.trim().to_string();
        if target.is_empty() {
            self.error_message = "Choose a branch first.".to_string();
            return;
        }

        self.status_message = format!("Switching to '{}'...", target);
        self.error_message.clear();
        let tx = self.event_tx.clone();
        let ctx = self.ctx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.switch_branch(&path, &target).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::BranchSwitched(res, target));
            ctx.request_repaint();
        });
    }

    fn merge_branch(&mut self) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
            return;
        };

        let target = self.merge_target.trim().to_string();
        if target.is_empty() {
            self.error_message = "Choose a branch to merge.".to_string();
            return;
        }

        self.status_message = format!("Merging '{}'...", target);
        self.error_message.clear();
        let tx = self.event_tx.clone();
        let ctx = self.ctx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.merge_branch(&path, &target).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::BranchMerged(res, target));
            ctx.request_repaint();
        });
    }

    fn commit_all(&mut self) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
            return;
        };

        if self.commit_summary.trim().is_empty() {
            self.error_message = "Commit summary cannot be empty.".to_string();
            return;
        }

        let message = if self.commit_body.trim().is_empty() {
            self.commit_summary.trim().to_string()
        } else {
            format!(
                "{}\n\n{}",
                self.commit_summary.trim(),
                self.commit_body.trim()
            )
        };

        self.status_message = "Creating commit...".to_string();
        self.error_message.clear();
        let tx = self.event_tx.clone();
        let ctx = self.ctx.clone();
        let git = GitClient::new();
        thread::spawn(move || {
            let res = git.commit_all(&path, &message).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::CommitCreated(res));
            ctx.request_repaint();
        });
    }

    fn generate_ai_commit(&mut self) {
        let Some(snapshot) = &self.current_repo else {
            self.error_message =
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
            self.error_message = "No text diff available for AI commit generation.".to_string();
            return;
        }

        self.status_message = "Generating AI commit suggestion...".to_string();
        self.error_message.clear();
        let tx = self.event_tx.clone();
        let ctx = self.ctx.clone();
        let ai = AiClient::new();
        let settings = self.settings.ai.clone();
        thread::spawn(move || {
            let res = ai
                .generate_commit_message(&settings, &diff)
                .map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::AiCommitGenerated(res));
            ctx.request_repaint();
        });
    }

    fn save_git_config(&mut self) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
            return;
        };

        match self.git.write_identity(&path, &self.repo_identity) {
            Ok(()) => {
                self.status_message = "Git config saved.".to_string();
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message = format!("Failed to save git config: {err}");
            }
        }
    }

    fn load_identity(&mut self, path: &Path) {
        match self.git.read_identity(path) {
            Ok(identity) => {
                self.repo_identity = identity;
            }
            Err(err) => {
                self.repo_identity = GitIdentity::default();
                self.error_message = format!("Could not load git config: {err}");
            }
        }
    }

    fn add_recent_repo(&mut self, path: PathBuf) {
        push_recent_repo(&mut self.settings, path);
        self.selected_recent_repo = Some(0);
        self.persist_settings();
    }

    fn persist_settings(&mut self) {
        if let Err(err) = save_settings(&self.settings) {
            self.error_message = format!("Failed to save settings: {err}");
        }
    }

    fn adopt_snapshot(&mut self, snapshot: RepoSnapshot) {
        let previous_commit = self.selected_commit.clone();
        let current_branch = snapshot.repo.current_branch.clone();
        self.selected_change = snapshot.changes.first().map(|change| change.path.clone());
        self.branch_target = current_branch;
        self.merge_target = snapshot
            .branches
            .iter()
            .find(|branch| !branch.is_current && !branch.is_remote)
            .map(|branch| branch.name.clone())
            .unwrap_or_default();
        self.load_identity(&snapshot.repo.path);
        self.current_repo = Some(snapshot);

        let next_selected_commit = self.current_repo.as_ref().and_then(|repo| {
            previous_commit
                .filter(|oid| repo.history.iter().any(|commit| commit.oid == *oid))
                .or_else(|| repo.history.first().map(|commit| commit.oid.clone()))
        });

        self.selected_commit = next_selected_commit.clone();
        self.selected_commit_file = None;
        self.commit_diffs = None;

        if let Some(oid) = next_selected_commit {
            self.load_commit_diff(oid);
        }
    }

    fn repo_path(&self) -> Option<&Path> {
        self.current_repo
            .as_ref()
            .map(|snapshot| snapshot.repo.path.as_path())
    }

    fn selected_diff(&self) -> Option<&DiffEntry> {
        let snapshot = self.current_repo.as_ref()?;
        let selected_change = self.selected_change.as_ref()?;
        snapshot
            .diffs
            .iter()
            .find(|diff| &diff.path == selected_change)
    }

    fn load_commit_diff(&mut self, oid: String) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            return;
        };

        let tx = self.event_tx.clone();
        let ctx = self.ctx.clone();
        let git = GitClient::new();

        thread::spawn(move || {
            let res = git.get_commit_diff(&path, &oid).map_err(|e| e.to_string());
            let _ = tx.send(AppEvent::CommitDiffLoaded(oid, res));
            ctx.request_repaint();
        });
    }

    fn select_commit(&mut self, oid: String) {
        let already_selected = self.selected_commit.as_deref() == Some(oid.as_str());
        if already_selected && self.commit_diffs.is_some() {
            return;
        }

        self.selected_commit = Some(oid.clone());
        self.selected_commit_file = None;
        self.commit_diffs = None;
        self.load_commit_diff(oid);
    }

    fn render_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar")
            .exact_height(52.0)
            .frame(
                egui::Frame::default()
                    .fill(SURFACE_BG_MUTED)
                    .inner_margin(egui::Margin::same(8))
                    .stroke(Stroke::new(1.0, BORDER)),
            )
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                    ui.add_space(4.0);

                    // Current Repository
                    let repo_name = self
                        .current_repo
                        .as_ref()
                        .map(|s| s.repo.name.as_str())
                        .unwrap_or("Choose repository");

                    ui.menu_button(RichText::new(repo_name).strong().color(TEXT_MAIN), |ui| {
                        ui.label("Repository selection not implemented");
                        if ui.button("Add Repository...").clicked() {
                            ui.close_menu();
                            self.open_repo_dialog();
                        }
                    });

                    ui.add_space(12.0);

                    // Current Branch
                    let current_branch = self
                        .current_repo
                        .as_ref()
                        .map(|s| s.repo.current_branch.clone())
                        .unwrap_or_else(|| "No branch".to_string());

                    ui.menu_button(RichText::new(&current_branch).color(TEXT_MAIN), |ui| {
                        let branches = self
                            .current_repo
                            .as_ref()
                            .map(|s| s.branches.clone())
                            .unwrap_or_default();

                        ui.label(RichText::new("Switch branch").small().color(TEXT_MUTED));
                        ui.separator();

                        for branch in branches {
                            if ui.button(branch.name.clone()).clicked() {
                                self.branch_target = branch.name;
                                self.switch_branch();
                                ui.close_menu();
                            }
                        }
                    });

                    ui.add_space(12.0);

                    // Fetch/Push Button
                    if let Some(snapshot) = &self.current_repo {
                        let label = format!(
                            "Fetch origin ({}↑ {}↓)",
                            snapshot.repo.ahead, snapshot.repo.behind
                        );
                        if ui
                            .add(
                                egui::Button::new(RichText::new(label).color(TEXT_MAIN))
                                    .fill(SURFACE_BG),
                            )
                            .clicked()
                        {
                            self.refresh_repo();
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        // Remove "Add Repo" button as it is in the menu now
                        let _ = ui;
                    });
                });
            });
    }

    fn render_status_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(26.0)
            .frame(
                egui::Frame::default()
                    .fill(PANEL_BG)
                    .inner_margin(egui::Margin::same(6)),
            )
            .show(ctx, |ui| {
                let text = if !self.error_message.is_empty() {
                    RichText::new(&self.error_message).color(DANGER)
                } else {
                    RichText::new(&self.status_message).color(TEXT_MUTED)
                };
                ui.label(text);
            });
    }

    fn render_sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(260.0)
            .min_width(220.0)
            .frame(
                egui::Frame::default()
                    .fill(PANEL_BG)
                    .inner_margin(egui::Margin::same(0)),
            )
            .show(ctx, |ui| {
                self.render_sidebar_tabs(ui);

                if self.current_repo.is_some() {
                    match self.sidebar_tab {
                        SidebarTab::Changes => {
                            // Commit area at the bottom
                            egui::TopBottomPanel::bottom("commit_area_panel")
                                .resizable(false)
                                .min_height(170.0)
                                .frame(
                                    egui::Frame::default()
                                        .fill(PANEL_BG)
                                        .inner_margin(egui::Margin::symmetric(0, 4))
                                        .stroke(Stroke::new(1.0, BORDER)),
                                )
                                .show_inside(ui, |ui| {
                                    self.render_commit_sidebar(ui);
                                });

                            // Changes list in the remaining space
                            egui::CentralPanel::default()
                                .frame(
                                    egui::Frame::default()
                                        .fill(PANEL_BG)
                                        .inner_margin(egui::Margin::same(0)),
                                )
                                .show_inside(ui, |ui| {
                                    self.render_filter_bar(ui);
                                    self.render_changes_header(ui);

                                    let changes = self
                                        .current_repo
                                        .as_ref()
                                        .map(|s| s.changes.clone())
                                        .unwrap_or_default();

                                    egui::ScrollArea::vertical()
                                        .auto_shrink([false, false])
                                        .show(ui, |ui| {
                                            for (index, change) in changes.iter().enumerate() {
                                                if !matches_filter(&self.filter_text, &change.path) {
                                                    continue;
                                                }
                                                self.render_change_row(ui, change, index);
                                            }

                                            if changes.is_empty() {
                                                ui.add_space(20.0);
                                                ui.vertical_centered(|ui| {
                                                    ui.label(RichText::new("No changes").color(TEXT_MUTED));
                                                });
                                            }
                                        });
                                });
                        }
                        SidebarTab::History => {
                             // Render history list
                             self.render_history_sidebar(ui);
                        }
                    }
                } else {
                    egui::CentralPanel::default().show_inside(ui, |ui| {
                        egui::Frame::default()
                            .fill(SURFACE_BG_MUTED)
                            .inner_margin(egui::Margin::same(12))
                            .stroke(Stroke::new(1.0, BORDER))
                            .show(ui, |ui| {
                                ui.label(RichText::new("No repository loaded").color(TEXT_MAIN).strong());
                                ui.label(RichText::new("Use the + button in the header or the recent repository picker to load a repo.").color(TEXT_MUTED));

                                ui.add_space(10.0);
                                self.render_recent_repos_picker(ui);
                            });
                    });
                }
            });
    }

    fn render_sidebar_tabs(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.set_height(46.0);
            ui.add_space(4.0);
            tab_button(ui, &mut self.sidebar_tab, SidebarTab::Changes, "Changes");
            tab_button(ui, &mut self.sidebar_tab, SidebarTab::History, "History");
        });
        ui.separator();
    }

    fn render_history_sidebar(&mut self, ui: &mut egui::Ui) {
        let history = self
            .current_repo
            .as_ref()
            .map(|repo| repo.history.clone())
            .unwrap_or_default();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;

                if history.is_empty() {
                    ui.add_space(20.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("No history").color(TEXT_MUTED));
                    });
                    return;
                }

                for commit in &history {
                    self.render_history_row(ui, commit);
                }
            });
    }

    fn render_history_row(&mut self, ui: &mut egui::Ui, commit: &crate::models::CommitInfo) {
        let is_selected = self.selected_commit.as_deref() == Some(commit.oid.as_str());
        let bg_color = if is_selected {
            ACCENT_MUTED
        } else {
            Color32::TRANSPARENT
        };
        let summary_color = if is_selected {
            Color32::WHITE
        } else {
            TEXT_MAIN
        };
        let meta_color = if is_selected {
            Color32::from_gray(225)
        } else {
            TEXT_MUTED
        };
        let summary = if commit.summary.trim().is_empty() {
            "Empty commit message"
        } else {
            commit.summary.trim()
        };

        let mut meta_parts = Vec::new();
        if commit.is_head {
            meta_parts.push("HEAD".to_string());
        }
        meta_parts.push(commit.short_oid.clone());
        meta_parts.push(commit.author_name.clone());
        meta_parts.push(commit.date.clone());
        let meta_text = meta_parts.join(" • ");

        let response = egui::Frame::default()
            .fill(bg_color)
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_min_height(40.0);
                ui.set_width(ui.available_width());

                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 3.0);
                    ui.add_sized(
                        [ui.available_width(), 18.0],
                        egui::Label::new(
                            RichText::new(truncate_single_line(summary, 54))
                                .color(summary_color)
                                .strong(),
                        )
                        .truncate(),
                    );
                    ui.add_sized(
                        [ui.available_width(), 16.0],
                        egui::Label::new(
                            RichText::new(truncate_single_line(&meta_text, 72)).color(meta_color),
                        )
                        .truncate(),
                    );
                });
            })
            .response
            .interact(egui::Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);

        ui.painter().hline(
            response.rect.x_range(),
            response.rect.bottom(),
            Stroke::new(1.0, BORDER),
        );

        if response.clicked() {
            self.select_commit(commit.oid.clone());
        }
    }

    fn render_filter_bar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let edit = egui::TextEdit::singleline(&mut self.filter_text)
                .hint_text("Filter files")
                .desired_width(f32::INFINITY)
                .margin(egui::Margin::symmetric(4, 4));

            ui.add_sized([ui.available_width(), 24.0], edit);
        });
        ui.add_space(8.0);
    }

    fn render_changes_header(&mut self, ui: &mut egui::Ui) {
        egui::Frame::default()
            .fill(SURFACE_BG)
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let count = self
                        .current_repo
                        .as_ref()
                        .map(|snapshot| snapshot.changes.len())
                        .unwrap_or(0);
                    ui.label(
                        RichText::new(format!("{count} changed files"))
                            .color(TEXT_MAIN)
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        icon_button(ui, icons::PLUS, "Open Repo")
                            .clicked()
                            .then(|| {
                                self.open_repo_dialog();
                            });
                    });
                });
            });
        ui.add_space(8.0);
    }

    fn render_change_row(
        &mut self,
        ui: &mut egui::Ui,
        change: &crate::models::ChangeEntry,
        _index: usize,
    ) {
        let selected = self.selected_change.as_deref() == Some(change.path.as_str());

        let (bg_fill, text_color) = if selected {
            (ACCENT_MUTED, Color32::WHITE)
        } else {
            (Color32::TRANSPARENT, TEXT_MAIN)
        };

        let response = egui::Frame::default()
            .fill(bg_fill)
            .inner_margin(egui::Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.set_min_height(24.0);
                ui.horizontal(|ui| {
                    // Checkbox (visual only for now)
                    let mut checked = true;
                    ui.checkbox(&mut checked, "");

                    // Path
                    let path_text = if change.path.len() > 40 {
                        format!(
                            "...{}",
                            &change.path[change.path.len().saturating_sub(37)..]
                        )
                    } else {
                        change.path.clone()
                    };

                    ui.label(RichText::new(path_text).color(text_color));

                    // Status Icon (Right aligned)
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        let badge_color = status_color(&change.status);
                        let symbol = status_symbol(&change.status);

                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(16.0, 16.0), egui::Sense::hover());
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            symbol,
                            egui::FontId::proportional(12.0),
                            badge_color,
                        );
                    });
                });
            })
            .response
            .interact(egui::Sense::click());

        if response.clicked() {
            self.selected_change = Some(change.path.clone());
        }

        response.context_menu(|ui| {
            ui.set_min_width(280.0);

            if ui.button("Discard Changes...").clicked() {
                self.discard_change(&change.path);
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Ignore File (Add to .gitignore)").clicked() {
                self.ignore_path(&change.path);
                ui.close_menu();
            }

            let ext = std::path::Path::new(&change.path)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("");

            if !ext.is_empty() {
                if ui
                    .button(format!("Ignore All .{} Files (Add to .gitignore)", ext))
                    .clicked()
                {
                    self.ignore_extension(ext);
                    ui.close_menu();
                }
            }

            ui.separator();
            if ui.button("Copy File Path").clicked() {
                if let Some(repo_path) = self.repo_path() {
                    let full_path = repo_path.join(&change.path);
                    ui.ctx().copy_text(full_path.to_string_lossy().to_string());
                    self.status_message = format!("Copied absolute path for '{}'.", change.path);
                    self.error_message.clear();
                }
                ui.close_menu();
            }
            if ui.button("Copy Relative File Path").clicked() {
                ui.ctx().copy_text(change.path.clone());
                self.status_message = format!("Copied relative path for '{}'.", change.path);
                self.error_message.clear();
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Reveal in Finder").clicked() {
                self.reveal_in_finder(&change.path);
                ui.close_menu();
            }
            if ui.button("Open in External Editor").clicked() {
                self.open_in_external_editor(&change.path);
                ui.close_menu();
            }
            if ui.button("Open with Default Program").clicked() {
                if let Some(repo_path) = self.repo_path() {
                    let full_path = repo_path.join(&change.path);
                    match open::that(&full_path) {
                        Ok(_) => {
                            self.status_message =
                                format!("Opened '{}' with the default program.", change.path);
                            self.error_message.clear();
                        }
                        Err(err) => {
                            self.error_message = format!(
                                "Failed to open '{}' with default program: {err}",
                                change.path
                            );
                        }
                    }
                }
                ui.close_menu();
            }
        });
    }

    fn discard_change(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
            return;
        };

        match self.git.discard_change(&repo_path, relative_path) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.status_message = format!("Discarded changes for '{}'.", relative_path);
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message =
                    format!("Failed to discard changes for '{}': {err}", relative_path);
            }
        }
    }

    fn ignore_path(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
            return;
        };

        let pattern = relative_path.replace('\\', "/");
        match self.git.append_gitignore_pattern(&repo_path, &pattern) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.status_message = format!("Added '{}' to .gitignore.", relative_path);
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message = format!("Failed to ignore '{}': {err}", relative_path);
            }
        }
    }

    fn ignore_extension(&mut self, ext: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
            return;
        };

        let pattern = format!("*.{ext}");
        match self.git.append_gitignore_pattern(&repo_path, &pattern) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.status_message = format!("Added '{}' to .gitignore.", pattern);
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message = format!("Failed to ignore '{}': {err}", pattern);
            }
        }
    }

    fn reveal_in_finder(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
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
                self.status_message = format!("Revealed '{}' in Finder.", relative_path);
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message = format!("Failed to reveal '{}': {err}", relative_path);
            }
        }
    }

    fn open_in_external_editor(&mut self, relative_path: &str) {
        let Some(repo_path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
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
                self.status_message = format!("Opened '{}' in external editor.", relative_path);
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message = format!(
                    "Failed to open '{}' in external editor: {err}",
                    relative_path
                );
            }
        }
    }

    fn render_stash_row(&mut self, ui: &mut egui::Ui) {
        let stash_count = self
            .current_repo
            .as_ref()
            .map(|repo| repo.stash_count)
            .unwrap_or(0);

        if stash_count == 0 {
            return;
        }

        ui.add_space(8.0);
        let label = if stash_count == 1 {
            "▸ Stashed Changes".to_string()
        } else {
            format!("▸ Stashed Changes ({stash_count})")
        };

        let response = ui.add(
            egui::Button::new(RichText::new(label).color(TEXT_MUTED))
                .fill(SURFACE_BG)
                .stroke(Stroke::new(1.0, BORDER))
                .corner_radius(5.0)
                .min_size(Vec2::new(ui.available_width(), 24.0)),
        );

        if response.clicked() {
            // TODO: Open stash view
        }
    }

    fn render_commit_sidebar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::default()
            .fill(PANEL_BG)
            .inner_margin(egui::Margin::symmetric(8, 8))
            .show(ui, |ui| {
                egui::Frame::default().fill(PANEL_BG).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width());

                        ui.horizontal(|ui| {
                            let (avatar_rect, _) =
                                ui.allocate_exact_size(Vec2::new(24.0, 24.0), egui::Sense::hover());
                            ui.painter().circle_filled(
                                avatar_rect.center(),
                                11.5,
                                Color32::from_rgb(201, 178, 158),
                            );
                            ui.painter().text(
                                avatar_rect.center(),
                                Align2::CENTER_CENTER,
                                "J",
                                egui::FontId::proportional(12.0),
                                Color32::from_rgb(70, 56, 47),
                            );

                            let summary = egui::TextEdit::singleline(&mut self.commit_summary)
                                .desired_width(f32::INFINITY)
                                .hint_text("Summary (required)")
                                .margin(egui::Margin::symmetric(6, 4));
                            ui.add_sized([ui.available_width(), 24.0], summary);
                        });

                        ui.add_space(8.0);

                        egui::Frame::default()
                            .fill(BG)
                            .stroke(Stroke::new(1.0, BORDER))
                            .corner_radius(5.0)
                            .inner_margin(egui::Margin::same(0))
                            .show(ui, |ui| {
                                ui.add_sized(
                                    [ui.available_width(), 108.0],
                                    egui::TextEdit::multiline(&mut self.commit_body)
                                        .desired_width(f32::INFINITY)
                                        .hint_text("Description")
                                        .margin(egui::Margin::symmetric(8, 8)),
                                );

                                let separator_y = ui.cursor().top();
                                ui.painter().hline(
                                    ui.min_rect().x_range(),
                                    separator_y,
                                    Stroke::new(1.0, BORDER),
                                );

                                egui::Frame::default()
                                    .fill(Color32::TRANSPARENT)
                                    .inner_margin(egui::Margin::symmetric(10, 6))
                                    .show(ui, |ui| {
                                        ui.set_height(28.0);
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 8.0;

                                            let toolbar_icon =
                                                |ui: &mut egui::Ui, icon: &str, tip: &str| {
                                                    ui.add(
                                                        egui::Button::new(
                                                            RichText::new(icon)
                                                                .size(15.0)
                                                                .color(TEXT_MUTED),
                                                        )
                                                        .fill(Color32::TRANSPARENT)
                                                        .stroke(Stroke::NONE)
                                                        .min_size(Vec2::new(20.0, 20.0)),
                                                    )
                                                    .on_hover_text(tip)
                                                };

                                            let _ = toolbar_icon(ui, "□", "Filtered commit");
                                            if toolbar_icon(ui, "+", "Generate with AI").clicked() {
                                                self.generate_ai_commit();
                                            }
                                            let _ = toolbar_icon(ui, "⌘", "Co-authors");

                                            ui.add_space(10.0);
                                            if toolbar_icon(ui, icons::GEAR, "Settings").clicked() {
                                                self.show_settings = true;
                                            }
                                        });
                                    });
                            });

                        if let Some(preview) = &self.ai_preview {
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new(format!("AI: {}", preview.subject))
                                    .small()
                                    .color(TEXT_MUTED),
                            );
                        }

                        self.render_stash_row(ui);
                        ui.add_space(8.0);
                        let branch_label = self
                            .current_repo
                            .as_ref()
                            .map(|snapshot| snapshot.repo.current_branch.clone())
                            .unwrap_or_else(|| "branch".to_string());
                        let commit_button = egui::Button::new(
                            RichText::new(format!("Commit to {branch_label}"))
                                .color(Color32::from_rgb(223, 230, 240))
                                .strong(),
                        )
                        .fill(Color32::from_rgb(58, 96, 194))
                        .stroke(Stroke::NONE)
                        .corner_radius(5.0);
                        if ui
                            .add_sized([ui.available_width(), 24.0], commit_button)
                            .clicked()
                        {
                            self.commit_all();
                        }

                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(
                                    RichText::new("Committed just now")
                                        .size(12.0)
                                        .color(TEXT_MUTED),
                                );
                                ui.label(
                                    RichText::new(truncate_commit_footer(&self.commit_summary))
                                        .size(12.0)
                                        .color(TEXT_MAIN),
                                );
                            });
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                let undo = egui::Button::new(
                                    RichText::new("Undo").size(12.0).color(TEXT_MAIN),
                                )
                                .fill(SURFACE_BG)
                                .stroke(Stroke::new(1.0, BORDER))
                                .corner_radius(5.0)
                                .min_size(Vec2::new(42.0, 22.0));
                                let _ = ui.add(undo);
                            });
                        });
                    });
                });
            });
    }

    fn render_workspace(&mut self, ctx: &egui::Context) {
        if self.sidebar_tab == SidebarTab::History {
            self.render_history_workspace(ctx);
            return;
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(BG))
            .show(ctx, |ui| {
                if self.selected_change.is_none() {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("No file selected")
                                .color(TEXT_MUTED)
                                .size(14.0),
                        );
                    });
                    return;
                }

                self.render_diff_header(ui);

                egui::Frame::default()
                    .fill(DIFF_BG)
                    .stroke(Stroke::new(1.0, BORDER))
                    .inner_margin(egui::Margin::same(0))
                    .show(ui, |ui| match self.selected_diff() {
                        Some(diff) if diff.is_binary => {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    RichText::new("Binary file changed.")
                                        .color(TEXT_MUTED)
                                        .size(14.0),
                                );
                            });
                        }
                        Some(diff) if diff.diff.trim().is_empty() => {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    RichText::new("No diff text available.")
                                        .color(TEXT_MUTED)
                                        .size(14.0),
                                );
                            });
                        }
                        Some(diff) => {
                            render_diff_text(ui, &diff.diff, &diff.path);
                        }
                        None => {
                            ui.centered_and_justified(|ui| {
                                ui.label(
                                    RichText::new("No diff available for this file.")
                                        .color(TEXT_MUTED)
                                        .size(14.0),
                                );
                            });
                        }
                    });
            });
    }

    fn render_history_workspace(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(BG))
            .show(ctx, |ui| {
                if let Some(oid) = &self.selected_commit {
                    let commit = self
                        .current_repo
                        .as_ref()
                        .and_then(|r| r.history.iter().find(|c| c.oid == *oid));

                    if let Some(commit) = commit {
                        // Top commit info
                        egui::TopBottomPanel::top("commit_info")
                            .frame(egui::Frame::default().fill(SURFACE_BG).inner_margin(12.0))
                            .show_inside(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.heading(RichText::new(&commit.summary).color(TEXT_MAIN));
                                });

                                if !commit.body.is_empty() {
                                    ui.add_space(4.0);
                                    ui.label(RichText::new(&commit.body).color(TEXT_MUTED));
                                }

                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "{} committed {}",
                                            commit.author_name, commit.date
                                        ))
                                        .color(TEXT_MUTED),
                                    );
                                    ui.label(
                                        RichText::new(&commit.short_oid)
                                            .monospace()
                                            .color(TEXT_MUTED),
                                    );
                                });
                            });

                        // Content area
                        if let Some(diffs) = &self.commit_diffs {
                            egui::TopBottomPanel::top("commit_file_list_panel")
                                .frame(egui::Frame::default().fill(PANEL_BG).inner_margin(0.0))
                                .show_inside(ui, |ui| {
                                    egui::SidePanel::left("commit_file_list")
                                        .resizable(true)
                                        .default_width(220.0)
                                        .show_inside(ui, |ui| {
                                            ui.add_space(8.0);
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} changed files",
                                                    diffs.len()
                                                ))
                                                .strong()
                                                .color(TEXT_MUTED),
                                            );
                                            ui.add_space(8.0);

                                            egui::ScrollArea::vertical().show(ui, |ui| {
                                                for diff in diffs {
                                                    let is_selected =
                                                        self.selected_commit_file.as_deref()
                                                            == Some(&diff.path);

                                                    if ui
                                                        .selectable_label(is_selected, &diff.path)
                                                        .clicked()
                                                    {
                                                        self.selected_commit_file =
                                                            Some(diff.path.clone());
                                                    }
                                                }
                                            });
                                        });

                                    egui::CentralPanel::default().show_inside(ui, |ui| {
                                        if let Some(selected_path) = &self.selected_commit_file {
                                            if let Some(diff) =
                                                diffs.iter().find(|d| d.path == *selected_path)
                                            {
                                                if diff.is_binary {
                                                    ui.centered_and_justified(|ui| {
                                                        ui.label(
                                                            RichText::new("Binary file changed.")
                                                                .color(TEXT_MUTED),
                                                        );
                                                    });
                                                } else if diff.diff.trim().is_empty() {
                                                    ui.centered_and_justified(|ui| {
                                                        ui.label(
                                                            RichText::new(
                                                                "No diff text available.",
                                                            )
                                                            .color(TEXT_MUTED),
                                                        );
                                                    });
                                                } else {
                                                    render_diff_text(ui, &diff.diff, selected_path);
                                                }
                                            }
                                        } else {
                                            ui.centered_and_justified(|ui| {
                                                ui.label(
                                                    RichText::new("Select a file to view diff")
                                                        .color(TEXT_MUTED),
                                                );
                                            });
                                        }
                                    });
                                });
                        } else {
                            ui.centered_and_justified(|ui| {
                                ui.spinner();
                            });
                        }
                    }
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new("Select a commit to view details").color(TEXT_MUTED),
                        );
                    });
                }
            });
    }

    fn render_diff_header(&mut self, ui: &mut egui::Ui) {
        egui::Frame::default()
            .fill(SURFACE_BG)
            .stroke(Stroke::new(1.0, BORDER))
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let path = self
                        .selected_change
                        .as_deref()
                        .unwrap_or("Select a file from the left panel");

                    ui.label(RichText::new(path).color(TEXT_MAIN).size(14.0).strong());
                });
            });
    }

    fn render_settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings;
        egui::Window::new(RichText::new("Settings").strong())
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_min_width(400.0);
                egui::Frame::default()
                    .fill(SURFACE_BG)
                    .inner_margin(egui::Margin::same(16))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.heading(RichText::new("Application Settings").color(TEXT_MAIN));
                        });
                        ui.add_space(20.0);

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(8.0);

                        ui.label(RichText::new("Git").color(TEXT_MUTED).small());
                        ui.label("User Name");
                        ui.text_edit_singleline(&mut self.repo_identity.user_name);
                        ui.label("User Email");
                        ui.text_edit_singleline(&mut self.repo_identity.user_email);
                        ui.label("Default Branch");
                        let default_branch = self
                            .repo_identity
                            .default_branch
                            .get_or_insert_with(String::new);
                        ui.text_edit_singleline(default_branch);
                        let mut pull_rebase = self.repo_identity.pull_rebase.unwrap_or(false);
                        ui.checkbox(&mut pull_rebase, "Use pull.rebase");
                        self.repo_identity.pull_rebase = Some(pull_rebase);
                        if compact_action_button(ui, "Save Git Config").clicked() {
                            self.save_git_config();
                        }

                        ui.add_space(14.0);
                        ui.separator();
                        ui.add_space(8.0);

                        ui.label(RichText::new("AI").color(TEXT_MUTED).small());
                        ui.label("Recent Repositories");
                        self.render_recent_repos_picker(ui);
                        ui.add_space(8.0);
                        ui.label("Endpoint");
                        ui.text_edit_singleline(&mut self.settings.ai.endpoint);
                        ui.label("Model");
                        ui.text_edit_singleline(&mut self.settings.ai.model);
                        ui.label("API Key");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.settings.ai.api_key)
                                .password(true),
                        );
                        ui.label("System Prompt");
                        ui.add(
                            egui::TextEdit::multiline(&mut self.settings.ai.system_prompt)
                                .desired_width(f32::INFINITY)
                                .desired_rows(5),
                        );

                        if compact_action_button(ui, "Save Preferences").clicked() {
                            self.persist_settings();
                            if self.error_message.is_empty() {
                                self.status_message = "App settings saved.".to_string();
                            }
                        }
                    });
            });
        self.show_settings = open;
    }

    fn render_recent_repos_picker(&mut self, ui: &mut egui::Ui) {
        if self.settings.recent_repos.is_empty() {
            ui.label(RichText::new("No recent repositories yet.").color(TEXT_MUTED));
            return;
        }

        let selected_text = self
            .selected_recent_repo
            .and_then(|index| self.settings.recent_repos.get(index))
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Choose recent repo".to_string());

        egui::ComboBox::from_id_salt("recent_repos_picker")
            .selected_text(selected_text)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for (index, path) in self.settings.recent_repos.iter().enumerate() {
                    ui.selectable_value(
                        &mut self.selected_recent_repo,
                        Some(index),
                        path.display().to_string(),
                    );
                }
            });
    }
}

impl eframe::App for RustTopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AppEvent::RepoLoaded(Ok(snapshot)) => {
                    self.adopt_snapshot(snapshot);
                    self.status_message = "Repository loaded.".to_string();
                    self.error_message.clear();
                }
                AppEvent::RepoLoaded(Err(err)) => {
                    self.error_message = format!("Failed to open repository: {err}");
                }
                AppEvent::RepoRefreshed(Ok(snapshot)) => {
                    self.adopt_snapshot(snapshot);
                    self.status_message = "Repository refreshed.".to_string();
                    self.error_message.clear();
                }
                AppEvent::RepoRefreshed(Err(err)) => {
                    self.error_message = format!("Refresh failed: {err}");
                }
                AppEvent::BranchSwitched(Ok(snapshot), branch) => {
                    self.adopt_snapshot(snapshot);
                    self.status_message = format!("Switched to branch '{branch}'.");
                    self.error_message.clear();
                }
                AppEvent::BranchSwitched(Err(err), _) => {
                    self.error_message = format!("Branch switch failed: {err}");
                }
                AppEvent::BranchMerged(Ok(snapshot), branch) => {
                    self.adopt_snapshot(snapshot);
                    self.status_message = format!("Merged '{branch}'.");
                    self.error_message.clear();
                }
                AppEvent::BranchMerged(Err(err), _) => {
                    self.error_message = format!("Merge failed: {err}");
                }
                AppEvent::CommitCreated(Ok(snapshot)) => {
                    self.adopt_snapshot(snapshot);
                    self.commit_summary.clear();
                    self.commit_body.clear();
                    self.ai_preview = None;
                    self.status_message = "Commit created.".to_string();
                    self.error_message.clear();
                }
                AppEvent::CommitCreated(Err(err)) => {
                    self.error_message = format!("Commit failed: {err}");
                }
                AppEvent::AiCommitGenerated(Ok(suggestion)) => {
                    self.commit_summary = suggestion.subject.clone();
                    self.commit_body = suggestion.body.clone();
                    self.ai_preview = Some(suggestion);
                    self.status_message = "Generated commit suggestion.".to_string();
                    self.error_message.clear();
                }
                AppEvent::AiCommitGenerated(Err(err)) => {
                    self.error_message = format!("AI generation failed: {err}");
                }
                AppEvent::CommitDiffLoaded(oid, Ok(diffs)) => {
                    if self.selected_commit.as_deref() == Some(oid.as_str()) {
                        if let Some(first) = diffs.first() {
                            self.selected_commit_file = Some(first.path.clone());
                        }
                        self.commit_diffs = Some(diffs);
                    }
                }
                AppEvent::CommitDiffLoaded(_, Err(err)) => {
                    self.error_message = format!("Failed to load commit details: {err}");
                }
            }
        }

        self.render_menu_bar(ctx);
        self.render_top_bar(ctx);
        self.render_status_bar(ctx);
        self.render_sidebar(ctx);

        match self.main_tab {
            MainTab::Workspace => self.render_workspace(ctx),
        }

        if self.show_settings {
            self.render_settings_window(ctx);
        }
    }
}

impl RustTopApp {
    fn render_menu_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu_bar")
            .exact_height(28.0)
            .frame(
                egui::Frame::default()
                    .fill(SURFACE_BG_MUTED)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .stroke(Stroke::new(0.0, Color32::TRANSPARENT)),
            )
            .show(ctx, |ui| {
                let previous_override = ui.visuals().override_text_color;
                ui.visuals_mut().override_text_color = Some(Color32::from_rgb(235, 240, 246));
                egui::menu::bar(ui, |ui| {
                    ui.menu_button(RichText::new("File").color(Color32::WHITE), |ui| {
                        if ui.button("New Repository...").clicked() {
                            ui.close_menu();
                        }
                        if ui.button("Add Local Repository...").clicked() {
                            self.open_repo_dialog();
                            ui.close_menu();
                        }
                        if ui.button("Clone Repository...").clicked() {
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Options...").clicked() {
                            self.show_settings = true;
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Exit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });

                    ui.menu_button(RichText::new("Edit").color(Color32::WHITE), |ui| {
                        let _ = ui.button("Undo");
                        let _ = ui.button("Redo");
                        ui.separator();
                        let _ = ui.button("Cut");
                        let _ = ui.button("Copy");
                        let _ = ui.button("Paste");
                        let _ = ui.button("Select All");
                    });

                    ui.menu_button(RichText::new("View").color(Color32::WHITE), |ui| {
                        if ui.button("Changes").clicked() {
                            self.sidebar_tab = SidebarTab::Changes;
                            ui.close_menu();
                        }
                        if ui.button("History").clicked() {
                            self.sidebar_tab = SidebarTab::History;
                            ui.close_menu();
                        }
                        ui.separator();
                        let _ = ui.button("Repository List");
                        ui.separator();
                        let _ = ui.button("Toggle Full Screen");
                    });

                    ui.menu_button(RichText::new("Repository").color(Color32::WHITE), |ui| {
                        if ui.button("Push").clicked() {
                            // Push
                            ui.close_menu();
                        }
                        if ui.button("Pull").clicked() {
                            self.refresh_repo();
                            ui.close_menu();
                        }
                        if ui.button("Remove...").clicked() {
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("View on GitHub").clicked() {
                            ui.close_menu();
                        }
                        if ui.button("Open in Terminal").clicked() {
                            ui.close_menu();
                        }
                        if ui.button("Show in Finder").clicked() {
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Repository Settings...").clicked() {
                            ui.close_menu();
                        }
                    });

                    ui.menu_button(RichText::new("Branch").color(Color32::WHITE), |ui| {
                        let _ = ui.button("New Branch...");
                        let _ = ui.button("Rename Branch...");
                        let _ = ui.button("Delete Branch...");
                        ui.separator();
                        let _ = ui.button("Update from Default Branch");
                        let _ = ui.button("Compare to Branch");
                        let _ = ui.button("Merge into Current Branch...");
                    });

                    ui.menu_button(RichText::new("Help").color(Color32::WHITE), |ui| {
                        let _ = ui.button("Report Issue...");
                        let _ = ui.button("Contact Support...");
                        ui.separator();
                        let _ = ui.button("Show Logs...");
                        ui.separator();
                        let _ = ui.button("About RustTop");
                    });
                });
                ui.visuals_mut().override_text_color = previous_override;
            });
    }
}

fn matches_filter(filter: &str, path: &str) -> bool {
    let filter = filter.trim();
    filter.is_empty()
        || path
            .to_ascii_lowercase()
            .contains(&filter.to_ascii_lowercase())
}

fn status_color(status: &str) -> Color32 {
    if status.contains('?') || status.contains('A') {
        SUCCESS
    } else if status.contains('M') {
        WARNING
    } else if status.contains('D') || status.contains('U') {
        DANGER
    } else {
        TEXT_MUTED
    }
}

fn status_symbol(status: &str) -> &'static str {
    if status.contains('?') || status.contains('A') {
        icons::PLUS
    } else if status.contains('M') {
        icons::DOT_OUTLINE
    } else if status.contains('D') {
        icons::MINUS
    } else if status.contains('U') {
        icons::WARNING
    } else {
        icons::QUESTION
    }
}

fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn truncate_single_line(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let mut chars = trimmed.chars();
    let shortened: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{shortened}...")
    } else {
        shortened
    }
}

fn truncate_commit_footer(summary: &str) -> String {
    let trimmed = summary.trim();
    if trimmed.is_empty() {
        return "No commit message yet".to_string();
    }

    let max = 34;
    let mut chars = trimmed.chars();
    let shortened: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        format!("{shortened}...")
    } else {
        shortened
    }
}
