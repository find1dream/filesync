use crate::app::{App, AppMode, ConfirmAction, Panel};

const HELP_ITEMS: &[(&str, &str)] = &[
    ("[Tab]", "Switch"),
    ("[↑↓/jk]", "Navigate"),
    ("[Enter/→]", "Open"),
    ("[←/BS]", "Back"),
    ("[Space]", "Select"),
    ("[c]", "Copy"),
    ("[d]", "Delete"),
    ("[H]", "Hidden"),
    ("[r]", "Refresh"),
    ("[q]", "Quit"),
];
use bytesize::ByteSize;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph},
};
use std::collections::HashSet;

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let transferring = app.transfer_job.is_some();
    let progress_height = if transferring || app.progress.bytes_total > 0 { 3u16 } else { 0 };
    let help_height = if transferring { 3u16 } else { needed_help_height(area.width) };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(progress_height),
            Constraint::Length(help_height),
        ])
        .split(area);

    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[0]);

    // Update visible_height so cursor methods clamp correctly after this frame.
    app.visible_height = panels[0].height.saturating_sub(2).max(1) as usize;
    app.clamp_scroll();

    render_panel(f, app, panels[0], Panel::Local);
    render_panel(f, app, panels[1], Panel::Remote);

    if progress_height > 0 {
        render_progress(f, app, chunks[1]);
    }

    render_help(f, chunks[2], transferring);

    match app.mode {
        AppMode::Confirm(action) => render_confirm(f, app, action, area),
        AppMode::Error => render_popup(f, " Error ", &app.error_msg.clone(), Color::Red, area),
        AppMode::Status => render_popup(f, " Status ", &app.status_msg.clone(), Color::Green, area),
        AppMode::Browse => {}
    }
}

fn render_panel(f: &mut Frame, app: &App, area: Rect, panel: Panel) {
    let is_active = app.active_panel == panel;
    let border_style = if is_active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let (title, scroll, cursor, selected) = match panel {
        Panel::Local => (
            format!(" Local: {} ", app.local_cwd.display()),
            app.local_scroll,
            app.local_cursor,
            &app.local_selected,
        ),
        Panel::Remote => (
            format!(" Remote [{}]: {} ", app.remote_host_label, app.remote_cwd.display()),
            app.remote_scroll,
            app.remote_cursor,
            &app.remote_selected,
        ),
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let items: Vec<ListItem> = match panel {
        Panel::Local => app
            .local_entries
            .iter()
            .enumerate()
            .skip(scroll)
            .take(inner.height as usize)
            .map(|(i, e)| make_file_item(&e.name, e.is_dir, e.size, i, cursor, is_active, selected))
            .collect(),
        Panel::Remote => app
            .remote_entries
            .iter()
            .enumerate()
            .skip(scroll)
            .take(inner.height as usize)
            .map(|(i, e)| make_file_item(&e.name, e.is_dir, e.size, i, cursor, is_active, selected))
            .collect(),
    };

    f.render_widget(List::new(items), inner);
}

fn make_file_item(
    name: &str,
    is_dir: bool,
    size: u64,
    idx: usize,
    cursor: usize,
    panel_active: bool,
    selected: &HashSet<usize>,
) -> ListItem<'static> {
    let is_cursor = idx == cursor && panel_active;
    let is_selected = selected.contains(&idx);

    let prefix = if is_dir { " ▶ " } else { "   " };
    let base_fg = if is_dir { Color::Blue } else { Color::White };
    let sel_mark = if is_selected { "*" } else { " " };
    let size_str = if is_dir {
        "       <DIR>".to_string()
    } else {
        format!("{:>12}", ByteSize(size).to_string())
    };

    let display_name = &name[..name.len().min(30)];
    let text = format!("{}{}{:<30}  {}", sel_mark, prefix, display_name, size_str);

    let mut style = Style::default().fg(if is_selected { Color::Yellow } else { base_fg });
    if is_cursor {
        style = style.bg(Color::DarkGray).add_modifier(Modifier::BOLD);
    }

    ListItem::new(Line::from(Span::styled(text, style)))
}

fn render_progress(f: &mut Frame, app: &App, area: Rect) {
    let p = &app.progress;
    let label = if p.bytes_total > 0 {
        format!(
            " {} [{}/{}]  {}/{} files",
            p.current_file,
            ByteSize(p.bytes_done),
            ByteSize(p.bytes_total),
            p.files_done,
            p.files_total,
        )
    } else {
        " Transferring...".to_string()
    };

    let gauge = Gauge::default()
        .block(Block::default().title(" Progress ").borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Green).bg(Color::Black))
        .percent(p.percent())
        .label(label);

    f.render_widget(gauge, area);
}

fn needed_help_height(width: u16) -> u16 {
    let available = width.saturating_sub(2) as usize;
    let total: usize = HELP_ITEMS
        .iter()
        .enumerate()
        .map(|(i, &(k, d))| {
            k.chars().count() + 1 + d.chars().count() + if i + 1 < HELP_ITEMS.len() { 2 } else { 0 }
        })
        .sum();
    if total <= available { 3 } else { 4 }
}

fn render_help(f: &mut Frame, area: Rect, transferring: bool) {
    let block = Block::default().borders(Borders::ALL);
    let key_style = Style::default().fg(Color::Yellow);

    if transferring {
        let line = Line::from(vec![
            Span::styled("[Esc]", key_style),
            Span::raw(" Cancel  "),
            Span::styled("[q]", key_style),
            Span::raw(" Quit"),
        ]);
        f.render_widget(Paragraph::new(line).block(block).alignment(Alignment::Left), area);
        return;
    }

    // Fill line 1; overflow to line 2 when width is exhausted.
    let available = area.width.saturating_sub(2) as usize;
    let mut line1: Vec<Span<'static>> = Vec::new();
    let mut line2: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    let mut on_line2 = false;

    for (i, &(key, desc)) in HELP_ITEMS.iter().enumerate() {
        let sep = if i + 1 < HELP_ITEMS.len() { "  " } else { "" };
        let width = key.chars().count() + 1 + desc.chars().count() + sep.len();
        if !on_line2 && !line1.is_empty() && used + width > available {
            on_line2 = true;
            used = 0;
        }
        if on_line2 {
            line2.push(Span::styled(key, key_style));
            line2.push(Span::raw(format!(" {}{}", desc, sep)));
        } else {
            line1.push(Span::styled(key, key_style));
            line1.push(Span::raw(format!(" {}{}", desc, sep)));
        }
        used += width;
    }

    let mut lines = vec![Line::from(line1)];
    if !line2.is_empty() {
        lines.push(Line::from(line2));
    }
    f.render_widget(Paragraph::new(lines).block(block).alignment(Alignment::Left), area);
}

fn active_selection_count(app: &App) -> usize {
    match app.active_panel {
        Panel::Local => {
            if app.local_selected.is_empty() { 1 } else { app.local_selected.len() }
        }
        Panel::Remote => {
            if app.remote_selected.is_empty() { 1 } else { app.remote_selected.len() }
        }
    }
}

fn render_confirm(f: &mut Frame, app: &App, action: ConfirmAction, area: Rect) {
    let count = active_selection_count(app);
    let (title, body) = match action {
        ConfirmAction::Delete => (
            " Confirm Delete ",
            format!("Delete {} item(s)?  [y] Yes  [any] Cancel", count),
        ),
        ConfirmAction::Copy => {
            let (src, dst) = match app.active_panel {
                Panel::Local => ("local", "remote"),
                Panel::Remote => ("remote", "local"),
            };
            (
                " Confirm Copy ",
                format!("Copy {} item(s) from {} → {}?  [y] Yes  [any] Cancel", count, src, dst),
            )
        }
    };
    render_popup(f, title, &body, Color::Yellow, area);
}

fn render_popup(f: &mut Frame, title: &str, body: &str, color: Color, area: Rect) {
    let popup_area = centered_rect(60, 20, area);
    f.render_widget(Clear, popup_area);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color));
    f.render_widget(
        Paragraph::new(body).block(block).alignment(Alignment::Center),
        popup_area,
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vert[1])[1]
}
