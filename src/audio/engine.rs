use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, unbounded};
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};

use crate::player::{PlaybackState, PlayerCommand, PlayerEvent};

use super::decoder::{open_decoder, open_decoder_at};

struct Engine {
    stream_handle: OutputStreamHandle,
    sink: Option<Sink>,
    state: PlaybackState,
    duration: Duration,
    play_started: Instant,
    paused_at: Duration,
    current_path: Option<PathBuf>,
}

impl Engine {
    fn new(stream_handle: OutputStreamHandle) -> Self {
        Self {
            stream_handle,
            sink: None,
            state: PlaybackState::Stopped,
            duration: Duration::ZERO,
            play_started: Instant::now(),
            paused_at: Duration::ZERO,
            current_path: None,
        }
    }

    fn position(&self) -> Duration {
        match self.state {
            PlaybackState::Playing => self.paused_at + self.play_started.elapsed(),
            PlaybackState::Paused => self.paused_at,
            PlaybackState::Stopped => Duration::ZERO,
        }
        .min(self.duration)
    }

    fn load(&mut self, path: PathBuf, evt_tx: &Sender<PlayerEvent>) {
        self.load_at(path, Duration::ZERO, evt_tx);
    }

    fn load_at(&mut self, path: PathBuf, position: Duration, evt_tx: &Sender<PlayerEvent>) {
        self.stop_internal();

        let source = match if position.is_zero() {
            open_decoder(&path)
        } else {
            open_decoder_at(&path, position)
        } {
            Ok(s) => s,
            Err(msg) => {
                let _ = evt_tx.send(PlayerEvent::Error(msg));
                return;
            }
        };

        self.duration = source.total_duration().unwrap_or(Duration::ZERO);

        let sink = match Sink::try_new(&self.stream_handle) {
            Ok(s) => s,
            Err(e) => {
                let _ = evt_tx.send(PlayerEvent::Error(format!("audio output error: {e}")));
                return;
            }
        };

        sink.set_volume(1.0);
        sink.append(source);
        self.sink = Some(sink);
        self.current_path = Some(path);
        self.paused_at = position;
        self.play_started = Instant::now();
        self.state = PlaybackState::Playing;

        let _ = evt_tx.send(PlayerEvent::Loaded {
            duration: self.duration,
        });
        let _ = evt_tx.send(PlayerEvent::Position(position));
        let _ = evt_tx.send(PlayerEvent::StateChanged(self.state));
    }

    fn play(&mut self, evt_tx: &Sender<PlayerEvent>) {
        if self.sink.is_none() {
            return;
        }
        if self.state == PlaybackState::Playing {
            return;
        }
        if let Some(sink) = &self.sink {
            sink.play();
        }
        self.play_started = Instant::now();
        self.state = PlaybackState::Playing;
        let _ = evt_tx.send(PlayerEvent::StateChanged(self.state));
    }

    fn pause(&mut self, evt_tx: &Sender<PlayerEvent>) {
        if self.state != PlaybackState::Playing {
            return;
        }
        self.paused_at = self.position();
        if let Some(sink) = &self.sink {
            sink.pause();
        }
        self.state = PlaybackState::Paused;
        let _ = evt_tx.send(PlayerEvent::StateChanged(self.state));
    }

    fn toggle(&mut self, evt_tx: &Sender<PlayerEvent>) {
        match self.state {
            PlaybackState::Playing => self.pause(evt_tx),
            PlaybackState::Paused => self.play(evt_tx),
            PlaybackState::Stopped => {}
        }
    }

    fn stop_internal(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.state = PlaybackState::Stopped;
        self.paused_at = Duration::ZERO;
        self.duration = Duration::ZERO;
        self.current_path = None;
    }

    fn stop(&mut self, evt_tx: &Sender<PlayerEvent>) {
        self.stop_internal();
        let _ = evt_tx.send(PlayerEvent::StateChanged(self.state));
    }

    fn tick(&mut self, evt_tx: &Sender<PlayerEvent>) {
        if self.state == PlaybackState::Playing {
            if let Some(sink) = &self.sink {
                if sink.empty() {
                    self.stop_internal();
                    let _ = evt_tx.send(PlayerEvent::StateChanged(self.state));
                    let _ = evt_tx.send(PlayerEvent::TrackEnded);
                    return;
                }
            }
            let _ = evt_tx.send(PlayerEvent::Position(self.position()));
        }
    }
}

pub fn spawn_engine() -> anyhow::Result<(Sender<PlayerCommand>, Receiver<PlayerEvent>)> {
    let (cmd_tx, cmd_rx) = unbounded();
    let (evt_tx, evt_rx) = unbounded();

    thread::spawn(move || {
        let stream = match OutputStream::try_default() {
            Ok(s) => s,
            Err(e) => {
                let _ = evt_tx.send(PlayerEvent::Error(format!("no audio device: {e}")));
                return;
            }
        };

        let (_stream, stream_handle) = stream;
        let mut engine = Engine::new(stream_handle);
        let evt = Arc::new(evt_tx);

        loop {
            while let Ok(cmd) = cmd_rx.try_recv() {
                match cmd {
                    PlayerCommand::Load(path) => engine.load(path, &evt),
                    PlayerCommand::LoadAt { path, position } => {
                        engine.load_at(path, position, &evt);
                    }
                    PlayerCommand::Toggle => engine.toggle(&evt),
                    PlayerCommand::Stop => engine.stop(&evt),
                    PlayerCommand::Shutdown => return,
                }
            }

            engine.tick(&evt);
            thread::sleep(Duration::from_millis(50));
        }
    });

    Ok((cmd_tx, evt_rx))
}
