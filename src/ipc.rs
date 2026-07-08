use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, unbounded};
use serde::{Deserialize, Serialize};

use crate::paths::{daemon_pid_path, ensure_runtime_dir};
use crate::player::{PlaybackState, PlayerCommand, PlayerEvent};
use crate::session::SessionSnapshot;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcRequest {
    Ping,
    GetState,
    Command { cmd: IpcCommand },
    SetUiState {
        selected_track_path: Option<PathBuf>,
        list_scroll: usize,
        search_query: String,
        quit_marked: bool,
    },
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcCommand {
    Load { path: PathBuf },
    LoadAt { path: PathBuf, position_ms: u64 },
    Toggle,
    Stop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IpcEnvelope {
    Request(IpcRequest),
    Response(IpcResponse),
    Event(IpcEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcResponse {
    Pong,
    State(SessionSnapshot),
    Ok,
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcEvent {
    Loaded { duration_ms: u64 },
    StateChanged { state: PlaybackState },
    Position { position_ms: u64 },
    TrackEnded,
    Error { message: String },
}

impl IpcCommand {
    pub fn into_player(self) -> PlayerCommand {
        match self {
            Self::Load { path } => PlayerCommand::Load(path),
            Self::LoadAt { path, position_ms } => PlayerCommand::LoadAt {
                path,
                position: Duration::from_millis(position_ms),
            },
            Self::Toggle => PlayerCommand::Toggle,
            Self::Stop => PlayerCommand::Stop,
        }
    }
}

impl From<PlayerEvent> for IpcEvent {
    fn from(event: PlayerEvent) -> Self {
        match event {
            PlayerEvent::Loaded { duration } => Self::Loaded {
                duration_ms: duration.as_millis() as u64,
            },
            PlayerEvent::StateChanged(state) => Self::StateChanged { state },
            PlayerEvent::Position(pos) => Self::Position {
                position_ms: pos.as_millis() as u64,
            },
            PlayerEvent::TrackEnded => Self::TrackEnded,
            PlayerEvent::Error(msg) => Self::Error { message: msg },
        }
    }
}

impl From<IpcEvent> for PlayerEvent {
    fn from(event: IpcEvent) -> Self {
        match event {
            IpcEvent::Loaded { duration_ms } => Self::Loaded {
                duration: Duration::from_millis(duration_ms),
            },
            IpcEvent::StateChanged { state } => Self::StateChanged(state),
            IpcEvent::Position { position_ms } => Self::Position(Duration::from_millis(
                position_ms,
            )),
            IpcEvent::TrackEnded => Self::TrackEnded,
            IpcEvent::Error { message } => Self::Error(message),
        }
    }
}

pub fn player_command_to_ipc(cmd: &PlayerCommand) -> Option<IpcCommand> {
    match cmd {
        PlayerCommand::Load(path) => Some(IpcCommand::Load { path: path.clone() }),
        PlayerCommand::LoadAt { path, position } => Some(IpcCommand::LoadAt {
            path: path.clone(),
            position_ms: position.as_millis() as u64,
        }),
        PlayerCommand::Toggle => Some(IpcCommand::Toggle),
        PlayerCommand::Stop => Some(IpcCommand::Stop),
        PlayerCommand::Shutdown => None,
    }
}

fn daemon_port_path() -> PathBuf {
    crate::paths::daemon_port_path()
}

fn read_daemon_addr() -> anyhow::Result<SocketAddr> {
    let data = std::fs::read_to_string(daemon_port_path())?;
    Ok(data.trim().parse()?)
}

fn write_daemon_addr(addr: SocketAddr) -> anyhow::Result<()> {
    ensure_runtime_dir()?;
    std::fs::write(daemon_port_path(), addr.to_string())?;
    Ok(())
}

pub fn is_daemon_running() -> bool {
    let pid_path = daemon_pid_path();
    let Ok(data) = std::fs::read_to_string(&pid_path) else {
        return false;
    };
    let Ok(pid) = data.trim().parse::<i32>() else {
        return false;
    };
    daemon_alive(pid)
}

#[cfg(unix)]
fn daemon_alive(pid: i32) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid.to_string().as_str()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn daemon_alive(_pid: i32) -> bool {
    false
}

pub fn stop_background() -> anyhow::Result<()> {
    if !is_daemon_running() {
        cleanup_daemon_files();
        println!("nusic: no background player running");
        return Ok(());
    }

    match IpcClient::connect() {
        Ok(mut client) => {
            client.shutdown()?;
        }
        Err(e) => {
            cleanup_daemon_files();
            anyhow::bail!("failed to connect to background player: {e}");
        }
    }

    for _ in 0..50 {
        if !is_daemon_running() {
            cleanup_daemon_files();
            println!("nusic: background player stopped");
            return Ok(());
        }
        thread::sleep(Duration::from_millis(100));
    }

    anyhow::bail!("background player did not stop")
}

fn cleanup_daemon_files() {
    let _ = std::fs::remove_file(daemon_pid_path());
    let _ = std::fs::remove_file(daemon_port_path());
}

pub fn wait_for_daemon(timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if is_daemon_running() {
            if read_daemon_addr().is_ok() {
                return true;
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}

fn write_envelope<W: Write>(writer: &mut W, msg: &IpcEnvelope) -> anyhow::Result<()> {
    let data = serde_json::to_vec(msg)?;
    let len = (data.len() as u32).to_be_bytes();
    writer.write_all(&len)?;
    writer.write_all(&data)?;
    writer.flush()?;
    Ok(())
}

fn read_envelope<R: Read>(reader: &mut R) -> std::io::Result<IpcEnvelope> {
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 16 * 1024 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "ipc message too large",
        ));
    }
    let mut data = vec![0u8; len];
    reader.read_exact(&mut data)?;
    serde_json::from_slice(&data).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

fn read_envelope_anyhow<R: Read>(reader: &mut R) -> anyhow::Result<IpcEnvelope> {
    read_envelope(reader).map_err(anyhow::Error::from)
}

pub struct IpcClient {
    stream: TcpStream,
}

impl IpcClient {
    pub fn connect() -> anyhow::Result<Self> {
        let addr = read_daemon_addr()?;
        let stream = TcpStream::connect(addr)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        Ok(Self { stream })
    }

    fn call(&mut self, req: IpcRequest) -> anyhow::Result<IpcResponse> {
        write_envelope(&mut self.stream, &IpcEnvelope::Request(req))?;
        loop {
            match read_envelope_anyhow(&mut self.stream)? {
                IpcEnvelope::Response(resp) => return Ok(resp),
                IpcEnvelope::Event(_) => continue,
                IpcEnvelope::Request(_) => anyhow::bail!("unexpected ipc request on client socket"),
            }
        }
    }

    pub fn ping(&mut self) -> anyhow::Result<()> {
        match self.call(IpcRequest::Ping)? {
            IpcResponse::Pong => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => anyhow::bail!("unexpected ping response"),
        }
    }

    pub fn get_state(&mut self) -> anyhow::Result<SessionSnapshot> {
        match self.call(IpcRequest::GetState)? {
            IpcResponse::State(snapshot) => Ok(snapshot),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => anyhow::bail!("unexpected get_state response"),
        }
    }

    pub fn send_command(&mut self, cmd: IpcCommand) -> anyhow::Result<()> {
        match self.call(IpcRequest::Command { cmd })? {
            IpcResponse::Ok => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => anyhow::bail!("unexpected command response"),
        }
    }

    pub fn set_ui_state(
        &mut self,
        selected_track_path: Option<PathBuf>,
        list_scroll: usize,
        search_query: String,
        quit_marked: bool,
    ) -> anyhow::Result<()> {
        match self.call(IpcRequest::SetUiState {
            selected_track_path,
            list_scroll,
            search_query,
            quit_marked,
        })? {
            IpcResponse::Ok => Ok(()),
            IpcResponse::Error { message } => Err(anyhow::anyhow!(message)),
            _ => anyhow::bail!("unexpected set_ui_state response"),
        }
    }

    pub fn shutdown(&mut self) -> anyhow::Result<()> {
        let _ = self.call(IpcRequest::Shutdown);
        Ok(())
    }

    pub fn try_read_event(&mut self) -> anyhow::Result<Option<PlayerEvent>> {
        self.stream
            .set_read_timeout(Some(Duration::from_millis(1)))
            .ok();
        let result = match read_envelope(&mut self.stream) {
            Ok(IpcEnvelope::Event(body)) => Ok(Some(body.into())),
            Ok(_) => Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => Ok(None),
            Err(e) => Err(e.into()),
        };
        self.stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .ok();
        result
    }
}

pub fn bridge_client(
    cmd_client: IpcClient,
    event_client: IpcClient,
) -> (
    Sender<PlayerCommand>,
    Receiver<PlayerEvent>,
    Arc<Mutex<IpcClient>>,
) {
    let cmd_shared = Arc::new(Mutex::new(cmd_client));
    let (cmd_tx, cmd_rx) = unbounded::<PlayerCommand>();
    let (evt_tx, evt_rx) = unbounded::<PlayerEvent>();

    {
        let cmd_shared = Arc::clone(&cmd_shared);
        thread::spawn(move || {
            while let Ok(cmd) = cmd_rx.recv() {
                if matches!(cmd, PlayerCommand::Shutdown) {
                    if let Ok(mut client) = cmd_shared.lock() {
                        let _ = client.shutdown();
                    }
                    break;
                }
                if let Some(ipc_cmd) = player_command_to_ipc(&cmd) {
                    if let Ok(mut client) = cmd_shared.lock() {
                        let _ = client.send_command(ipc_cmd);
                    }
                }
            }
        });
    }

    thread::spawn(move || {
        let mut client = event_client;
        loop {
            match client.try_read_event() {
                Ok(Some(ev)) => {
                    let _ = evt_tx.send(ev);
                }
                Ok(None) => thread::sleep(Duration::from_millis(16)),
                Err(_) => break,
            }
        }
    });

    (cmd_tx, evt_rx, cmd_shared)
}

pub struct DaemonServer {
    listener: TcpListener,
    clients: Arc<Mutex<Vec<TcpStream>>>,
}

impl DaemonServer {
    pub fn bind() -> anyhow::Result<Self> {
        ensure_runtime_dir()?;
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        write_daemon_addr(addr)?;
        std::fs::write(daemon_pid_path(), std::process::id().to_string())?;
        Ok(Self {
            listener,
            clients: Arc::new(Mutex::new(Vec::new())),
        })
    }

    pub fn accept(&self) -> anyhow::Result<()> {
        match self.listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(true).ok();
                if let Ok(mut clients) = self.clients.lock() {
                    clients.retain(|s| {
                        s.peer_addr()
                            .ok()
                            .map(|_| true)
                            .unwrap_or(false)
                    });
                    clients.push(stream);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    pub fn broadcast_event(&self, event: &PlayerEvent) {
        let ipc = IpcEnvelope::Event(event.clone().into());
        let Ok(mut clients) = self.clients.lock() else {
            return;
        };
        clients.retain_mut(|stream| {
            write_envelope(stream, &ipc).is_ok()
        });
    }

    pub fn handle_requests<F>(&self, mut on_request: F) -> anyhow::Result<bool>
    where
        F: FnMut(IpcRequest) -> IpcResponse,
    {
        let mut clients = self
            .clients
            .lock()
            .map_err(|_| anyhow::anyhow!("client lock poisoned"))?;
        let mut shutdown = false;

        clients.retain_mut(|stream| {
            loop {
                match read_envelope(stream) {
                    Ok(IpcEnvelope::Request(req)) => {
                        if matches!(req, IpcRequest::Shutdown) {
                            shutdown = true;
                        }
                        let resp = on_request(req);
                        if write_envelope(stream, &IpcEnvelope::Response(resp)).is_err() {
                            return false;
                        }
                    }
                    Ok(_) => return true,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        return true;
                    }
                    Err(_) => return false,
                }
            }
        });

        Ok(shutdown)
    }

    pub fn cleanup(&self) {
        let _ = std::fs::remove_file(daemon_pid_path());
        let _ = std::fs::remove_file(daemon_port_path());
    }
}

pub fn sync_ui_state(client: &Arc<Mutex<IpcClient>>, app: &crate::app::App) {
    let selected_track_path = app
        .filtered_indices
        .get(app.list_selection)
        .and_then(|&i| app.queue.tracks().get(i).map(|t| t.path.clone()));
    if let Ok(mut ipc) = client.lock() {
        let _ = ipc.set_ui_state(
            selected_track_path,
            app.list_scroll,
            app.search_query.clone(),
            app.quit_marked,
        );
    }
}

pub fn spawn_detached_daemon(session_path: &PathBuf) -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    std::process::Command::new(exe)
        .arg("--daemon")
        .arg(session_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_envelope_roundtrip() {
        let msg = IpcEnvelope::Request(IpcRequest::GetState);
        let json = serde_json::to_string(&msg).unwrap();
        let back: IpcEnvelope = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, IpcEnvelope::Request(IpcRequest::GetState)));
    }
}
