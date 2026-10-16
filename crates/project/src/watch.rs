//! Watches the open folder and reports, in batches, which folders and files changed outside leaf.

use std::path::{Path, PathBuf};

/// One batch of changes, relative to the open folder.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// Folders whose listing changed (something was created, removed or renamed in them).
    pub dirs: Vec<PathBuf>,
    /// Files whose content may have changed.
    pub files: Vec<PathBuf>,
}

pub struct Watcher {
    _inner: notify::RecommendedWatcher,
}

impl Watcher {
    /// Starts watching `root`; `on_change` runs on a background thread for each batch.
    pub fn new(root: &Path, on_change: impl Fn(Changes) + Send + 'static) -> notify::Result<Self> {
        use notify::Watcher as _;
        let root_for_events = root.to_path_buf();
        let mut inner =
            notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                let Ok(event) = event else { return };
                let mut changes = Changes::default();
                for path in event.paths {
                    let Ok(rel) = path.strip_prefix(&root_for_events) else {
                        continue;
                    };
                    changes.files.push(rel.to_path_buf());
                    changes
                        .dirs
                        .push(rel.parent().map(Path::to_path_buf).unwrap_or_default());
                }
                on_change(changes);
            })?;
        inner.watch(root, notify::RecursiveMode::Recursive)?;
        Ok(Self { _inner: inner })
    }
}
