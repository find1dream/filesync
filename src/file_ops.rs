use crate::app::LocalEntry;
use crate::ssh::{RemoteEntry, SshClient};
use anyhow::{Context, Result};
use ssh2::Sftp;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct ProgressState {
    pub current_file: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: usize,
    pub files_total: usize,
    pub finished: bool,
    pub cancelled: bool,
    pub error: Option<String>,
}

impl ProgressState {
    pub fn percent(&self) -> u16 {
        if self.bytes_total == 0 {
            return 0;
        }
        ((self.bytes_done as f64 / self.bytes_total as f64) * 100.0).min(100.0) as u16
    }
}

pub struct TransferJob {
    pub progress: Arc<Mutex<ProgressState>>,
    pub cancel: Arc<AtomicBool>,
    pub handle: std::thread::JoinHandle<()>,
}

pub fn start_upload(
    sftp: Arc<Mutex<Sftp>>,
    local_files: Vec<LocalEntry>,
    remote_dest: PathBuf,
) -> TransferJob {
    let progress = Arc::new(Mutex::new(ProgressState {
        files_total: local_files.len(),
        ..Default::default()
    }));
    let cancel = Arc::new(AtomicBool::new(false));
    let prog_clone = Arc::clone(&progress);
    let cancel_clone = Arc::clone(&cancel);

    let handle = std::thread::spawn(move || {
        // Calculate total size inside the thread so we don't block the render loop.
        let total: u64 = local_files
            .iter()
            .map(|e| if e.is_dir { dir_size_local(&e.path) } else { e.size })
            .sum();
        prog_clone.lock().unwrap().bytes_total = total;

        for entry in &local_files {
            if cancel_clone.load(Ordering::Relaxed) {
                let mut p = prog_clone.lock().unwrap();
                p.cancelled = true;
                p.finished = true;
                return;
            }
            let dest = remote_dest.join(&entry.name);
            let result = if entry.is_dir {
                upload_dir(&sftp, &entry.path, &dest, &prog_clone, &cancel_clone)
            } else {
                upload_file(&sftp, &entry.path, &dest, &prog_clone, &cancel_clone)
            };
            if let Err(e) = result {
                let mut p = prog_clone.lock().unwrap();
                if cancel_clone.load(Ordering::Relaxed) {
                    p.cancelled = true;
                } else {
                    p.error = Some(e.to_string());
                }
                p.finished = true;
                return;
            }
            prog_clone.lock().unwrap().files_done += 1;
        }
        prog_clone.lock().unwrap().finished = true;
    });

    TransferJob { progress, cancel, handle }
}

fn upload_file(
    sftp_mtx: &Arc<Mutex<Sftp>>,
    src: &Path,
    dest: &Path,
    progress: &Arc<Mutex<ProgressState>>,
    cancel: &AtomicBool,
) -> Result<()> {
    progress.lock().unwrap().current_file =
        src.file_name().unwrap_or_default().to_string_lossy().to_string();

    let mut local_file = fs::File::open(src).with_context(|| format!("open {:?}", src))?;

    // Hold the sftp lock for the full file to avoid concurrent subsystem use.
    let sftp = sftp_mtx.lock().unwrap();
    let mut remote_file = sftp
        .create(dest)
        .with_context(|| format!("sftp create {:?}", dest))?;

    let mut buf = vec![0u8; 64 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(anyhow::anyhow!("cancelled"));
        }
        let n = local_file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        remote_file.write_all(&buf[..n])?;
        progress.lock().unwrap().bytes_done += n as u64;
    }
    Ok(())
}

fn upload_dir(
    sftp_mtx: &Arc<Mutex<Sftp>>,
    src: &Path,
    dest: &Path,
    progress: &Arc<Mutex<ProgressState>>,
    cancel: &AtomicBool,
) -> Result<()> {
    let _ = sftp_mtx.lock().unwrap().mkdir(dest, 0o755);
    for entry in fs::read_dir(src)?.filter_map(|e| e.ok()) {
        if cancel.load(Ordering::Relaxed) {
            return Err(anyhow::anyhow!("cancelled"));
        }
        // Use file_type() to avoid following symlinks into potential loops.
        let ft = entry.file_type()?;
        let child_dest = dest.join(entry.file_name());
        if ft.is_dir() {
            upload_dir(sftp_mtx, &entry.path(), &child_dest, progress, cancel)?;
        } else {
            upload_file(sftp_mtx, &entry.path(), &child_dest, progress, cancel)?;
        }
    }
    Ok(())
}

fn dir_size_local(path: &Path) -> u64 {
    let Ok(rd) = fs::read_dir(path) else { return 0 };
    rd.filter_map(|e| e.ok())
        .map(|e| {
            // Use file_type() (no extra syscall, no symlink follow) for recursion guard.
            let Ok(ft) = e.file_type() else { return 0 };
            if ft.is_dir() {
                dir_size_local(&e.path())
            } else {
                e.metadata().map(|m| m.len()).unwrap_or(0)
            }
        })
        .sum()
}

pub fn start_download(
    sftp: Arc<Mutex<Sftp>>,
    remote_files: Vec<RemoteEntry>,
    local_dest: PathBuf,
) -> TransferJob {
    let progress = Arc::new(Mutex::new(ProgressState {
        files_total: remote_files.len(),
        ..Default::default()
    }));
    let cancel = Arc::new(AtomicBool::new(false));
    let prog_clone = Arc::clone(&progress);
    let cancel_clone = Arc::clone(&cancel);

    let handle = std::thread::spawn(move || {
        // Calculate total bytes inside the thread (dir recursion avoids blocking render).
        let total: u64 = remote_files
            .iter()
            .map(|e| if e.is_dir { dir_size_remote(&sftp, &e.path) } else { e.size })
            .sum();
        prog_clone.lock().unwrap().bytes_total = total;

        for entry in &remote_files {
            if cancel_clone.load(Ordering::Relaxed) {
                let mut p = prog_clone.lock().unwrap();
                p.cancelled = true;
                p.finished = true;
                return;
            }
            prog_clone.lock().unwrap().current_file = entry.name.clone();
            let dest = local_dest.join(&entry.name);
            let result = if entry.is_dir {
                download_dir(&sftp, &entry.path, &dest, &prog_clone, &cancel_clone)
            } else {
                download_file(&sftp, &entry.path, &dest, &prog_clone, &cancel_clone)
            };
            if let Err(e) = result {
                let mut p = prog_clone.lock().unwrap();
                if cancel_clone.load(Ordering::Relaxed) {
                    p.cancelled = true;
                } else {
                    p.error = Some(e.to_string());
                }
                p.finished = true;
                return;
            }
            prog_clone.lock().unwrap().files_done += 1;
        }
        prog_clone.lock().unwrap().finished = true;
    });

    TransferJob { progress, cancel, handle }
}

fn download_file(
    sftp_mtx: &Arc<Mutex<Sftp>>,
    src: &Path,
    dest: &Path,
    progress: &Arc<Mutex<ProgressState>>,
    cancel: &AtomicBool,
) -> Result<()> {
    progress.lock().unwrap().current_file =
        src.file_name().unwrap_or_default().to_string_lossy().to_string();

    // Hold the sftp lock for the full transfer (same discipline as upload_file).
    let sftp = sftp_mtx.lock().unwrap();
    let mut remote_file =
        sftp.open(src).with_context(|| format!("sftp open {:?}", src))?;
    let mut local_file =
        std::fs::File::create(dest).with_context(|| format!("create {:?}", dest))?;
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(anyhow::anyhow!("cancelled"));
        }
        let n = remote_file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        local_file.write_all(&buf[..n])?;
        progress.lock().unwrap().bytes_done += n as u64;
    }
    Ok(())
}

fn download_dir(
    sftp_mtx: &Arc<Mutex<Sftp>>,
    src: &Path,
    dest: &Path,
    progress: &Arc<Mutex<ProgressState>>,
    cancel: &AtomicBool,
) -> Result<()> {
    std::fs::create_dir_all(dest)?;
    let entries = sftp_mtx.lock().unwrap().readdir(src)?;
    for (pb, stat) in entries {
        if cancel.load(Ordering::Relaxed) {
            return Err(anyhow::anyhow!("cancelled"));
        }
        let name = pb
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name == "." || name == ".." {
            continue;
        }
        let child_dest = dest.join(&name);
        if stat.is_dir() {
            download_dir(sftp_mtx, &pb, &child_dest, progress, cancel)?;
        } else {
            download_file(sftp_mtx, &pb, &child_dest, progress, cancel)?;
        }
    }
    Ok(())
}

fn dir_size_remote(sftp: &Arc<Mutex<Sftp>>, path: &Path) -> u64 {
    let entries = match sftp.lock().unwrap().readdir(path) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    entries
        .iter()
        .map(|(pb, stat)| {
            if stat.is_dir() { dir_size_remote(sftp, pb) } else { stat.size.unwrap_or(0) }
        })
        .sum()
}

pub fn delete_local(entry: &LocalEntry) -> Result<()> {
    if entry.is_dir {
        fs::remove_dir_all(&entry.path)
            .with_context(|| format!("remove_dir_all {:?}", entry.path))
    } else {
        fs::remove_file(&entry.path)
            .with_context(|| format!("remove_file {:?}", entry.path))
    }
}

pub fn delete_remote(ssh: &SshClient, entry: &RemoteEntry) -> Result<()> {
    if entry.is_dir {
        ssh.delete_dir(&entry.path)
    } else {
        ssh.delete_file(&entry.path)
    }
}
