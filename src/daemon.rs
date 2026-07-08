use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use crate::app::App;
use crate::audio::spawn_engine;
use crate::ipc::{DaemonServer, IpcRequest, IpcResponse};
use crate::session::read_session;

pub fn run(session_path: PathBuf) -> anyhow::Result<()> {
    let snapshot = read_session(&session_path)?;
    let _ = std::fs::remove_file(&session_path);

    let (cmd_tx, evt_rx) = spawn_engine()?;
    let mut app = App::from_snapshot(snapshot, cmd_tx.clone(), evt_rx, true);
    app.bootstrap_playback();

    let server = DaemonServer::bind()?;

    loop {
        server.accept()?;

        for event in app.drain_player_events() {
            server.broadcast_event(&event);
        }

        app.on_tick();

        let shutdown = server.handle_requests(|req| match req {
            IpcRequest::Ping => IpcResponse::Pong,
            IpcRequest::GetState => IpcResponse::State(app.snapshot()),
            IpcRequest::Command { cmd } => {
                let _ = cmd_tx.send(cmd.into_player());
                IpcResponse::Ok
            }
            IpcRequest::SetUiState {
                selected_track_path,
                list_scroll,
                search_query,
                quit_marked,
            } => {
                app.list_scroll = list_scroll;
                app.search_query = search_query;
                app.quit_marked = quit_marked;
                app.apply_filter();
                if let Some(path) = selected_track_path {
                    if let Some(pos) = app.filtered_indices.iter().position(|&i| {
                        app.queue
                            .tracks()
                            .get(i)
                            .is_some_and(|t| t.path == path)
                    }) {
                        app.list_selection = pos;
                    }
                }
                IpcResponse::Ok
            }
            IpcRequest::Shutdown => IpcResponse::Ok,
        })?;

        if shutdown {
            app.shutdown();
            break;
        }

        thread::sleep(Duration::from_millis(16));
    }

    server.cleanup();
    Ok(())
}
