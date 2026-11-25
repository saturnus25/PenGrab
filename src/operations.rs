use crate::{catalog::Catalog, packages, terminal::clean_text};
use serde::Serialize;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Install,
    Upgrade,
}

impl Operation {
    pub fn verb(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Upgrade => "upgrade",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Planned,
    Running,
    Verified,
    AlreadyInstalled,
    NoUpdate,
    NotInstalled,
    Manual,
    Unverified,
    Failed,
    Cancelled,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Self::Planned => "JobPlanned",
            Self::Running => "JobRunning",
            Self::Verified => "JobVerified",
            Self::AlreadyInstalled => "JobAlreadyInstalled",
            Self::NoUpdate => "JobNoUpdate",
            Self::NotInstalled => "JobNotInstalled",
            Self::Manual => "JobManual",
            Self::Unverified => "JobUnverified",
            Self::Failed => "JobFailed",
            Self::Cancelled => "JobCancelled",
        }
    }

    pub fn complete(self) -> bool {
        matches!(
            self,
            Self::Verified | Self::AlreadyInstalled | Self::NoUpdate
        )
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub winget_id: Option<String>,
    pub status: Status,
    pub version: Option<String>,
    pub detail: Option<String>,
    pub exit_code: Option<i32>,
}

#[derive(Clone)]
pub struct Settings {
    pub operation: Operation,
    pub unattended: bool,
    pub location: Option<PathBuf>,
}

impl Settings {
    pub fn location(path: PathBuf) -> Result<PathBuf, String> {
        if !path.is_absolute() || path.to_string_lossy().chars().any(char::is_control) {
            return Err(
                "The tool install path must be absolute and cannot contain control characters."
                    .into(),
            );
        }
        Ok(path)
    }
}

pub fn selected(
    catalog: &Catalog,
    tools: &[String],
    categories: &[String],
) -> Result<Vec<Entry>, String> {
    let mut indices = HashSet::new();
    for id in tools {
        let index = catalog
            .tools
            .iter()
            .position(|tool| &tool.id == id)
            .ok_or_else(|| format!("Unknown tool ID: {id}"))?;
        indices.insert(index);
    }
    for id in categories {
        let index = catalog
            .category_index(id)
            .ok_or_else(|| format!("Unknown category ID: {id}"))?;
        indices.extend(catalog.branch(Some(index)).iter().copied());
    }
    if indices.is_empty() {
        return Err("Select at least one tool or category.".into());
    }
    Ok(entries(catalog, &indices))
}

pub fn entries(catalog: &Catalog, indices: &HashSet<usize>) -> Vec<Entry> {
    catalog
        .tools
        .iter()
        .enumerate()
        .filter(|(index, _)| indices.contains(index))
        .map(|(_, tool)| {
            let winget_id = tool
                .winget_id
                .clone()
                .filter(|_| tool.winget_install_command().is_some());
            Entry {
                id: tool.id.clone(),
                name: tool.name.clone(),
                status: if winget_id.is_some() {
                    Status::Planned
                } else {
                    Status::Manual
                },
                winget_id,
                version: None,
                detail: None,
                exit_code: None,
            }
        })
        .collect()
}

enum Update {
    Entry(usize, Entry),
    Done,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Phase {
    Review,
    Running,
    Finished,
}

pub struct Queue {
    pub settings: Settings,
    pub entries: Vec<Entry>,
    pub phase: Phase,
    pub confirm: bool,
    pub offset: usize,
    pub edit_location: Option<String>,
    pub location_invalid: bool,
    stop: Arc<AtomicBool>,
    updates: Option<Receiver<Update>>,
}

impl Queue {
    pub fn new(settings: Settings, entries: Vec<Entry>) -> Self {
        Self {
            settings,
            entries,
            phase: Phase::Review,
            confirm: false,
            offset: 0,
            edit_location: None,
            location_invalid: false,
            stop: Arc::new(AtomicBool::new(false)),
            updates: None,
        }
    }

    pub fn start(&mut self) {
        if self.phase != Phase::Review {
            return;
        }
        let (send, recv) = mpsc::channel();
        let settings = self.settings.clone();
        let mut entries = self.entries.clone();
        let stop = Arc::clone(&self.stop);
        self.updates = Some(recv);
        self.phase = Phase::Running;
        thread::spawn(move || {
            run(&settings, &mut entries, &stop, |index, entry| {
                let _ = send.send(Update::Entry(index, entry.clone()));
            });
            let _ = send.send(Update::Done);
        });
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    pub fn stopping(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    pub fn poll(&mut self) -> bool {
        let Some(updates) = &self.updates else {
            return false;
        };
        let mut changed = false;
        loop {
            match updates.try_recv() {
                Ok(Update::Entry(index, entry)) => self.entries[index] = entry,
                Ok(Update::Done) | Err(mpsc::TryRecvError::Disconnected) => {
                    self.phase = Phase::Finished;
                    self.updates = None;
                    return true;
                }
                Err(mpsc::TryRecvError::Empty) => return changed,
            }
            changed = true;
        }
    }
}

pub fn run(
    settings: &Settings,
    entries: &mut [Entry],
    stop: &AtomicBool,
    mut progress: impl FnMut(usize, &Entry),
) {
    let inventory_cancel = AtomicBool::new(false);
    let known: HashSet<_> = entries
        .iter()
        .filter_map(|entry| entry.winget_id.as_ref())
        .map(|id| id.to_ascii_lowercase())
        .collect();
    if known.is_empty() {
        return;
    }
    let before = packages::installed(&known, &inventory_cancel);
    let updates = (settings.operation == Operation::Upgrade)
        .then(|| packages::updates(&known, &inventory_cancel));
    for (index, entry) in entries.iter_mut().enumerate() {
        let Some(id) = entry.winget_id.as_deref() else {
            continue;
        };
        if stop.load(Ordering::Relaxed) {
            entry.status = Status::Cancelled;
            progress(index, entry);
            continue;
        }
        let Ok(inventory) = &before else {
            entry.status = Status::Failed;
            entry.detail = Some("WinGet inventory could not be read.".into());
            progress(index, entry);
            continue;
        };
        let package_key = id.to_ascii_lowercase();
        let installed = inventory.get(&package_key);
        match settings.operation {
            Operation::Install if installed.is_some() => {
                entry.status = Status::AlreadyInstalled;
                entry.version = installed.cloned();
                progress(index, entry);
                continue;
            }
            Operation::Upgrade => match &updates {
                Some(Ok(available))
                    if installed.is_some() && available.contains_key(&package_key) => {}
                Some(Ok(_)) => {
                    entry.status = if installed.is_some() {
                        Status::NoUpdate
                    } else {
                        Status::NotInstalled
                    };
                    entry.version = installed.cloned();
                    progress(index, entry);
                    continue;
                }
                _ => {
                    entry.status = Status::Failed;
                    entry.detail = Some("Available updates could not be read.".into());
                    progress(index, entry);
                    continue;
                }
            },
            Operation::Install => {}
        }
        entry.status = Status::Running;
        progress(index, entry);
        match invoke(settings, id) {
            Ok(output) => {
                entry.exit_code = output.status.code();
                if output.status.success() {
                    let ids = HashSet::from([package_key.clone()]);
                    entry.version = packages::installed(&ids, &inventory_cancel)
                        .ok()
                        .and_then(|mut versions| versions.remove(&package_key));
                    entry.status = if entry
                        .version
                        .as_ref()
                        .is_some_and(|after| Some(after) != installed)
                    {
                        Status::Verified
                    } else {
                        Status::Unverified
                    };
                } else {
                    entry.status = Status::Failed;
                    entry.detail =
                        last_message(&output.stderr).or_else(|| last_message(&output.stdout));
                }
            }
            Err(error) => {
                entry.status = Status::Failed;
                entry.detail = Some(error.to_string());
            }
        }
        progress(index, entry);
    }
}

fn invoke(settings: &Settings, id: &str) -> std::io::Result<std::process::Output> {
    let mut command = Command::new("winget.exe");
    command
        .args([
            settings.operation.verb(),
            "--id",
            id,
            "--exact",
            "--source",
            "winget",
            "--silent",
            "--disable-interactivity",
        ])
        .stdin(Stdio::null());
    if settings.unattended {
        command.args([
            "--scope",
            "user",
            "--installer-type",
            "portable",
            "--skip-dependencies",
        ]);
    }
    if settings.operation == Operation::Install {
        command.arg("--no-upgrade");
    }
    if let Some(path) = &settings.location {
        command.arg("--location").arg(Path::new(path));
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command.output()
}

fn last_message(bytes: &[u8]) -> Option<String> {
    let output = String::from_utf8_lossy(bytes);
    let line = output.lines().rev().find(|line| {
        let trimmed = line.trim().trim_start_matches('\r');
        !trimmed.is_empty() && trimmed.chars().any(char::is_alphanumeric)
    })?;
    Some(clean_text(line.trim()).chars().take(300).collect())
}
