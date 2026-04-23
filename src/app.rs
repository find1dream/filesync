use crate::file_ops::{ProgressState, TransferJob};
use crate::ssh::{RemoteEntry, SshClient};
use crate::util::{selected_entries, sort_dir_first};
use anyhow::Result;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Local,
    Remote,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AppMode {
    Browse,
    Confirm(ConfirmAction),
    Error,
    Status,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ConfirmAction {
    Delete,
    Copy,
}

#[derive(Clone, Debug)]
pub struct LocalEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
}

pub struct App {
    pub ssh: SshClient,
    pub remote_host_label: String,

    pub local_cwd: PathBuf,
    pub local_entries: Vec<LocalEntry>,
    pub local_cursor: usize,
    pub local_selected: HashSet<usize>,
    pub local_scroll: usize,

    pub remote_cwd: PathBuf,
    pub remote_entries: Vec<RemoteEntry>,
    pub remote_cursor: usize,
    pub remote_selected: HashSet<usize>,
    pub remote_scroll: usize,

    pub show_hidden: bool,
    pub active_panel: Panel,
    pub mode: AppMode,

    pub transfer_job: Option<TransferJob>,
    pub progress: ProgressState,

    pub status_msg: String,
    pub error_msg: String,

    /// Visible list rows — set by render before scroll clamping.
    pub visible_height: usize,
}

impl App {
    pub fn new(user: String, host: String, password: String) -> Result<Self> {
        let remote_host_label = format!("{}@{}", user, host);
        let ssh = SshClient::connect(&user, &host, &password)?;

        let remote_cwd = ssh.home_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let remote_entries = ssh.list_dir(&remote_cwd).unwrap_or_default();

        let local_cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let local_entries = list_local(&local_cwd, false);

        Ok(Self {
            ssh,
            remote_host_label,
            local_cwd,
            local_entries,
            local_cursor: 0,
            local_selected: HashSet::new(),
            local_scroll: 0,
            remote_cwd,
            remote_entries,
            remote_cursor: 0,
            remote_selected: HashSet::new(),
            remote_scroll: 0,
            show_hidden: false,
            active_panel: Panel::Local,
            mode: AppMode::Browse,
            transfer_job: None,
            progress: ProgressState::default(),
            status_msg: String::new(),
            error_msg: String::new(),
            visible_height: 20,
        })
    }

    pub fn refresh_local(&mut self) {
        self.local_entries = list_local(&self.local_cwd, self.show_hidden);
        self.local_cursor = self.local_cursor.min(self.local_entries.len().saturating_sub(1));
        self.local_selected.clear();
        self.clamp_scroll();
    }

    pub fn refresh_remote(&mut self) {
        match self.ssh.list_dir(&self.remote_cwd) {
            Ok(entries) => {
                self.remote_entries = entries;
                self.remote_cursor =
                    self.remote_cursor.min(self.remote_entries.len().saturating_sub(1));
                self.remote_selected.clear();
                self.clamp_scroll();
            }
            Err(e) => {
                self.error_msg = format!("Failed to list remote dir: {}", e);
                self.mode = AppMode::Error;
            }
        }
    }

    pub fn move_cursor_up(&mut self) {
        match self.active_panel {
            Panel::Local => {
                if self.local_cursor > 0 {
                    self.local_cursor -= 1;
                }
            }
            Panel::Remote => {
                if self.remote_cursor > 0 {
                    self.remote_cursor -= 1;
                }
            }
        }
        self.clamp_scroll();
    }

    pub fn move_cursor_down(&mut self) {
        match self.active_panel {
            Panel::Local => {
                if self.local_cursor + 1 < self.local_entries.len() {
                    self.local_cursor += 1;
                }
            }
            Panel::Remote => {
                if self.remote_cursor + 1 < self.remote_entries.len() {
                    self.remote_cursor += 1;
                }
            }
        }
        self.clamp_scroll();
    }

    pub fn enter_dir(&mut self) {
        match self.active_panel {
            Panel::Local => {
                if let Some(entry) = self.local_entries.get(self.local_cursor) {
                    if entry.is_dir {
                        self.local_cwd = entry.path.clone();
                        self.local_cursor = 0;
                        self.local_scroll = 0;
                        self.refresh_local();
                    }
                }
            }
            Panel::Remote => {
                if let Some(entry) = self.remote_entries.get(self.remote_cursor) {
                    if entry.is_dir {
                        self.remote_cwd = entry.path.clone();
                        self.remote_cursor = 0;
                        self.remote_scroll = 0;
                        self.refresh_remote();
                    }
                }
            }
        }
    }

    pub fn go_parent(&mut self) {
        match self.active_panel {
            Panel::Local => {
                if let Some(parent) = self.local_cwd.parent() {
                    self.local_cwd = parent.to_path_buf();
                    self.local_cursor = 0;
                    self.local_scroll = 0;
                    self.refresh_local();
                }
            }
            Panel::Remote => {
                if let Some(parent) = self.remote_cwd.parent() {
                    self.remote_cwd = parent.to_path_buf();
                    self.remote_cursor = 0;
                    self.remote_scroll = 0;
                    self.refresh_remote();
                }
            }
        }
    }

    pub fn toggle_select(&mut self) {
        let (cursor, entries_len, selected) = match self.active_panel {
            Panel::Local => (
                self.local_cursor,
                self.local_entries.len(),
                &mut self.local_selected,
            ),
            Panel::Remote => (
                self.remote_cursor,
                self.remote_entries.len(),
                &mut self.remote_selected,
            ),
        };
        if entries_len > 0 {
            if !selected.remove(&cursor) {
                selected.insert(cursor);
            }
        }
        self.move_cursor_down();
    }

    pub fn toggle_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.refresh_local();
    }

    pub fn selected_local_entries(&self) -> Vec<&LocalEntry> {
        selected_entries(&self.local_entries, self.local_cursor, &self.local_selected)
    }

    pub fn selected_remote_entries(&self) -> Vec<&RemoteEntry> {
        selected_entries(&self.remote_entries, self.remote_cursor, &self.remote_selected)
    }

    /// Clamp both panels' scroll offsets to keep their cursors visible.
    /// Call after changing `visible_height` or moving any cursor.
    pub fn clamp_scroll(&mut self) {
        let h = self.visible_height.max(1);
        clamp_one(self.local_cursor, &mut self.local_scroll, h);
        clamp_one(self.remote_cursor, &mut self.remote_scroll, h);
    }
}

fn clamp_one(cursor: usize, scroll: &mut usize, height: usize) {
    if cursor >= *scroll + height {
        *scroll = cursor + 1 - height;
    }
    if cursor < *scroll {
        *scroll = cursor;
    }
}

pub fn list_local(dir: &PathBuf, show_hidden: bool) -> Vec<LocalEntry> {
    let Ok(rd) = fs::read_dir(dir) else {
        return vec![];
    };
    let mut entries: Vec<LocalEntry> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !show_hidden && name.starts_with('.') {
                return None;
            }
            let meta = e.metadata().ok()?;
            Some(LocalEntry {
                name,
                path: e.path(),
                is_dir: meta.is_dir(),
                size: if meta.is_file() { meta.len() } else { 0 },
            })
        })
        .collect();

    entries.sort_by(|a, b| sort_dir_first(a.is_dir, &a.name, b.is_dir, &b.name));
    entries
}
