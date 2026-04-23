use crate::util::sort_dir_first;
use anyhow::{Context, Result, bail};
use ssh2::{Session, Sftp};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct SshClient {
    pub session: Session,
    pub sftp: Arc<Mutex<Sftp>>,
}

#[derive(Clone, Debug)]
pub struct RemoteEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
}

impl SshClient {
    pub fn connect(user: &str, host: &str, password: &str) -> Result<Self> {
        let addr = format!("{}:22", host);
        let tcp = TcpStream::connect(&addr)
            .with_context(|| format!("Failed to connect to {}", addr))?;

        let mut session = Session::new()?;
        session.set_tcp_stream(tcp);
        session.handshake()?;

        session
            .userauth_password(user, password)
            .with_context(|| "Authentication failed")?;

        if !session.authenticated() {
            bail!("Authentication failed for {}@{}", user, host);
        }

        let sftp = Arc::new(Mutex::new(session.sftp()?));

        Ok(Self { session, sftp })
    }

    pub fn list_dir(&self, path: &Path) -> Result<Vec<RemoteEntry>> {
        let sftp = self.sftp.lock().unwrap();
        let mut result: Vec<RemoteEntry> = sftp
            .readdir(path)?
            .into_iter()
            .filter_map(|(pb, stat)| {
                let name = pb.file_name()?.to_string_lossy().to_string();
                if name == "." || name == ".." {
                    return None;
                }
                Some(RemoteEntry {
                    name,
                    path: pb,
                    is_dir: stat.is_dir(),
                    size: stat.size.unwrap_or(0),
                })
            })
            .collect();

        result.sort_by(|a, b| sort_dir_first(a.is_dir, &a.name, b.is_dir, &b.name));
        Ok(result)
    }

    pub fn home_dir(&self) -> Result<PathBuf> {
        let mut channel = self.session.channel_session()?;
        channel.exec("echo $HOME")?;
        let mut output = String::new();
        use std::io::Read;
        channel.read_to_string(&mut output)?;
        channel.wait_close()?;
        let path = output.trim().to_string();
        if path.is_empty() {
            Ok(PathBuf::from("/"))
        } else {
            Ok(PathBuf::from(path))
        }
    }

    pub fn delete_file(&self, path: &Path) -> Result<()> {
        self.sftp
            .lock()
            .unwrap()
            .unlink(path)
            .with_context(|| format!("Failed to delete {:?}", path))
    }

    pub fn delete_dir(&self, path: &Path) -> Result<()> {
        let entries = {
            let sftp = self.sftp.lock().unwrap();
            sftp.readdir(path)?
        };
        for (pb, stat) in entries {
            let name = pb
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if name == "." || name == ".." {
                continue;
            }
            if stat.is_dir() {
                self.delete_dir(&pb)?;
            } else {
                self.sftp.lock().unwrap().unlink(&pb)?;
            }
        }
        self.sftp
            .lock()
            .unwrap()
            .rmdir(path)
            .with_context(|| format!("Failed to remove dir {:?}", path))
    }
}
