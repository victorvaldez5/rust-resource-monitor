//! Works out where each running program came from.
//!
//! Package manager queries are slow, so a worker thread does them. `Trusts` hands
//! it each program file once and returns whatever it has finished so far; until
//! then a process is `Trust::Unknown`.

use std::collections::{HashMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use super::{filehash, pkg};
use crate::security::{Provenance, Trust, classify};

/// Identifies one version of one file, so a replaced binary is looked at again.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
struct FileKey {
    dev: u64,
    ino: u64,
    size: u64,
    mtime: i64,
}

impl FileKey {
    /// Of the file the process is actually running, even if it was deleted since.
    fn of_pid(pid: u32) -> Option<Self> {
        let m = std::fs::metadata(format!("/proc/{pid}/exe")).ok()?;
        Some(Self {
            dev: m.dev(),
            ino: m.ino(),
            size: m.size(),
            mtime: m.mtime(),
        })
    }
}

struct Job {
    key: FileKey,
    pid: u32,
    /// As `/proc/<pid>/exe` links to it, so it may end in " (deleted)".
    exe: PathBuf,
}

type Results = Arc<Mutex<HashMap<FileKey, Arc<Provenance>>>>;

pub struct Trusts {
    jobs: Sender<Job>,
    results: Results,
    /// Each running pid's program path and file, so `/proc` is only read for new processes.
    by_pid: HashMap<u32, (PathBuf, Option<FileKey>)>,
    queued: HashSet<FileKey>,
    unreadable: Arc<Provenance>,
}

impl Trusts {
    pub fn new() -> Self {
        let (jobs, rx) = channel();
        let results = Results::default();
        let shared = results.clone();
        std::thread::Builder::new()
            .name("trust".into())
            .spawn(move || worker(rx, shared))
            .expect("failed to start trust thread");
        Self {
            jobs,
            results,
            by_pid: HashMap::new(),
            queued: HashSet::new(),
            unreadable: Arc::new(Provenance {
                trust: Trust::Unknown,
                reason: "The program file isn't readable. Other users' processes, and protected programs (setuid or with capabilities), can only be checked when run as root.".into(),
                ..Default::default()
            }),
        }
    }

    /// The verdict for a process, if the worker has reached it. `exe` is None for
    /// kernel threads and for processes whose `/proc/<pid>/exe` can't be read.
    pub fn lookup(
        &mut self,
        pid: u32,
        exe: Option<&Path>,
        kernel: bool,
    ) -> (Trust, Option<Arc<Provenance>>) {
        if kernel {
            return (Trust::Kernel, None);
        }
        let Some(exe) = exe else {
            return (Trust::Unknown, Some(self.unreadable.clone()));
        };
        let entry = self.by_pid.get(&pid).filter(|(path, _)| path == exe);
        let key = match entry {
            Some((_, key)) => *key,
            None => {
                let key = FileKey::of_pid(pid);
                self.by_pid.insert(pid, (exe.to_path_buf(), key));
                key
            }
        };
        let Some(key) = key else {
            return (Trust::Unknown, Some(self.unreadable.clone()));
        };
        if let Some(found) = self.results.lock().unwrap().get(&key) {
            return (found.trust, Some(found.clone()));
        }
        if self.queued.insert(key) {
            let job = Job {
                key,
                pid,
                exe: exe.to_path_buf(),
            };
            // The worker only goes away at exit.
            let _ = self.jobs.send(job);
        }
        (Trust::Unknown, None)
    }

    /// Forgets processes that have exited.
    pub fn retain(&mut self, live: &HashSet<u32>) {
        self.by_pid.retain(|pid, _| live.contains(pid));
    }

    /// Program files the worker hasn't finished yet.
    pub fn pending(&self) -> usize {
        self.queued
            .len()
            .saturating_sub(self.results.lock().unwrap().len())
    }
}

fn worker(rx: Receiver<Job>, results: Results) {
    let backend = pkg::detect();
    let mut cache = pkg::Cache::default();
    for job in rx {
        let provenance = evaluate(&job, backend, &mut cache);
        results
            .lock()
            .unwrap()
            .insert(job.key, Arc::new(provenance));
    }
}

fn evaluate(job: &Job, backend: Option<pkg::Backend>, cache: &mut pkg::Cache) -> Provenance {
    let raw = job.exe.to_string_lossy();
    let deleted = raw.ends_with(" (deleted)");
    let path = raw.trim_end_matches(" (deleted)");
    let Some(backend) = backend else {
        return Provenance {
            trust: Trust::Unknown,
            reason: "No supported package manager (rpm, dpkg or pacman) found.".into(),
            ..Default::default()
        };
    };
    // A program running from memory has no path a package could own.
    let owner = if path.starts_with('/') && !path.starts_with("/memfd:") {
        backend.owner(path, cache)
    } else {
        None
    };
    let (trust, reason) = classify(path, deleted, owner.as_ref());
    // Hashing is for files worth a second opinion; the package already vouches for the rest.
    let sha256 = if matches!(trust, Trust::Verified | Trust::Stale) {
        None
    } else {
        // The proc link works even when the file was deleted or replaced.
        filehash::sha256(Path::new(&format!("/proc/{}/exe", job.pid))).or_else(|| {
            (!deleted)
                .then(|| filehash::sha256(Path::new(path)))
                .flatten()
        })
    };
    Provenance {
        trust,
        reason,
        package: owner.map(|o| o.package),
        sha256,
    }
}
