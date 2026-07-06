use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossbeam_channel::{unbounded, Receiver};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};

const DEBOUNCE: Duration = Duration::from_millis(800);

pub fn spawn_watcher(load_path: PathBuf) -> Receiver<()> {
    let (tx, rx) = unbounded();

    std::thread::spawn(move || {
        if let Err(e) = run_watcher(load_path, tx) {
            eprintln!("library watcher stopped: {e}");
        }
    });

    rx
}

fn run_watcher(
    load_path: PathBuf,
    tx: crossbeam_channel::Sender<()>,
) -> notify::Result<()> {
    let watch_root = if load_path.is_file() {
        load_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| load_path.clone())
    } else {
        load_path.clone()
    };

    if !watch_root.exists() {
        return Ok(());
    }

    let (notify_tx, notify_rx) = std::sync::mpsc::channel();
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = notify_tx.send(res);
        },
        Config::default(),
    )?;

    watcher.watch(&watch_root, RecursiveMode::Recursive)?;

    let mut dirty = false;
    let mut last_change = Instant::now();

    loop {
        match notify_rx.recv_timeout(Duration::from_millis(150)) {
            Ok(Ok(event)) => {
                if event.kind.is_access() {
                    continue;
                }
                if event.paths.iter().any(|p| affects_library(p, &load_path)) {
                    dirty = true;
                    last_change = Instant::now();
                }
            }
            Ok(Err(e)) => eprintln!("library watcher error: {e}"),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }

        if dirty && last_change.elapsed() >= DEBOUNCE {
            let _ = tx.send(());
            dirty = false;
        }
    }

    Ok(())
}

fn affects_library(changed: &Path, load_path: &Path) -> bool {
    if load_path.is_file() {
        return changed == load_path;
    }

    changed.starts_with(load_path)
        || load_path.starts_with(changed)
        || changed
            .parent()
            .is_some_and(|parent| parent.starts_with(load_path))
}
