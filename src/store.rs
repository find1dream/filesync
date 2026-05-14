use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default)]
pub struct HostStore {
    #[serde(default)]
    pub hosts: HashMap<String, HostEntry>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct HostEntry {
    pub user: String,
    pub host: String,
    pub password: String,
}

impl HostStore {
    pub fn load() -> Self {
        let path = store_path();
        fs::read_to_string(&path)
            .ok()
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let path = store_path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, toml::to_string_pretty(self)?)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    pub fn get(&self, user: &str, host: &str) -> Option<&HostEntry> {
        self.hosts.get(&format!("{}@{}", user, host))
    }

    /// Find by exact key, full "user@host" substring, or hostname substring.
    pub fn find_partial(&self, partial: &str) -> Option<&HostEntry> {
        if let Some(e) = self.hosts.get(partial) {
            return Some(e);
        }
        self.hosts.values().find(|e| {
            e.host.contains(partial) || format!("{}@{}", e.user, e.host).contains(partial)
        })
    }

    pub fn upsert(&mut self, user: &str, host: &str, password: &str) {
        self.hosts.insert(
            format!("{}@{}", user, host),
            HostEntry {
                user: user.to_string(),
                host: host.to_string(),
                password: password.to_string(),
            },
        );
    }
}

fn store_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("filesync")
        .join("hosts.toml")
}
