use crate::app::{App, AppMode, ConfirmAction, Panel};
use crate::file_ops::{ProgressState, delete_local, delete_remote, start_download, start_upload};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers, MouseButton, MouseEventKind, poll};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// Returns true if the app should quit.
pub fn handle_events(app: &mut App) -> Result<bool> {
    if !poll(Duration::from_millis(50))? {
        tick_transfer(app);
        return Ok(false);
    }

    match event::read()? {
        Event::Key(key) => {
            let should_quit = match app.mode {
                AppMode::Browse => handle_browse(app, key.code, key.modifiers)?,
                AppMode::Confirm(action) => handle_confirm(app, action, key.code)?,
                AppMode::Error => {
                    app.mode = AppMode::Browse;
                    app.error_msg.clear();
                    false
                }
                AppMode::Status => {
                    app.mode = AppMode::Browse;
                    app.status_msg.clear();
                    false
                }
            };
            if should_quit {
                return Ok(true);
            }
        }
        Event::Mouse(mouse) => handle_mouse(app, mouse.kind, mouse.column, mouse.row),
        _ => {}
    }

    tick_transfer(app);
    Ok(false)
}

fn tick_transfer(app: &mut App) {
    app.poll_remote_listing();

    if let Some(job) = &app.transfer_job {
        let p = job.progress.lock().unwrap();
        // Only clone when something changed to avoid 50ms-tick allocations.
        if p.bytes_done != app.progress.bytes_done || p.finished || p.error.is_some() {
            app.progress = p.clone();
        }
        if p.finished {
            let err = p.error.clone();
            let cancelled = p.cancelled;
            drop(p);
            let job = app.transfer_job.take().unwrap();
            let _ = job.handle.join();
            if cancelled {
                app.status_msg = "Transfer cancelled.".to_string();
                app.mode = AppMode::Status;
            } else if let Some(e) = err {
                app.error_msg = format!("Transfer error: {}", e);
                app.mode = AppMode::Error;
            } else {
                app.status_msg = "Transfer complete!".to_string();
                app.mode = AppMode::Status;
                app.refresh_local();
                app.refresh_remote();
            }
            app.progress = ProgressState::default();
        }
    }
}

fn handle_browse(app: &mut App, code: KeyCode, _mods: KeyModifiers) -> Result<bool> {
    match code {
        KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(true),

        KeyCode::Tab => {
            app.active_panel = match app.active_panel {
                Panel::Local => Panel::Remote,
                Panel::Remote => Panel::Local,
            };
        }

        KeyCode::Up | KeyCode::Char('k') => app.move_cursor_up(),
        KeyCode::Down | KeyCode::Char('j') => app.move_cursor_down(),
        KeyCode::PageUp => app.move_page_up(),
        KeyCode::PageDown => app.move_page_down(),
        KeyCode::Home => app.move_to_top(),
        KeyCode::End => app.move_to_bottom(),

        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => app.enter_dir(),
        KeyCode::Left | KeyCode::Backspace | KeyCode::Char('h') => app.go_parent(),

        KeyCode::Char(' ') => app.toggle_select(),
        KeyCode::Char('H') => app.toggle_hidden(),
        KeyCode::Char('s') => app.cycle_sort(),
        KeyCode::Char('o') => app.toggle_reverse(),

        KeyCode::Char('r') => match app.active_panel {
            Panel::Local => app.refresh_local(),
            Panel::Remote => app.refresh_remote(),
        },

        KeyCode::Char('d') | KeyCode::Delete => {
            if app.transfer_job.is_none() {
                app.mode = AppMode::Confirm(ConfirmAction::Delete);
            }
        }

        KeyCode::Char('c') => {
            if app.transfer_job.is_none() {
                app.mode = AppMode::Confirm(ConfirmAction::Copy);
            }
        }

        KeyCode::Esc => {
            if let Some(job) = &app.transfer_job {
                job.cancel.store(true, Ordering::Relaxed);
            }
        }

        _ => {}
    }
    Ok(false)
}

fn handle_confirm(app: &mut App, action: ConfirmAction, code: KeyCode) -> Result<bool> {
    app.mode = AppMode::Browse;
    if matches!(code, KeyCode::Char('y') | KeyCode::Char('Y')) {
        match action {
            ConfirmAction::Delete => do_delete(app),
            ConfirmAction::Copy => do_copy(app),
        }
    }
    Ok(false)
}

fn do_delete(app: &mut App) {
    match app.active_panel {
        Panel::Local => {
            let entries: Vec<_> = app.selected_local_entries().into_iter().cloned().collect();
            for entry in entries {
                if let Err(e) = delete_local(&entry) {
                    app.error_msg = format!("Delete error: {}", e);
                    app.mode = AppMode::Error;
                    app.refresh_local();
                    return;
                }
            }
            app.refresh_local();
            app.status_msg = "Deleted successfully.".to_string();
            app.mode = AppMode::Status;
        }
        Panel::Remote => {
            let entries: Vec<_> = app.selected_remote_entries().into_iter().cloned().collect();
            for entry in entries {
                if let Err(e) = delete_remote(&app.ssh, &entry) {
                    app.error_msg = format!("Delete error: {}", e);
                    app.mode = AppMode::Error;
                    app.refresh_remote();
                    return;
                }
            }
            app.refresh_remote();
            app.status_msg = "Deleted successfully.".to_string();
            app.mode = AppMode::Status;
        }
    }
}

fn handle_mouse(app: &mut App, kind: MouseEventKind, col: u16, row: u16) {
    match kind {
        MouseEventKind::ScrollUp => {
            if let Some(panel) = panel_at(app, col, row) {
                scroll_panel(app, panel, -1);
            }
        }
        MouseEventKind::ScrollDown => {
            if let Some(panel) = panel_at(app, col, row) {
                scroll_panel(app, panel, 1);
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            handle_click(app, col, row);
        }
        _ => {}
    }
}

fn panel_at(app: &App, col: u16, row: u16) -> Option<Panel> {
    for panel in [Panel::Local, Panel::Remote] {
        let area = match panel {
            Panel::Local => app.local_area?,
            Panel::Remote => app.remote_area?,
        };
        if col >= area.x && col < area.x + area.width && row >= area.y && row < area.y + area.height {
            return Some(panel);
        }
    }
    None
}

fn scroll_panel(app: &mut App, panel: Panel, delta: i32) {
    // Accelerate when scroll events arrive quickly; reset when the user pauses.
    let now = Instant::now();
    let gap_ms = app
        .last_scroll_time
        .map(|t| now.duration_since(t).as_millis())
        .unwrap_or(u128::MAX);

    if gap_ms > 120 {
        app.scroll_velocity = 1.0;
    } else {
        app.scroll_velocity = (app.scroll_velocity * 1.6).min(25.0);
    }
    app.last_scroll_time = Some(now);

    let step = app.scroll_velocity as usize;

    match panel {
        Panel::Local => {
            let len = app.local_entries.len();
            if delta > 0 {
                app.local_cursor = (app.local_cursor + step).min(len.saturating_sub(1));
            } else if delta < 0 {
                app.local_cursor = app.local_cursor.saturating_sub(step);
            }
        }
        Panel::Remote => {
            let len = app.remote_entries.len();
            if delta > 0 {
                app.remote_cursor = (app.remote_cursor + step).min(len.saturating_sub(1));
            } else if delta < 0 {
                app.remote_cursor = app.remote_cursor.saturating_sub(step);
            }
        }
    }
    app.clamp_scroll();
}

fn handle_click(app: &mut App, col: u16, row: u16) {
    let Some(panel) = panel_at(app, col, row) else { return };
    let area = match panel {
        Panel::Local => app.local_area.unwrap(),
        Panel::Remote => app.remote_area.unwrap(),
    };

    // Ignore clicks on the border row.
    if row <= area.y || row >= area.y + area.height - 1 {
        app.active_panel = panel;
        return;
    }

    let row_in_content = (row - area.y - 1) as usize;
    let scroll = match panel {
        Panel::Local => app.local_scroll,
        Panel::Remote => app.remote_scroll,
    };
    let entry_idx = scroll + row_in_content;

    let entry_exists = match panel {
        Panel::Local => entry_idx < app.local_entries.len(),
        Panel::Remote => entry_idx < app.remote_entries.len(),
    };

    app.active_panel = panel;

    if !entry_exists {
        return;
    }

    let is_double = app
        .last_click
        .map(|(p, idx, t)| p == panel && idx == entry_idx && t.elapsed().as_millis() < 400)
        .unwrap_or(false);

    match panel {
        Panel::Local => app.local_cursor = entry_idx,
        Panel::Remote => app.remote_cursor = entry_idx,
    }
    app.clamp_scroll();

    if is_double {
        app.last_click = None;
        app.enter_dir();
    } else {
        app.last_click = Some((panel, entry_idx, Instant::now()));
    }
}

fn do_copy(app: &mut App) {
    match app.active_panel {
        Panel::Local => {
            let entries: Vec<_> = app.selected_local_entries().into_iter().cloned().collect();
            if entries.is_empty() {
                return;
            }
            // Reuse the already-established SFTP subsystem instead of opening a new channel.
            let sftp_arc = Arc::clone(&app.ssh.sftp);
            let dest = app.remote_cwd.clone();
            app.transfer_job = Some(start_upload(sftp_arc, entries, dest));
        }
        Panel::Remote => {
            let entries: Vec<_> = app.selected_remote_entries().into_iter().cloned().collect();
            if entries.is_empty() {
                return;
            }
            let sftp_arc = Arc::clone(&app.ssh.sftp);
            let dest = app.local_cwd.clone();
            app.transfer_job = Some(start_download(sftp_arc, entries, dest));
        }
    }
}

