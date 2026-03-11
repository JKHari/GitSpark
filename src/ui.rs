use std::path::{Path, PathBuf};

use eframe::egui::{self, Align, Color32, RichText, Stroke, TextStyle, Vec2};
use rfd::FileDialog;

use crate::ai::AiClient;
use crate::git::GitClient;
use crate::models::{AppSettings, BranchInfo, CommitSuggestion, GitIdentity, RepoSnapshot};
use crate::storage::{load_settings, push_recent_repo, save_settings};

const BG: Color32 = Color32::from_rgb(18, 22, 29);
const PANEL_BG: Color32 = Color32::from_rgb(24, 29, 38);
const SURFACE_BG: Color32 = Color32::from_rgb(31, 37, 47);
const SURFACE_BG_ALT: Color32 = Color32::from_rgb(34, 40, 51);
const SURFACE_BG_MUTED: Color32 = Color32::from_rgb(27, 32, 41);
const BORDER: Color32 = Color32::from_rgb(56, 63, 76);
const TEXT_MAIN: Color32 = Color32::from_rgb(221, 226, 232);
const TEXT_MUTED: Color32 = Color32::from_rgb(146, 155, 168);
const ACCENT: Color32 = Color32::from_rgb(53, 105, 220);
const ACCENT_MUTED: Color32 = Color32::from_rgb(44, 77, 134);
const SUCCESS: Color32 = Color32::from_rgb(78, 168, 94);
const WARNING: Color32 = Color32::from_rgb(219, 180, 51);
const DANGER: Color32 = Color32::from_rgb(212, 83, 84);
const DIFF_BG: Color32 = Color32::from_rgb(17, 31, 20);

#[derive(Clone, Copy, PartialEq, Eq)]
enum MainTab {
    Workspace,
    Config,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SidebarTab {
    Changes,
    History,
}

pub struct RustTopApp {
    git: GitClient,
    ai: AiClient,
    settings: AppSettings,
    current_repo: Option<RepoSnapshot>,
    repo_identity: GitIdentity,
    selected_recent_repo: Option<usize>,
    selected_change: Option<String>,
    branch_target: String,
    merge_target: String,
    commit_message: String,
    ai_preview: Option<CommitSuggestion>,
    status_message: String,
    error_message: String,
    main_tab: MainTab,
    sidebar_tab: SidebarTab,
    filter_text: String,
}

impl RustTopApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_visuals(&cc.egui_ctx);

        let (settings, error_message) = match load_settings() {
            Ok(settings) => (settings, String::new()),
            Err(err) => (AppSettings::default(), err.to_string()),
        };

        Self {
            git: GitClient::new(),
            ai: AiClient::new(),
            settings,
            current_repo: None,
            repo_identity: GitIdentity::default(),
            selected_recent_repo: None,
            selected_change: None,
            branch_target: String::new(),
            merge_target: String::new(),
            commit_message: String::new(),
            ai_preview: None,
            status_message: "Open a repository to get started.".to_string(),
            error_message,
            main_tab: MainTab::Workspace,
            sidebar_tab: SidebarTab::Changes,
            filter_text: String::new(),
        }
    }

    fn open_repo_dialog(&mut self) {
        if let Some(path) = FileDialog::new().pick_folder() {
            self.open_repo(path);
        }
    }

    fn open_repo(&mut self, path: PathBuf) {
        match self.git.open_repo(path.clone()) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.add_recent_repo(path);
                self.status_message = "Repository loaded.".to_string();
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message = format!("Failed to open repository: {err}");
            }
        }
    }

    fn refresh_repo(&mut self) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
            return;
        };

        match self.git.refresh_repo(&path) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.status_message = "Repository refreshed.".to_string();
                self.error_message.clear();
            }
            Err(err) => self.error_message = format!("Refresh failed: {err}"),
        }
    }

    fn switch_branch(&mut self) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
            return;
        };

        if self.branch_target.trim().is_empty() {
            self.error_message = "Choose a branch first.".to_string();
            return;
        }

        match self.git.switch_branch(&path, self.branch_target.trim()) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.status_message =
                    format!("Switched to branch '{}'.", self.branch_target.trim());
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message = format!("Branch switch failed: {err}");
            }
        }
    }

    fn merge_branch(&mut self) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
            return;
        };

        if self.merge_target.trim().is_empty() {
            self.error_message = "Choose a branch to merge.".to_string();
            return;
        }

        match self.git.merge_branch(&path, self.merge_target.trim()) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.status_message = format!("Merged '{}'.", self.merge_target.trim());
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message = format!("Merge failed: {err}");
            }
        }
    }

    fn commit_all(&mut self) {
        let Some(path) = self.repo_path().map(PathBuf::from) else {
            self.error_message = "No repository selected.".to_string();
            return;
        };

        let message = self.commit_message.trim();
        if message.is_empty() {
            self.error_message = "Commit message cannot be empty.".to_string();
            return;
        }

        match self.git.commit_all(&path, message) {
            Ok(snapshot) => {
                self.adopt_snapshot(snapshot);
                self.commit_message.clear();
                self.ai_preview = None;
                self.status_message = "Commit created.".to_string();
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message = format!("Commit failed: {err}");
            }
        }
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

        match self.ai.generate_commit_message(&self.settings.ai, &diff) {
            Ok(suggestion) => {
                self.commit_message = if suggestion.body.trim().is_empty() {
                    suggestion.subject.clone()
                } else {
                    format!("{}\n\n{}", suggestion.subject, suggestion.body)
                };
                self.ai_preview = Some(suggestion);
                self.status_message = "Generated commit suggestion.".to_string();
                self.error_message.clear();
            }
            Err(err) => {
                self.error_message = format!("AI generation failed: {err}");
            }
        }
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
    }

    fn repo_path(&self) -> Option<&Path> {
        self.current_repo
            .as_ref()
            .map(|snapshot| snapshot.repo.path.as_path())
    }

    fn selected_diff_text(&self) -> String {
        let Some(snapshot) = &self.current_repo else {
            return "Open a repository to inspect diffs.".to_string();
        };

        let Some(selected_change) = &self.selected_change else {
            return "Select a file from the changes list.".to_string();
        };

        match snapshot
            .diffs
            .iter()
            .find(|diff| &diff.path == selected_change)
        {
            Some(diff) if diff.is_binary => "Binary file changed.".to_string(),
            Some(diff) if diff.diff.trim().is_empty() => "No diff text available.".to_string(),
            Some(diff) => diff.diff.clone(),
            None => "No diff available for this file.".to_string(),
        }
    }

    fn render_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar")
            .exact_height(52.0)
            .frame(
                egui::Frame::default()
                    .fill(PANEL_BG)
                    .inner_margin(egui::Margin::same(8))
                    .stroke(Stroke::new(1.0, BORDER)),
            )
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                    ui.add_space(4.0);
                    
                    // Current Repository
                    egui::ComboBox::from_id_salt("repo_selector")
                        .selected_text(
                            RichText::new(
                                self.current_repo
                                    .as_ref()
                                    .map(|s| s.repo.name.as_str())
                                    .unwrap_or("Choose repository")
                            )
                            .strong()
                            .color(TEXT_MAIN)
                        )
                        .height(300.0)
                        .show_ui(ui, |ui| {
                            ui.label("Repository selection not implemented");
                        });

                    ui.add_space(12.0);

                    // Current Branch
                    let current_branch = self.current_repo.as_ref()
                        .map(|s| s.repo.current_branch.clone())
                        .unwrap_or_else(|| "No branch".to_string());

                    egui::ComboBox::from_id_salt("branch_selector")
                        .selected_text(
                            RichText::new(&current_branch)
                                .color(TEXT_MAIN)
                        )
                        .height(300.0)
                        .show_ui(ui, |ui| {
                            if let Some(snapshot) = &self.current_repo {
                                for branch in &snapshot.branches {
                                    if ui.selectable_value(
                                        &mut self.branch_target, 
                                        branch.name.clone(), 
                                        &branch.name
                                    ).clicked() {
                                        self.switch_branch();
                                        ui.close_menu();
                                    }
                                }
                            }
                        });

                    ui.add_space(12.0);

                    // Fetch/Push Button
                    if let Some(snapshot) = &self.current_repo {
                         let label = format!("Fetch origin ({}↑ {}↓)", snapshot.repo.ahead, snapshot.repo.behind);
                         if ui.add(egui::Button::new(RichText::new(label).color(TEXT_MAIN)).fill(SURFACE_BG)).clicked() {
                             self.refresh_repo();
                         }
                    }

                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if ui.add(egui::Button::new("Add Repo").frame(false)).clicked() {
                             self.open_repo_dialog();
                        }
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

                if self.main_tab == MainTab::Config {
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("Workspace content is hidden while Config is open.")
                            .color(TEXT_MUTED),
                    );
                    return;
                }

                if self.current_repo.is_some() {
                    // Commit area at the bottom
                    egui::TopBottomPanel::bottom("commit_area_panel")
                        .resizable(false)
                        .min_height(160.0)
                        .frame(
                            egui::Frame::default()
                                .fill(PANEL_BG)
                                .inner_margin(egui::Margin::same(0))
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

                            let changes = self.current_repo.as_ref().map(|s| s.changes.clone()).unwrap_or_default();
                            
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

                                    ui.add_space(8.0);
                                    self.render_stash_row(ui);
                                });
                        });
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
            ui.set_height(42.0);
            tab_button(ui, &mut self.sidebar_tab, SidebarTab::Changes, "Changes");
            tab_button(ui, &mut self.sidebar_tab, SidebarTab::History, "History");
        });
        ui.separator();
    }

    fn render_filter_bar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            icon_button(ui, "≡", "Recent Repos").clicked().then(|| {
                if let Some(index) = self.selected_recent_repo {
                    if let Some(path) = self.settings.recent_repos.get(index).cloned() {
                        self.open_repo(path);
                    }
                }
            });

            let edit = egui::TextEdit::singleline(&mut self.filter_text)
                .hint_text("Filter")
                .desired_width(f32::INFINITY);
            ui.add_sized([ui.available_width() - 32.0, 28.0], edit);
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
                        icon_button(ui, "+", "Open Repo").clicked().then(|| {
                            self.open_repo_dialog();
                        });
                    });
                });
            });
        ui.add_space(8.0);
    }

    fn render_inline_branch_controls(&mut self, ui: &mut egui::Ui, branches: &[BranchInfo]) {
        egui::Frame::default()
            .fill(SURFACE_BG)
            .stroke(Stroke::new(1.0, BORDER))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Branch").color(TEXT_MUTED).small());
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if compact_action_button(ui, "Switch").clicked() {
                            self.switch_branch();
                        }
                    });
                });

                egui::ComboBox::from_id_salt("checkout_branch")
                    .selected_text(if self.branch_target.is_empty() {
                        "Choose branch".to_string()
                    } else {
                        self.branch_target.clone()
                    })
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for branch in branches {
                            ui.selectable_value(
                                &mut self.branch_target,
                                branch.name.clone(),
                                branch_label(branch),
                            );
                        }
                    });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Merge").color(TEXT_MUTED).small());
                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        if compact_action_button(ui, "Merge").clicked() {
                            self.merge_branch();
                        }
                    });
                });

                egui::ComboBox::from_id_salt("merge_branch")
                    .selected_text(if self.merge_target.is_empty() {
                        "Choose branch".to_string()
                    } else {
                        self.merge_target.clone()
                    })
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for branch in branches
                            .iter()
                            .filter(|branch| !branch.is_current && !branch.is_remote)
                        {
                            ui.selectable_value(
                                &mut self.merge_target,
                                branch.name.clone(),
                                branch.name.as_str(),
                            );
                        }
                    });
            });
    }

    fn render_change_row(
        &mut self,
        ui: &mut egui::Ui,
        change: &crate::models::ChangeEntry,
        index: usize,
    ) {
        let selected = self.selected_change.as_deref() == Some(change.path.as_str());
        let base_fill = if selected {
            ACCENT_MUTED
        } else if index % 2 == 0 {
            SURFACE_BG_MUTED
        } else {
            PANEL_BG
        };

        let response = egui::Frame::default()
            .fill(base_fill)
            .stroke(Stroke::new(1.0, BORDER))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.set_min_height(28.0);
                ui.horizontal(|ui| {
                    let badge_color = status_color(&change.status);
                    let (rect, _) =
                        ui.allocate_exact_size(Vec2::new(14.0, 14.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, PANEL_BG);
                    ui.painter().rect_stroke(
                        rect,
                        2.0,
                        Stroke::new(1.0, badge_color),
                        egui::StrokeKind::Outside,
                    );
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        status_symbol(&change.status),
                        egui::FontId::proportional(11.0),
                        badge_color,
                    );

                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(truncate_middle(&change.path, 48))
                            .color(TEXT_MAIN)
                            .size(13.5),
                    );
                });
            })
            .response;

        if response.clicked() {
            self.selected_change = Some(change.path.clone());
        }
    }

    fn render_stash_row(&mut self, ui: &mut egui::Ui) {
        egui::Frame::default()
            .fill(SURFACE_BG_MUTED)
            .stroke(Stroke::new(1.0, BORDER))
            .inner_margin(egui::Margin::same(8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("▸").color(TEXT_MUTED));
                    ui.label(RichText::new("Stashed Changes").color(TEXT_MAIN).size(13.5));
                });
            });
    }

    fn render_commit_sidebar(&mut self, ui: &mut egui::Ui) {
        egui::Frame::default()
            .fill(SURFACE_BG)
            .stroke(Stroke::new(1.0, BORDER))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (avatar_rect, _) =
                        ui.allocate_exact_size(Vec2::new(24.0, 24.0), egui::Sense::hover());
                    ui.painter().circle_filled(
                        avatar_rect.center(),
                        12.0,
                        Color32::from_rgb(181, 154, 126),
                    );
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Summary").color(TEXT_MUTED).small());
                        ui.add(
                            egui::TextEdit::singleline(&mut self.commit_message)
                                .desired_width(f32::INFINITY)
                                .hint_text("Create a commit message"),
                        );
                    });
                });

                ui.add_space(8.0);
                let body = commit_body_mut(&mut self.commit_message);
                ui.add(
                    egui::TextEdit::multiline(body)
                        .desired_width(f32::INFINITY)
                        .desired_rows(3)
                        .hint_text("Description"),
                );

                if let Some(preview) = &self.ai_preview {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!("AI: {}", preview.subject))
                            .color(TEXT_MUTED)
                            .small(),
                    );
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    icon_button(ui, "✦", "Generate with AI")
                        .clicked()
                        .then(|| self.generate_ai_commit());
                    icon_button(ui, "⌘", "Config")
                        .clicked()
                        .then(|| self.main_tab = MainTab::Config);
                });

                ui.add_space(10.0);
                let branch_label = self
                    .current_repo
                    .as_ref()
                    .map(|snapshot| snapshot.repo.current_branch.clone())
                    .unwrap_or_else(|| "branch".to_string());
                let label = format!("Commit changes to {branch_label}");
                if ui
                    .add_sized(
                        [ui.available_width(), 28.0],
                        egui::Button::new(RichText::new(label).color(Color32::WHITE).strong())
                            .fill(ACCENT)
                            .corner_radius(4.0),
                    )
                    .clicked()
                {
                    self.commit_all();
                }
            });
    }

    fn render_workspace(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(BG))
            .show(ctx, |ui| {
                if self.selected_change.is_none() {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new("No file selected").color(TEXT_MUTED).size(16.0));
                    });
                    return;
                }

                self.render_diff_header(ui);
                
                egui::Frame::default()
                    .fill(DIFF_BG)
                    .stroke(Stroke::new(1.0, BORDER))
                    .inner_margin(egui::Margin::same(0))
                    .show(ui, |ui| {
                        let diff_text = self.selected_diff_text();
                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                ui.style_mut().spacing.item_spacing = Vec2::ZERO;
                                
                                for line in diff_text.lines() {
                                    let (bg_color, text_color) = if line.starts_with('+') && !line.starts_with("+++") {
                                        (Color32::from_rgba_premultiplied(40, 167, 69, 50), TEXT_MAIN)
                                    } else if line.starts_with('-') && !line.starts_with("---") {
                                        (Color32::from_rgba_premultiplied(215, 58, 73, 50), TEXT_MAIN)
                                    } else if line.starts_with("@@") {
                                        (SURFACE_BG_ALT, ACCENT)
                                    } else {
                                        (Color32::TRANSPARENT, TEXT_MUTED)
                                    };

                                    egui::Frame::default()
                                        .fill(bg_color)
                                        .inner_margin(egui::Margin::symmetric(8, 2))
                                        .show(ui, |ui| {
                                            ui.add_sized(
                                                [ui.available_width(), 16.0],
                                                egui::Label::new(
                                                    RichText::new(line)
                                                        .family(egui::FontFamily::Monospace)
                                                        .size(12.5)
                                                        .color(text_color)
                                                ).wrap_mode(egui::TextWrapMode::Extend)
                                            );
                                        });
                                }
                            });
                    });
            });
    }

    fn render_diff_header(&mut self, ui: &mut egui::Ui) {
        egui::Frame::default()
            .fill(SURFACE_BG)
            .stroke(Stroke::new(1.0, BORDER))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let path = self
                        .selected_change
                        .as_deref()
                        .unwrap_or("Select a file from the left panel");
                    ui.label(RichText::new(path).color(TEXT_MUTED).size(13.0));

                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        icon_button(ui, "+", "Open Repo")
                            .clicked()
                            .then(|| self.open_repo_dialog());
                        icon_button(ui, "⚙", "Config")
                            .clicked()
                            .then(|| self.main_tab = MainTab::Config);
                    });
                });
            });
    }

    fn render_config(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(BG))
            .show(ctx, |ui| {
                egui::Frame::default()
                    .fill(SURFACE_BG)
                    .stroke(Stroke::new(1.0, BORDER))
                    .inner_margin(egui::Margin::same(16))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.heading(RichText::new("Preferences").color(TEXT_MAIN));
                            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                compact_action_button(ui, "Back")
                                    .clicked()
                                    .then(|| self.main_tab = MainTab::Workspace);
                            });
                        });

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
        self.render_top_bar(ctx);
        self.render_status_bar(ctx);
        self.render_sidebar(ctx);

        match self.main_tab {
            MainTab::Workspace => self.render_workspace(ctx),
            MainTab::Config => self.render_config(ctx),
        }
    }
}

fn configure_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = BG;
    visuals.widgets.noninteractive.bg_fill = SURFACE_BG;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.inactive.bg_fill = SURFACE_BG;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    visuals.widgets.hovered.bg_fill = SURFACE_BG_ALT;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_MUTED);
    visuals.widgets.active.bg_fill = SURFACE_BG_ALT;
    visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    visuals.selection.bg_fill = ACCENT_MUTED;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.override_text_color = Some(TEXT_MAIN);
    visuals.extreme_bg_color = SURFACE_BG_MUTED;
    visuals.faint_bg_color = SURFACE_BG_MUTED;
    visuals.code_bg_color = DIFF_BG;
    visuals.window_shadow = egui::epaint::Shadow::NONE;
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.button_padding = Vec2::new(10.0, 6.0);
    style.spacing.indent = 14.0;
    style.visuals.window_corner_radius = 6.0.into();
    style.visuals.menu_corner_radius = 6.0.into();
    style
        .text_styles
        .insert(TextStyle::Heading, egui::FontId::proportional(18.0));
    style
        .text_styles
        .insert(TextStyle::Body, egui::FontId::proportional(13.5));
    style
        .text_styles
        .insert(TextStyle::Monospace, egui::FontId::monospace(13.0));
    ctx.set_style(style);
}

fn toolbar_block(ui: &mut egui::Ui, title: &str, value: &str, icon: &str, width: f32) {
    egui::Frame::default()
        .fill(SURFACE_BG)
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_min_width(width);
            ui.set_height(46.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new(title).color(TEXT_MUTED).small());
                    ui.label(RichText::new(value).color(TEXT_MAIN).strong());
                });
                ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(icon).color(TEXT_MUTED));
                });
            });
        });
}

fn push_block(ui: &mut egui::Ui, text: String) {
    egui::Frame::default()
        .fill(SURFACE_BG)
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(egui::Margin::same(10))
        .show(ui, |ui| {
            ui.set_min_width(210.0);
            ui.set_height(46.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("↑").color(TEXT_MAIN).size(16.0));
                ui.vertical(|ui| {
                    ui.label(RichText::new(text).color(TEXT_MAIN).strong());
                    ui.label(
                        RichText::new("Last fetched recently")
                            .color(TEXT_MUTED)
                            .small(),
                    );
                });
            });
        });
}

fn icon_button(ui: &mut egui::Ui, icon: &str, tooltip: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(icon).color(TEXT_MAIN).size(14.0))
            .fill(SURFACE_BG)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(4.0)
            .min_size(Vec2::new(28.0, 28.0)),
    )
    .on_hover_text(tooltip)
}

fn compact_action_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(label).size(12.5).color(TEXT_MAIN))
            .fill(SURFACE_BG_MUTED)
            .stroke(Stroke::new(1.0, BORDER))
            .corner_radius(4.0),
    )
}

fn tab_button(ui: &mut egui::Ui, value: &mut SidebarTab, tab: SidebarTab, label: &str) {
    let active = *value == tab;
    let response = ui.add_sized(
        [110.0, 30.0],
        egui::Button::new(
            RichText::new(label)
                .color(if active { TEXT_MAIN } else { TEXT_MUTED })
                .strong(),
        )
        .fill(if active { SURFACE_BG } else { PANEL_BG })
        .stroke(Stroke::new(0.0, Color32::TRANSPARENT))
        .corner_radius(0.0),
    );

    if active {
        let underline_rect = egui::Rect::from_min_max(
            response.rect.left_bottom() - Vec2::new(0.0, 2.0),
            response.rect.right_bottom() + Vec2::new(0.0, 1.0),
        );
        ui.painter().rect_filled(underline_rect, 0.0, ACCENT);
    }

    if response.clicked() {
        *value = tab;
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
        "+"
    } else if status.contains('M') {
        "●"
    } else if status.contains('D') {
        "−"
    } else if status.contains('U') {
        "!"
    } else {
        "•"
    }
}

fn truncate_middle(input: &str, max_chars: usize) -> String {
    let chars = input.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return input.to_string();
    }

    let head = max_chars / 2;
    let tail = max_chars.saturating_sub(head + 1);
    format!(
        "{}…{}",
        chars[..head].iter().collect::<String>(),
        chars[chars.len().saturating_sub(tail)..]
            .iter()
            .collect::<String>()
    )
}

fn branch_label(branch: &BranchInfo) -> String {
    let mut label = branch.name.clone();
    if branch.is_current {
        label.push_str(" (current)");
    }
    if branch.is_remote {
        label.push_str(" [remote]");
    }
    label
}

fn commit_body_mut(commit_message: &mut String) -> &mut String {
    if let Some((_, body)) = commit_message.split_once("\n\n") {
        let mut lines = commit_message.lines();
        let summary = lines.next().unwrap_or_default().to_string();
        *commit_message = format!("{summary}\n\n{body}");
    }
    commit_message
}
