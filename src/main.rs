#[cfg(windows)]
compile_error!("Windows is not supported. Build and run on Linux or macOS.");

mod app;
mod events;
mod file_ops;
mod ssh;
mod store;
mod ui;
mod util;

use anyhow::Result;
use app::App;
use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use ssh::SshClient;
use store::HostStore;
use std::io;

#[derive(Parser, Debug)]
#[command(
    name = "filesync",
    about = "TUI SSH file manager",
    after_help = "Examples:\n  filesync muse@armo10220\n  filesync 10220        (short alias after first login)"
)]
struct Args {
    /// user@host, or a partial hostname / alias saved from a previous login
    #[arg(value_name = "target")]
    target: String,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let (user, host, ssh) = connect_and_save(&args.target)?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, user, host, ssh);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }

    Ok(())
}

/// Resolve target to (user, host, SshClient), prompting for password when needed.
/// Credentials are saved to ~/.config/filesync/hosts.toml on first successful login.
fn connect_and_save(target: &str) -> Result<(String, String, SshClient)> {
    let (user, host, stored_password) = lookup_target(target)?;

    // Try stored password first.
    if let Some(ref pwd) = stored_password {
        match SshClient::connect(&user, &host, pwd) {
            Ok(ssh) => return Ok((user, host, ssh)),
            Err(e) if is_auth_error(&e) => {
                eprintln!("Saved password rejected — please re-enter.");
            }
            Err(e) => return Err(e),
        }
    }

    // Prompt, connect, save on success.
    let password = rpassword::prompt_password(format!("Password for {}@{}: ", user, host))?;
    let ssh = SshClient::connect(&user, &host, &password)?;
    let mut store = HostStore::load();
    store.upsert(&user, &host, &password);
    let _ = store.save();
    Ok((user, host, ssh))
}

/// Returns (user, host, Option<stored_password>).
fn lookup_target(target: &str) -> Result<(String, String, Option<String>)> {
    if target.contains('@') {
        let (user, host) = target.split_once('@').unwrap();
        let password = HostStore::load().get(user, host).map(|e| e.password.clone());
        Ok((user.to_string(), host.to_string(), password))
    } else {
        let store = HostStore::load();
        match store.find_partial(target) {
            Some(e) => Ok((e.user.clone(), e.host.clone(), Some(e.password.clone()))),
            None => anyhow::bail!(
                "No saved host matching '{}'. Use user@host on first login.",
                target
            ),
        }
    }
}

fn is_auth_error(e: &anyhow::Error) -> bool {
    let s = e.to_string().to_lowercase();
    s.contains("authentication") || s.contains("auth")
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    user: String,
    host: String,
    ssh: SshClient,
) -> Result<()> {
    let mut app = App::new(user, host, ssh)?;
    loop {
        terminal.draw(|f| ui::render(f, &mut app))?;
        if events::handle_events(&mut app)? {
            break;
        }
    }
    Ok(())
}
