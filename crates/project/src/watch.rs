//! Watches the open folder and reports, in batches, which folders and files changed outside subwy.

use std::{
    collections::BTreeSet,
    ffi::OsStr,
    path::{Component, Path, PathBuf},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    thread,
    time::{Duration, Instant},
};

use notify::{Event, EventKind, event::ModifyKind};
use unicode_normalization::UnicodeNormalization;

/// One batch of changes, relative to the open folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// Folders whose listing changed (something was created, removed or renamed in them).
    pub dirs: Vec<PathBuf>,
    /// Paths that were written, created, removed or renamed: an open file may need reading
    /// again. Folders can appear here too; only paths that match a tab matter.
    pub files: Vec<PathBuf>,
}

/// How long the folder must stay quiet before a batch goes out, so one save or one
/// `git checkout` becomes one refresh instead of dozens.
const QUIET: Duration = Duration::from_millis(100);
/// A batch goes out after this long even while events keep coming, so a busy folder still
/// updates.
const MAX_WAIT: Duration = Duration::from_secs(1);

pub struct Watcher {
    _inner: notify::RecommendedWatcher,
}

impl Watcher {
    /// Starts watching `root`; `on_change` runs on a background thread once per batch.
    /// Dropping the watcher stops both the watching and that thread.
    pub fn new(root: &Path, on_change: impl Fn(Changes) + Send + 'static) -> notify::Result<Self> {
        use notify::Watcher as _;
        let roots = Roots::new(root);
        let (sender, receiver) = mpsc::channel();
        let mut inner =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if let Ok(event) = event {
                    let _ = sender.send(event);
                }
            })?;
        inner.watch(root, notify::RecursiveMode::Recursive)?;
        thread::Builder::new()
            .name("subwy-watch".into())
            .spawn(move || debounce(&receiver, &roots, &on_change))
            .map_err(notify::Error::io)?;
        Ok(Self { _inner: inner })
    }
}

/// Gathers events until the folder has been quiet for `QUIET` and hands over one batch.
/// Returns once the watcher is dropped, which drops the sending side.
fn debounce(events: &Receiver<Event>, roots: &Roots, on_change: &impl Fn(Changes)) {
    while let Ok(first) = events.recv() {
        let started = Instant::now();
        let mut batch = Batch::default();
        batch.add(roots, &first);
        loop {
            let left = MAX_WAIT.saturating_sub(started.elapsed());
            if left.is_zero() {
                break;
            }
            match events.recv_timeout(QUIET.min(left)) {
                Ok(event) => batch.add(roots, &event),
                Err(RecvTimeoutError::Timeout) => break,
                // Nobody is listening any more.
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        if let Some(changes) = batch.finish() {
            on_change(changes);
        }
    }
}

/// The open folder as given and as the OS reports it. FSEvents reports real paths
/// (`/private/tmp/..` for `/tmp/..`), and both are compared in NFC like the tree's own paths.
struct Roots(Vec<PathBuf>);

impl Roots {
    fn new(root: &Path) -> Self {
        let mut roots = vec![nfc(root)];
        if let Ok(real) = std::fs::canonicalize(root) {
            roots.push(nfc(&real));
        }
        Self(roots)
    }

    fn relative(&self, path: &Path) -> Option<PathBuf> {
        let path = nfc(path);
        self.0
            .iter()
            .find_map(|root| path.strip_prefix(root).ok())
            .map(Path::to_path_buf)
    }
}

fn nfc(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().nfc().collect::<String>())
}

#[derive(Default)]
struct Batch {
    dirs: BTreeSet<PathBuf>,
    files: BTreeSet<PathBuf>,
}

impl Batch {
    fn add(&mut self, roots: &Roots, event: &Event) {
        // Reading a file does not change it.
        if event.kind.is_access() {
            return;
        }
        let listing = matches!(
            event.kind,
            EventKind::Create(_)
                | EventKind::Remove(_)
                | EventKind::Modify(ModifyKind::Name(_))
                | EventKind::Any
                | EventKind::Other
        );
        for path in &event.paths {
            let Some(rel) = roots.relative(path) else {
                continue;
            };
            // Git rewrites its own files on every command, and the tree never shows them.
            if rel
                .components()
                .any(|c| c == Component::Normal(OsStr::new(".git")))
            {
                continue;
            }
            // Events were dropped under this folder; its listing has to be read again.
            if event.need_rescan() {
                self.dirs.insert(rel.clone());
            }
            if listing && let Some(parent) = rel.parent() {
                self.dirs.insert(parent.to_path_buf());
            }
            if !rel.as_os_str().is_empty() {
                self.files.insert(rel);
            }
        }
    }

    fn finish(self) -> Option<Changes> {
        if self.dirs.is_empty() && self.files.is_empty() {
            return None;
        }
        Some(Changes {
            dirs: self.dirs.into_iter().collect(),
            files: self.files.into_iter().collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use notify::event::{AccessKind, CreateKind, DataChange, Flag, RemoveKind, RenameMode};

    use super::*;

    const ROOT: &str = "/project";

    fn event(kind: EventKind, path: &str) -> Event {
        Event::new(kind).add_path(PathBuf::from(path))
    }

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    fn batch(events: &[Event]) -> Option<Changes> {
        let roots = Roots(vec![PathBuf::from(ROOT), PathBuf::from("/private/project")]);
        let mut batch = Batch::default();
        for event in events {
            batch.add(&roots, event);
        }
        batch.finish()
    }

    #[test]
    fn sorts_events_into_folders_to_reread_and_files_to_check() {
        let changes = batch(&[
            event(EventKind::Create(CreateKind::File), "/project/docs/new.md"),
            event(
                EventKind::Modify(ModifyKind::Data(DataChange::Content)),
                "/project/readme.md",
            ),
            event(
                EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
                "/project/src/old.rs",
            ),
            event(EventKind::Remove(RemoveKind::Folder), "/project/build"),
        ])
        .unwrap();
        assert_eq!(changes.dirs, paths(&["", "docs", "src"]));
        assert_eq!(
            changes.files,
            paths(&["build", "docs/new.md", "readme.md", "src/old.rs"])
        );
    }

    #[test]
    fn repeated_events_come_out_once() {
        let modify = EventKind::Modify(ModifyKind::Data(DataChange::Content));
        let changes = batch(&[
            event(EventKind::Create(CreateKind::File), "/project/a.md"),
            event(modify, "/project/a.md"),
            event(modify, "/project/a.md"),
            event(EventKind::Create(CreateKind::File), "/project/b.md"),
        ])
        .unwrap();
        assert_eq!(changes.dirs, paths(&[""]));
        assert_eq!(changes.files, paths(&["a.md", "b.md"]));
    }

    #[test]
    fn ignores_git_internals_reads_and_paths_outside() {
        assert_eq!(
            batch(&[
                event(
                    EventKind::Create(CreateKind::File),
                    "/project/.git/index.lock"
                ),
                event(EventKind::Remove(RemoveKind::File), "/project/.git"),
                event(
                    EventKind::Modify(ModifyKind::Data(DataChange::Content)),
                    "/project/vendor/lib/.git/HEAD",
                ),
                event(EventKind::Access(AccessKind::Any), "/project/a.md"),
                event(EventKind::Create(CreateKind::File), "/elsewhere/a.md"),
                event(EventKind::Create(CreateKind::File), "/projectile/a.md"),
            ]),
            None
        );
        // `.gitignore` and `.github` are ordinary files and folders.
        let changes = batch(&[
            event(EventKind::Create(CreateKind::File), "/project/.gitignore"),
            event(EventKind::Create(CreateKind::Folder), "/project/.github"),
        ])
        .unwrap();
        assert_eq!(changes.files, paths(&[".github", ".gitignore"]));
    }

    #[test]
    fn paths_reported_under_the_real_root_or_in_nfd_match_the_tree() {
        let nfd = "/private/project/\u{1112}\u{1161}\u{11ab}\u{1100}\u{1173}\u{11af}.md";
        let changes = batch(&[event(EventKind::Create(CreateKind::File), nfd)]).unwrap();
        assert_eq!(changes.files, paths(&["\u{d55c}\u{ae00}.md"]));
    }

    #[test]
    fn a_rescan_rereads_the_folder_itself() {
        let rescan = Event::new(EventKind::Other)
            .set_flag(Flag::Rescan)
            .add_path(PathBuf::from("/project/docs"));
        let changes = batch(&[rescan]).unwrap();
        assert_eq!(changes.dirs, paths(&["", "docs"]));
    }

    fn create(path: &str) -> Event {
        event(EventKind::Create(CreateKind::File), path)
    }

    /// Runs `debounce` on its own thread; batches come back on the returned channel.
    fn start_debounce() -> (
        mpsc::Sender<Event>,
        Receiver<Changes>,
        thread::JoinHandle<()>,
    ) {
        let (events, receiver) = mpsc::channel();
        let (batches, results) = mpsc::channel();
        let roots = Roots(vec![PathBuf::from(ROOT)]);
        let handle = thread::spawn(move || {
            debounce(&receiver, &roots, &move |changes| {
                let _ = batches.send(changes);
            })
        });
        (events, results, handle)
    }

    #[test]
    fn a_burst_of_events_becomes_one_batch() {
        let (events, results, handle) = start_debounce();
        for name in ["/project/a.md", "/project/b.md", "/project/a.md"] {
            events.send(create(name)).unwrap();
        }
        let first = results.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(first.files, paths(&["a.md", "b.md"]));

        events.send(create("/project/c.md")).unwrap();
        let second = results.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(second.files, paths(&["c.md"]));

        // Dropping the sender, as dropping the watcher does, ends the thread.
        drop(events);
        handle.join().unwrap();
        assert!(results.try_recv().is_err());
    }

    #[test]
    fn a_batch_without_anything_to_report_is_not_sent() {
        let (events, results, handle) = start_debounce();
        events.send(create("/project/.git/HEAD")).unwrap();
        events.send(create("/project/a.md")).unwrap();
        let only = results.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(only.files, paths(&["a.md"]));
        events.send(create("/project/.git/index")).unwrap();
        thread::sleep(QUIET * 3);
        drop(events);
        handle.join().unwrap();
        assert!(results.try_recv().is_err());
    }

    #[test]
    fn a_folder_that_never_goes_quiet_still_reports() {
        let (events, results, handle) = start_debounce();
        let writer = thread::spawn(move || {
            let until = Instant::now() + MAX_WAIT * 2;
            while Instant::now() < until {
                let _ = events.send(create("/project/log.txt"));
                thread::sleep(QUIET / 4);
            }
        });
        // The first batch closes at `MAX_WAIT`, while events are still coming.
        let first = results.recv_timeout(MAX_WAIT + MAX_WAIT / 2);
        assert_eq!(first.unwrap().files, paths(&["log.txt"]));
        writer.join().unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn watches_a_real_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        let (batches, results) = mpsc::channel();
        let _watcher = Watcher::new(root, move |changes| {
            let _ = batches.send(changes);
        })
        .unwrap();
        // The temporary folder sits under `/var`, which FSEvents reports as `/private/var`.
        std::fs::create_dir(root.join("notes")).unwrap();
        std::fs::write(root.join("notes/a.md"), "hello").unwrap();
        std::fs::write(root.join(".git/HEAD"), "ref: refs/heads/main").unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut seen = Changes::default();
        while !seen.files.contains(&PathBuf::from("notes/a.md")) {
            let left = deadline.saturating_duration_since(Instant::now());
            let changes = results.recv_timeout(left).expect("no change reported");
            seen.dirs.extend(changes.dirs);
            seen.files.extend(changes.files);
        }
        assert!(seen.dirs.contains(&PathBuf::from("notes")));
        for path in seen.dirs.iter().chain(&seen.files) {
            assert!(path.is_relative(), "{path:?}");
            assert!(!path.starts_with(".git"), "{path:?}");
        }
    }
}
