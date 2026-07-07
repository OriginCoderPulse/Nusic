mod app;
mod audio;
mod error;
mod library;
mod paths;
mod player;
mod system_volume;
mod ui;

use anyhow::Context;

use app::App;
use audio::spawn_engine;
use paths::ensure_music_dir;

fn main() -> anyhow::Result<()> {
    let args: std::env::Args = std::env::args();
    if args.skip(1).any(|a| a == "--version" || a == "-V") {
        println!("nusic {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let (cmd_tx, evt_rx) = spawn_engine().context("failed to start audio engine")?;

    let music_dir = ensure_music_dir()?;
    let app = App::new(music_dir, cmd_tx, evt_rx);
    ui::run(app)
}
