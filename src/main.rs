mod app;
mod audio;
mod daemon;
mod error;
mod ipc;
mod library;
mod paths;
mod player;
mod session;
mod system_volume;
mod ui;

use anyhow::Context;

use app::App;
use audio::spawn_engine;
use ipc::IpcClient;
use paths::ensure_music_dir;

fn print_help() {
    let version = env!("CARGO_PKG_VERSION");
    println!(
        "\
nusic {version} — terminal music player for local files

Usage:
  nusic              Launch the UI (attach to background player if running)
  nusic --exit       Stop the background player
  nusic --version    Print version
  nusic --help       Print this help

Options:
  -h, --help         Show command help
  -V, --version      Show version

In-app help: press K inside nusic."
    );
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return Ok(());
    }

    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("nusic {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    if args.iter().any(|a| a == "--exit") {
        return ipc::stop_background();
    }

    if let Some(pos) = args.iter().position(|a| a == "--daemon") {
        let session = args
            .get(pos + 1)
            .context("--daemon requires a session file path")?;
        return daemon::run(session.into());
    }

    if ipc::is_daemon_running() {
        return run_attached();
    }

    run_standalone()
}

fn run_standalone() -> anyhow::Result<()> {
    let (cmd_tx, evt_rx) = spawn_engine().context("failed to start audio engine")?;
    let music_dir = ensure_music_dir()?;
    let app = App::new(music_dir, cmd_tx, evt_rx);
    ui::run(app)
}

fn run_attached() -> anyhow::Result<()> {
    let mut cmd_client = IpcClient::connect().context("failed to connect to background player")?;
    cmd_client.ping()?;
    let snapshot = cmd_client.get_state()?;
    let event_client = IpcClient::connect().context("failed to open event stream")?;
    let (cmd_tx, evt_rx, ipc_handle) = ipc::bridge_client(cmd_client, event_client);
    let mut app = App::from_snapshot(snapshot, cmd_tx, evt_rx, false);
    app.ipc_client = Some(ipc_handle);
    ui::run(app)
}
