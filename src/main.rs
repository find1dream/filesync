#[cfg(windows)]
compile_error!("Windows is not supported. Build and run on Linux or macOS.");

mod app;
mod events;
mod file_ops;
mod ssh;
mod ui;
mod util;

use anyhow::Result;
use app::App;
use clap::Parser;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

#[derive(Parser, Debug)]
#[command(name = "filesync", about = "TUI SSH file manager")]
struct Args {
    /// Remote host (e.g. user@hostname or user@192.168.0.1)
    host: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let (user, host) = if let Some((u, h)) = args.host.split_once('@') {
        (u.to_string(), h.to_string())
    } else {
        eprintln!("Error: host must be in format user@hostname");
        std::process::exit(1);
    };

    let password = rpassword::prompt_password(format!("Password for {}@{}: ", user, host))?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, user, host, password);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }

    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    user: String,
    host: String,
    password: String,
) -> Result<()> {
    let mut app = App::new(user, host, password)?;
    loop {
        terminal.draw(|f| ui::render(f, &mut app))?;
        if events::handle_events(&mut app)? {
            break;
        }
    }
    Ok(())
}
