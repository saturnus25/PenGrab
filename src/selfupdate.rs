use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc::{self, Receiver},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const CHECK_INTERVAL: u64 = 86_400;

pub enum State {
    Checking,
    Current,
    Available(String),
    Unavailable,
}

pub struct ReleaseUpdate {
    folder: PathBuf,
    pending: Option<Receiver<State>>,
    pub state: State,
    pub confirm: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Check {
    version: Option<String>,
    exit_code: u8,
    retry_after_seconds: Option<u64>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Cache {
    next_check: u64,
    version: Option<String>,
}

impl Cache {
    fn read(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn state(&self) -> State {
        let Some(version) = &self.version else {
            return State::Unavailable;
        };
        if parse_version(version) > parse_version(env!("CARGO_PKG_VERSION")) {
            State::Available(version.clone())
        } else {
            State::Current
        }
    }
}

fn parse_version(text: &str) -> Option<[u32; 3]> {
    let mut numbers = text.split('.').map(str::parse::<u32>);
    let version = [
        numbers.next()?.ok()?,
        numbers.next()?.ok()?,
        numbers.next()?.ok()?,
    ];
    numbers.next().is_none().then_some(version)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn check(folder: &Path, cache_path: &Path) -> State {
    let mut cache = Cache::read(cache_path);
    if unix_now() < cache.next_check {
        return cache.state();
    }
    let lock = folder.join("config/.release-check.lock");
    if fs::metadata(&lock)
        .ok()
        .and_then(|entry| entry.modified().ok())
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age > Duration::from_secs(180))
    {
        let _ = fs::remove_file(&lock);
    }
    let Ok(guard) = OpenOptions::new().write(true).create_new(true).open(&lock) else {
        return cache.state();
    };
    cache = Cache::read(cache_path);
    if unix_now() >= cache.next_check {
        let report = command(folder)
            .args(["-Mode", "Check"])
            .output()
            .ok()
            .and_then(|output| serde_json::from_slice::<Check>(&output.stdout).ok());
        let interval = report
            .as_ref()
            .and_then(|report| report.retry_after_seconds)
            .unwrap_or(CHECK_INTERVAL)
            .max(CHECK_INTERVAL);
        if let Some(report) = report
            && report.exit_code == 0
        {
            cache.version = report
                .version
                .filter(|version| parse_version(version).is_some());
        }
        cache.next_check = unix_now().saturating_add(interval);
        if let Ok(mut file) = tempfile::NamedTempFile::new_in(folder.join("config"))
            && serde_json::to_writer(&mut file, &cache).is_ok()
        {
            let _ = file.persist(cache_path);
        }
    }
    drop(guard);
    let _ = fs::remove_file(lock);
    cache.state()
}

impl ReleaseUpdate {
    pub fn start() -> Option<Self> {
        let executable = std::env::current_exe().ok()?;
        let folder = executable.parent()?.to_owned();
        if folder.join("config/temporary").is_file()
            || !folder.join("config/release-version").is_file()
            || !folder.join("install.ps1").is_file()
        {
            return None;
        }
        let (send, pending) = mpsc::channel();
        let root = folder.clone();
        let cache = folder.join("config/release-check.json");
        let cached = Cache::read(&cache);
        if unix_now() < cached.next_check {
            return Some(Self {
                folder,
                pending: None,
                state: cached.state(),
                confirm: false,
            });
        }
        thread::spawn(move || {
            let _ = send.send(check(&root, &cache));
        });
        Some(Self {
            folder,
            pending: Some(pending),
            state: State::Checking,
            confirm: false,
        })
    }

    pub fn poll(&mut self) -> bool {
        let Some(pending) = &self.pending else {
            return false;
        };
        self.state = match pending.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Disconnected) => State::Unavailable,
            Err(mpsc::TryRecvError::Empty) => return false,
        };
        self.pending = None;
        true
    }

    pub fn apply(&self) -> std::io::Result<()> {
        let State::Available(version) = &self.state else {
            return Ok(());
        };
        let result = self.folder.join("config/last-self-update.json");
        command(&self.folder)
            .args(["-Mode", "Update", "-Version", version, "-WaitForProcessId"])
            .arg(std::process::id().to_string())
            .arg("-ResultPath")
            .arg(result)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(())
    }
}

fn command(folder: &Path) -> Command {
    let shell = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe");
    let mut command = Command::new(shell);
    command
        .args(["-NoProfile", "-File"])
        .arg(folder.join("install.ps1"))
        .args(["-InstallPath"])
        .arg(folder)
        .args(["-NonInteractive", "-Json", "-NoLaunch"])
        .current_dir(folder.parent().unwrap_or(folder))
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}
