use std::{
    collections::{HashMap, HashSet},
    io::{self, Read, Seek, SeekFrom},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub enum WinGet {
    Available(String),
    Missing,
    Failed,
    TimedOut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckError {
    Missing,
    Failed,
    TimedOut,
}

pub struct Upgrade {
    pub installed: String,
    pub available: String,
}

pub type Installed = HashMap<String, String>;
pub type Updates = HashMap<String, Upgrade>;

pub enum CheckResult {
    Version(WinGet),
    Installed(Result<Installed, CheckError>),
    Updates(Result<Updates, CheckError>),
}

#[derive(Default)]
pub struct Inventory {
    pub installed: Option<Result<Installed, CheckError>>,
    pub updates: Option<Result<Updates, CheckError>>,
    pub tracked: Arc<HashSet<String>>,
}

pub enum Package<'a> {
    Unknown,
    NotDetected,
    Installed(&'a str),
    Update(&'a Upgrade),
}

impl Inventory {
    pub fn package(&self, id: Option<&str>) -> Package<'_> {
        let Some(id) = id else {
            return Package::Unknown;
        };
        let id = id.to_ascii_lowercase();
        if !self.tracked.contains(&id) {
            return Package::Unknown;
        }
        if let Some(update) = self
            .updates
            .as_ref()
            .and_then(|updates| updates.as_ref().ok())
            .and_then(|updates| updates.get(&id))
        {
            return Package::Update(update);
        }
        if let Some(installed) = self
            .installed
            .as_ref()
            .and_then(|installed| installed.as_ref().ok())
        {
            return match installed.get(&id) {
                Some(version) => Package::Installed(version),
                None => Package::NotDetected,
            };
        }
        Package::Unknown
    }
}

pub struct Scan {
    pub results: Receiver<CheckResult>,
    cancel: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>,
}

impl Scan {
    pub fn start(tracked: Arc<HashSet<String>>) -> Self {
        let (send, results) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let spawn = |query: fn(&HashSet<String>, &AtomicBool) -> CheckResult| {
            let send = send.clone();
            let cancel = Arc::clone(&cancel);
            let tracked = Arc::clone(&tracked);
            thread::spawn(move || {
                let _ = send.send(query(&tracked, &cancel));
            })
        };
        let workers = vec![
            spawn(|_, cancel| CheckResult::Version(version(cancel))),
            spawn(|ids, cancel| CheckResult::Installed(installed(ids, cancel))),
            spawn(|ids, cancel| CheckResult::Updates(updates(ids, cancel))),
        ];
        Self {
            results,
            cancel,
            workers,
        }
    }
}

impl Drop for Scan {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

pub fn version(cancel: &AtomicBool) -> WinGet {
    match run(&["--version"], Duration::from_secs(4), cancel) {
        Ok(output) => {
            let version = output.trim();
            if version.is_empty()
                || version.len() > 60
                || !version
                    .chars()
                    .all(|letter| letter.is_ascii_alphanumeric() || ".+-".contains(letter))
            {
                WinGet::Failed
            } else {
                WinGet::Available(version.to_owned())
            }
        }
        Err(CheckError::Missing) => WinGet::Missing,
        Err(CheckError::Failed) => WinGet::Failed,
        Err(CheckError::TimedOut) => WinGet::TimedOut,
    }
}

pub fn installed(ids: &HashSet<String>, cancel: &AtomicBool) -> Result<Installed, CheckError> {
    let output = list(&[], cancel)?;
    Ok(catalog_rows(&output, ids)
        .map(|(id, version, _)| (id, version.to_owned()))
        .collect())
}

pub fn updates(ids: &HashSet<String>, cancel: &AtomicBool) -> Result<Updates, CheckError> {
    let output = list(&["--upgrade-available"], cancel)?;
    Ok(parse_updates(&output, ids))
}

fn list(flags: &[&str], cancel: &AtomicBool) -> Result<String, CheckError> {
    let mut args = vec!["list", "--source", "winget", "--disable-interactivity"];
    args.extend_from_slice(flags);
    run(&args, Duration::from_secs(20), cancel)
}

fn catalog_rows<'a>(
    output: &'a str,
    ids: &'a HashSet<String>,
) -> impl Iterator<Item = (String, &'a str, Option<&'a str>)> + 'a {
    output
        .lines()
        .skip_while(|line| {
            let rule = line.trim();
            rule.len() < 8 || !rule.bytes().all(|byte| byte == b'-')
        })
        .skip(1)
        .filter_map(|line| {
            let mut cells: Vec<_> = line
                .split("  ")
                .map(str::trim)
                .filter(|cell| !cell.is_empty())
                .collect();
            if cells
                .last()
                .is_some_and(|cell| cell.eq_ignore_ascii_case("winget"))
            {
                cells.pop();
            }
            // The table is localized, but its final columns are ID, installed
            // version, and, when present, the available version.
            if let [.., _, id, installed, available] = cells.as_slice() {
                let id = id.to_ascii_lowercase();
                if ids.contains(&id) {
                    return Some((id, *installed, Some(*available)));
                }
            }
            let [.., _, id, installed] = cells.as_slice() else {
                return None;
            };
            let id = id.to_ascii_lowercase();
            ids.contains(&id).then_some((id, *installed, None))
        })
}

fn parse_updates(output: &str, ids: &HashSet<String>) -> Updates {
    catalog_rows(output, ids)
        .filter_map(|(id, installed, available)| {
            let available = available?;
            if installed == available || available.contains('…') {
                return None;
            }
            Some((
                id,
                Upgrade {
                    installed: installed.to_owned(),
                    available: available.to_owned(),
                },
            ))
        })
        .collect()
}

fn run(args: &[&str], timeout: Duration, cancel: &AtomicBool) -> Result<String, CheckError> {
    // A regular file keeps a large WinGet table from blocking on a full pipe.
    let mut output = tempfile::tempfile().map_err(|_| CheckError::Failed)?;
    let mut command = Command::new("winget.exe");
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(output.try_clone().map_err(|_| CheckError::Failed)?)
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command.spawn().map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => CheckError::Missing,
        _ => CheckError::Failed,
    })?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                output
                    .seek(SeekFrom::Start(0))
                    .map_err(|_| CheckError::Failed)?;
                let mut bytes = Vec::new();
                output
                    .take(4_194_305)
                    .read_to_end(&mut bytes)
                    .map_err(|_| CheckError::Failed)?;
                if bytes.len() > 4_194_304 {
                    return Err(CheckError::Failed);
                }
                return Ok(String::from_utf8_lossy(&bytes).into_owned());
            }
            Ok(Some(_)) => return Err(CheckError::Failed),
            Ok(None) if Instant::now() < deadline && !cancel.load(Ordering::Relaxed) => {
                thread::sleep(Duration::from_millis(20));
            }
            state => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(if state.is_ok() {
                    CheckError::TimedOut
                } else {
                    CheckError::Failed
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_matching_ids_from_a_localized_upgrade_table() {
        let ids = HashSet::from(["insecure.nmap".to_owned(), "zap.zap".to_owned()]);
        let output = "Nombre    Id              Versión  Disponible\r\n\
            -------------------------------------------------\r\n\
            Nmap      Insecure.Nmap   7.60     7.80\r\n\
            Existing  Insecure.Cutter  2.5.0\r\n\
            Other     Unrelated.Test  1.0      1.1\r\n\
            OWASP ZAP  ZAP.ZAP        2.15     2.17     winget\r\n";
        let result = parse_updates(output, &ids);
        assert_eq!(result.len(), 2);
        assert_eq!(result["insecure.nmap"].installed, "7.60");
        assert_eq!(result["zap.zap"].available, "2.17");
        assert!(parse_updates("No upgrades available.", &ids).is_empty());
        let mut installed_ids = ids;
        installed_ids.insert("insecure.cutter".to_owned());
        let installed: Vec<_> = catalog_rows(output, &installed_ids).collect();
        assert_eq!(installed.len(), 3);
        assert_eq!(installed[1], ("insecure.cutter".to_owned(), "2.5.0", None));
    }
}
