//! Background file watcher for MCP server.
//!
//! When the MCP server starts, it detects the project root and spawns a
//! background task that watches source files for changes. After a quiet
//! period (debounce), it triggers `CodeXray init` as a subprocess for
//! incremental re-indexing.
//!
//! The watcher lifecycle is tied to `FileWatcherHandle`: when the handle
//! is dropped (MCP server exits), all watcher threads and tasks exit cleanly.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use notify::event::ModifyKind;
use notify::{Event, EventKind, RecursiveMode, Watcher};

use tokio::sync::mpsc;
use tokio::sync::Notify;

/// Directories excluded from file watching.
/// Uses exact name matching against path ancestors.
const EXCLUDED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "__pycache__",
    ".venv",
    "venv",
    "dist",
    "build",
    "out",
    "vendor",
    ".next",
    ".nuxt",
    "coverage",
    ".idea",
    ".vscode",
];

/// Quiet period before triggering re-index after the last file change.
const DEBOUNCE_DURATION: Duration = Duration::from_secs(8);

/// Handle that keeps the file watcher alive.
///
/// When dropped, signals the notify thread and tokio task to shut down.
pub struct FileWatcherHandle {
    /// Dropping this Sender closes the channel, causing the notify thread to exit.
    _shutdown_tx: Option<std::sync::mpsc::Sender<()>>,
    /// Notifies the tokio debounce task to stop.
    shutdown_notify: Arc<Notify>,
    /// Handle to the notify thread for clean joining.
    _watcher_thread: Option<std::thread::JoinHandle<()>>,
}

impl Drop for FileWatcherHandle {
    fn drop(&mut self) {
        // Signal the tokio task to stop
        self.shutdown_notify.notify_waiters();
        // Drop the sender → notify thread's recv() returns Err → thread exits
        drop(self._shutdown_tx.take());
        // Note: we don't join the thread in Drop to avoid blocking.
        // The thread will exit naturally when the channel closes.
    }
}

/// Check whether a path is inside an excluded directory.
fn is_in_excluded_dir(path: &Path) -> bool {
    path.ancestors()
        .skip(1) // skip the file/dir itself, check parent dirs
        .filter_map(|ancestor| ancestor.file_name())
        .any(|name| EXCLUDED_DIRS.iter().any(|excluded| name == *excluded))
}

/// Start the background file watcher for `project_root`.
///
/// Returns a `FileWatcherHandle` that keeps the watcher alive.
/// The watcher stops when the handle is dropped.
pub fn start_watcher(
    project_root: PathBuf,
) -> Result<FileWatcherHandle, Box<dyn std::error::Error>> {
    // Channel: notify thread → tokio debounce task
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<()>();

    // Channel: Drop signal → notify thread
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel::<()>();

    // Notify: Drop signal → tokio task
    let shutdown_notify = Arc::new(Notify::new());
    let shutdown_notify_clone = shutdown_notify.clone();

    // ── Notify thread ──
    let root = project_root.clone();
    let watcher_thread = std::thread::spawn(move || {
        let event_tx = event_tx; // capture for callback

        let mut watcher = match notify::recommended_watcher(
            move |res: Result<Event, notify::Error>| match res {
                Ok(event) => {
                    if should_trigger_reindex(&event) {
                        let _ = event_tx.send(());
                    }
                }
                Err(e) => {
                    eprintln!("[CodeXray] Watch error: {}", e);
                }
            },
        ) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("[CodeXray] Failed to create file watcher: {}", e);
                return;
            }
        };

        if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
            eprintln!("[CodeXray] Failed to watch {}: {}", root.display(), e);
            return;
        }

        // Block until shutdown signal
        let _ = shutdown_rx.recv();
    });

    // ── Debounce + re-index task ──
    tokio::spawn(async move {
        loop {
            // Wait for the first event
            match event_rx.recv().await {
                Some(()) => {}
                None => break, // channel closed
            }

            // Debounce: reset timer on each new event
            loop {
                tokio::select! {
                    _ = shutdown_notify_clone.notified() => return,
                    _ = tokio::time::sleep(DEBOUNCE_DURATION) => break,
                    _ = event_rx.recv() => {
                        // Another event — reset the timer by looping again
                    }
                }
            }

            // Quiet period elapsed — trigger re-index
            run_index_subprocess().await;
        }
    });

    Ok(FileWatcherHandle {
        _shutdown_tx: Some(shutdown_tx),
        shutdown_notify,
        _watcher_thread: Some(watcher_thread),
    })
}

/// Determine whether a file system event should trigger a re-index.
fn should_trigger_reindex(event: &Event) -> bool {
    // Only react to data changes (not metadata-only changes like `chmod`)
    match &event.kind {
        EventKind::Modify(ModifyKind::Data(_)) => {}
        EventKind::Create(_) | EventKind::Remove(_) => {}
        _ => return false,
    }

    // Filter out excluded directories
    for path in &event.paths {
        if !is_in_excluded_dir(path) {
            return true;
        }
    }

    false
}

/// Spawn `CodeXray init` as a subprocess to re-index the project.
async fn run_index_subprocess() {
    let bin = match std::env::current_exe() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[CodeXray] Cannot find binary: {}", e);
            return;
        }
    };

    match tokio::process::Command::new(&bin)
        .arg("init")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
    {
        Ok(output) if output.status.success() => {
            eprintln!("[CodeXray] Index updated.");
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            eprintln!("[CodeXray] Index update failed: {}", stderr.trim());
        }
        Err(e) => {
            eprintln!("[CodeXray] Failed to spawn init: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_in_excluded_dir_git() {
        assert!(is_in_excluded_dir(Path::new(
            "project/.git/config"
        )));
    }

    #[test]
    fn test_is_in_excluded_dir_target() {
        assert!(is_in_excluded_dir(Path::new(
            "project/target/debug/foo.rs"
        )));
    }

    #[test]
    fn test_is_in_excluded_dir_node_modules() {
        assert!(is_in_excluded_dir(Path::new(
            "node_modules/foo/index.js"
        )));
    }

    #[test]
    fn test_is_in_excluded_dir_src_not_excluded() {
        assert!(!is_in_excluded_dir(Path::new(
            "project/src/main.rs"
        )));
    }

    #[test]
    fn test_is_in_excluded_dir_similar_name_not_excluded() {
        assert!(!is_in_excluded_dir(Path::new(
            "project/mytarget/foo.rs"
        )));
    }

    #[test]
    fn test_is_in_excluded_dir_nested_node_modules() {
        assert!(is_in_excluded_dir(Path::new(
            "project/packages/foo/node_modules/bar/index.js"
        )));
    }
}
