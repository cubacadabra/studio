use std::{
    collections::VecDeque,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use crate::wardrobe::Artifact;
use log::{debug, info, warn};
use sha2::{Digest, Sha256};
use tungstenite::{Message, WebSocket, connect, stream::MaybeTlsStream};
use url::Url;

const DEFAULT_BACKEND_URL: &str = "http://127.0.0.1:8787";
const BACKEND_URL_ENV: &str = "CUBACADABRA_BACKEND_URL";
const MOVE_SEND_INTERVAL: Duration = Duration::from_millis(83);
const MOVE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const RECONNECT_INTERVAL: Duration = Duration::from_millis(750);
const NETWORK_POLL_INTERVAL: Duration = Duration::from_millis(10);

type Socket = WebSocket<MaybeTlsStream<std::net::TcpStream>>;

#[derive(Debug)]
pub enum BackendEvent {
    Connected,
    Disconnected,
    Message(String),
    MorphCatalog(String),
    MorphCatalogError(String),
    MorphPacks {
        request_id: u64,
        packs: Vec<(String, Vec<u8>)>,
    },
    MorphPacksError {
        request_id: u64,
        message: String,
    },
    MorphThumbnail {
        url: String,
        bytes: Vec<u8>,
    },
}

#[derive(Debug)]
struct MoveCommand {
    x: f32,
    y: f32,
    z: f32,
    yaw: f32,
    moving: bool,
    sprinting: bool,
    respawn_event_id: u32,
}

enum Command {
    SetWorld(String),
    Send(String),
    Move(MoveCommand),
    FetchMorphCatalog,
    FetchMorphPacks {
        request_id: u64,
        assets: Vec<(String, Artifact)>,
    },
    FetchMorphThumbnail(String),
    Shutdown,
}

pub struct BackendClient {
    commands: Sender<Command>,
    events: Receiver<BackendEvent>,
    worker: Option<thread::JoinHandle<()>>,
}

impl BackendClient {
    pub fn new(game_id: &str) -> Result<Self, String> {
        let raw_url = std::env::var(BACKEND_URL_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_BACKEND_URL.to_owned());
        let backend_url = parse_backend_url(&raw_url)?;
        info!(
            "backend client starting: url={} game_id={}",
            backend_url, game_id
        );
        let game_id = game_id.to_owned();
        let (command_sender, command_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("studio-backend".to_owned())
            .spawn(move || run_worker(backend_url, game_id, command_receiver, event_sender))
            .map_err(|error| format!("could not start backend worker: {error}"))?;

        Ok(Self {
            commands: command_sender,
            events: event_receiver,
            worker: Some(worker),
        })
    }

    pub fn set_world(&self, world_id: impl Into<String>) {
        let world_id = world_id.into();
        debug!("queueing world selection: world_id={}", world_id);
        let _ = self.commands.send(Command::SetWorld(world_id));
    }

    pub fn send(&self, message: String) {
        let _ = self.commands.send(Command::Send(message));
    }

    pub fn send_move(
        &self,
        x: f32,
        y: f32,
        z: f32,
        yaw: f32,
        moving: bool,
        sprinting: bool,
        respawn_event_id: u32,
    ) {
        let _ = self.commands.send(Command::Move(MoveCommand {
            x,
            y,
            z,
            yaw,
            moving,
            sprinting,
            respawn_event_id,
        }));
    }

    pub fn request_morph_catalog(&self) {
        debug!("queueing morph catalog request");
        let _ = self.commands.send(Command::FetchMorphCatalog);
    }

    pub fn request_morph_packs(&self, request_id: u64, assets: Vec<(String, Artifact)>) {
        let _ = self
            .commands
            .send(Command::FetchMorphPacks { request_id, assets });
    }

    pub fn request_morph_thumbnail(&self, url: String) {
        let _ = self.commands.send(Command::FetchMorphThumbnail(url));
    }

    pub fn try_recv(&self) -> Option<BackendEvent> {
        self.events.try_recv().ok()
    }
}

impl Drop for BackendClient {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn parse_backend_url(raw_url: &str) -> Result<Url, String> {
    let url = Url::parse(raw_url.trim())
        .map_err(|error| format!("{BACKEND_URL_ENV} is not a valid URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https" | "ws" | "wss") || url.host_str().is_none() {
        return Err(format!(
            "{BACKEND_URL_ENV} must be an http(s) or ws(s) URL with a host"
        ));
    }
    Ok(url)
}

fn socket_url(base_url: &Url, game_id: &str, world_id: &str) -> Result<Url, String> {
    let mut url = base_url.clone();
    let socket_scheme = match url.scheme() {
        "http" | "ws" => "ws",
        "https" | "wss" => "wss",
        scheme => return Err(format!("unsupported backend URL scheme: {scheme}")),
    };
    url.set_scheme(socket_scheme)
        .map_err(|()| "could not set the backend WebSocket scheme".to_owned())?;
    let base_path = url.path().trim_end_matches('/');
    url.set_path(&format!(
        "{base_path}/world/{}",
        encode_path_segment(world_id)
    ));
    url.query_pairs_mut()
        .append_pair("client", "web")
        .append_pair("game", game_id);
    Ok(url)
}

fn set_nonblocking(socket: &mut Socket) -> std::io::Result<()> {
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => stream.set_nonblocking(true),
        MaybeTlsStream::NativeTls(stream) => stream.get_mut().set_nonblocking(true),
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "unsupported TLS stream",
        )),
    }
}

fn encode_path_segment(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b':' => {
                char::from(byte).to_string()
            }
            byte => format!("%{byte:02X}"),
        })
        .collect()
}

fn run_worker(
    backend_url: Url,
    game_id: String,
    commands: Receiver<Command>,
    events: Sender<BackendEvent>,
) {
    // Studio owns world selection. Waiting for SetWorld avoids connecting to
    // an initial world and then resetting that same session on the first frame.
    let mut desired_world_id = None;
    let mut connected_world_id = None;
    let mut socket = None;
    let mut next_connect_at = Instant::now();
    let mut pending_messages = VecDeque::new();
    let mut latest_move = None;
    let mut last_sent_move = None;
    let mut last_move_sent_at = Instant::now() - MOVE_HEARTBEAT_INTERVAL;
    let mut running = true;

    while running {
        loop {
            match commands.try_recv() {
                Ok(Command::SetWorld(world_id)) => {
                    if desired_world_id.as_deref() != Some(world_id.as_str()) {
                        debug!(
                            "backend worker changed desired world: world_id={}",
                            world_id
                        );
                        desired_world_id = Some(world_id);
                        disconnect(&mut socket, &mut connected_world_id, &events);
                        pending_messages.clear();
                        last_sent_move = None;
                        next_connect_at = Instant::now();
                    }
                }
                Ok(Command::Send(message)) => pending_messages.push_back(message),
                Ok(Command::Move(movement)) => latest_move = Some(movement),
                Ok(Command::FetchMorphCatalog) => {
                    let endpoint = http_url(&backend_url, "/morphs/catalog")
                        .map(|url| url.to_string())
                        .unwrap_or_else(|_| "/morphs/catalog".to_owned());
                    debug!("fetching morph catalog: url={}", endpoint);
                    match fetch_morph_catalog(&backend_url) {
                        Ok(source) => {
                            debug!("morph catalog response received: bytes={}", source.len());
                            let _ = events.send(BackendEvent::MorphCatalog(source));
                        }
                        Err(message) => {
                            warn!("morph catalog HTTP request failed: {}", message);
                            let _ = events.send(BackendEvent::MorphCatalogError(message));
                        }
                    }
                }
                Ok(Command::FetchMorphPacks { request_id, assets }) => {
                    let result = assets
                        .into_iter()
                        .map(|(id, artifact)| {
                            let bytes = fetch_http_bytes(&backend_url, &artifact.url)?;
                            verify_artifact(&bytes, &artifact)?;
                            Ok((id, bytes))
                        })
                        .collect::<Result<Vec<_>, String>>();
                    let event = match result {
                        Ok(packs) => BackendEvent::MorphPacks { request_id, packs },
                        Err(message) => BackendEvent::MorphPacksError {
                            request_id,
                            message,
                        },
                    };
                    let _ = events.send(event);
                }
                Ok(Command::FetchMorphThumbnail(url)) => {
                    // Images are optional; a failed thumbnail never blocks selection.
                    if let Ok(bytes) = fetch_http_bytes(&backend_url, &url) {
                        let expected = url
                            .rsplit('/')
                            .next()
                            .and_then(|name| name.strip_suffix(".png"));
                        if expected
                            .is_some_and(|hash| format!("{:x}", Sha256::digest(&bytes)) == hash)
                        {
                            let _ = events.send(BackendEvent::MorphThumbnail { url, bytes });
                        }
                    }
                }
                Ok(Command::Shutdown) | Err(TryRecvError::Disconnected) => {
                    running = false;
                    break;
                }
                Err(TryRecvError::Empty) => break,
            }
        }

        if !running {
            break;
        }

        if socket.is_none() && desired_world_id.is_some() && Instant::now() >= next_connect_at {
            let world_id = desired_world_id.as_deref().unwrap_or_default();
            match socket_url(&backend_url, &game_id, world_id)
                .map_err(|error| {
                    tungstenite::Error::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        error,
                    ))
                })
                .and_then(|url| connect(url.as_str()).map(|(socket, _)| socket))
                .and_then(|mut socket| {
                    set_nonblocking(&mut socket)
                        .map(|()| socket)
                        .map_err(tungstenite::Error::Io)
                }) {
                Ok(next_socket) => {
                    info!("backend world socket connected: world_id={}", world_id);
                    socket = Some(next_socket);
                    connected_world_id = Some(world_id.to_owned());
                    next_connect_at = Instant::now();
                    let _ = events.send(BackendEvent::Connected);
                }
                Err(error) => {
                    debug!(
                        "backend world socket connection failed: world_id={} error={}",
                        world_id, error
                    );
                    next_connect_at = Instant::now() + RECONNECT_INTERVAL;
                }
            }
        }

        if let Some(current_socket) = socket.as_mut() {
            let mut failed = false;
            while let Some(message) = pending_messages.front() {
                match current_socket.send(Message::Text(message.clone().into())) {
                    Ok(()) => {
                        pending_messages.pop_front();
                    }
                    Err(tungstenite::Error::Io(error))
                        if error.kind() == std::io::ErrorKind::WouldBlock =>
                    {
                        break;
                    }
                    Err(_) => {
                        failed = true;
                        break;
                    }
                }
            }

            if !failed {
                if let Some(movement) = latest_move.as_ref() {
                    let now = Instant::now();
                    let movement_json = serde_json::json!({
                        "type": "move",
                        "x": movement.x,
                        "y": movement.y,
                        "z": movement.z,
                        "yaw": movement.yaw,
                        "moving": movement.moving,
                        "sprinting": movement.sprinting,
                        "respawnEventId": movement.respawn_event_id,
                    })
                    .to_string();
                    if now.duration_since(last_move_sent_at) >= MOVE_SEND_INTERVAL
                        && (last_sent_move.as_deref() != Some(movement_json.as_str())
                            || now.duration_since(last_move_sent_at) >= MOVE_HEARTBEAT_INTERVAL)
                    {
                        match current_socket.send(Message::Text(movement_json.clone().into())) {
                            Ok(()) => {
                                last_sent_move = Some(movement_json);
                                last_move_sent_at = now;
                            }
                            Err(tungstenite::Error::Io(error))
                                if error.kind() == std::io::ErrorKind::WouldBlock => {}
                            Err(_) => failed = true,
                        }
                    }
                }
            }

            if !failed {
                loop {
                    match current_socket.read() {
                        Ok(Message::Text(message)) => {
                            let _ = events.send(BackendEvent::Message(message.to_string()));
                        }
                        Ok(Message::Ping(payload)) => {
                            if current_socket.send(Message::Pong(payload)).is_err() {
                                failed = true;
                                break;
                            }
                        }
                        Ok(Message::Pong(_)) => {}
                        Ok(Message::Close(_)) => {
                            failed = true;
                            break;
                        }
                        Ok(_) => {}
                        Err(tungstenite::Error::Io(error))
                            if error.kind() == std::io::ErrorKind::WouldBlock =>
                        {
                            break;
                        }
                        Err(_) => {
                            failed = true;
                            break;
                        }
                    }
                }
            }

            if failed {
                disconnect(&mut socket, &mut connected_world_id, &events);
                next_connect_at = Instant::now() + RECONNECT_INTERVAL;
            }
        }

        thread::sleep(NETWORK_POLL_INTERVAL);
    }

    disconnect(&mut socket, &mut connected_world_id, &events);
}

fn http_url(base_url: &Url, path: &str) -> Result<Url, String> {
    let mut url = base_url.clone();
    let scheme = match url.scheme() {
        "ws" => "http",
        "wss" => "https",
        "http" => "http",
        "https" => "https",
        scheme => return Err(format!("unsupported backend URL scheme: {scheme}")),
    };
    url.set_scheme(scheme)
        .map_err(|()| "could not set HTTP scheme".to_owned())?;
    let base_path = url.path().trim_end_matches('/');
    let request_path = if path.starts_with('/') {
        format!("{base_path}{path}")
    } else if base_path.is_empty() {
        format!("/{path}")
    } else {
        format!("{base_path}/{path}")
    };

    // Parse the complete URL instead of passing an already-percent-encoded
    // path through Url::set_path, which would encode `%3A` as `%253A` and
    // make morph asset IDs fail to resolve in the backend.
    url.set_path("");
    url.set_query(None);
    url.set_fragment(None);
    Url::parse(&format!(
        "{}{}",
        url.as_str().trim_end_matches('/'),
        request_path
    ))
    .map_err(|error| format!("could not build HTTP URL: {error}"))
}

fn verify_artifact(bytes: &[u8], artifact: &Artifact) -> Result<(), String> {
    if bytes.len() != artifact.bytes || format!("{:x}", Sha256::digest(bytes)) != artifact.sha256 {
        return Err("Downloaded morph did not match the published content hash.".into());
    }
    Ok(())
}

fn fetch_morph_catalog(base_url: &Url) -> Result<String, String> {
    let mut combined: Option<serde_json::Value> = None;
    let mut path = "/morphs/catalog?limit=100".to_owned();
    let mut cursors = std::collections::BTreeSet::new();
    for _ in 0..8 {
        let source = fetch_http_text(base_url, &path)?;
        let mut page: serde_json::Value = serde_json::from_str(&source)
            .map_err(|error| format!("Invalid catalog page: {error}"))?;
        let cursor = page
            .get("nextCursor")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        if let Some(catalog) = &mut combined {
            if catalog["release"] != page["release"] {
                return Err("Catalog changed while loading. Retry.".into());
            }
            let rows = page["assets"]
                .as_array_mut()
                .ok_or("Missing catalog assets")?;
            catalog["assets"]
                .as_array_mut()
                .ok_or("Missing catalog assets")?
                .append(rows);
        } else {
            combined = Some(page);
        }
        if let Some(catalog) = &combined {
            if catalog["assets"]
                .as_array()
                .is_none_or(|rows| rows.len() > cubacadabra_morphs::MAX_CATALOG_ASSETS)
            {
                return Err("Catalog exceeds the supported asset count.".into());
            }
        }
        let Some(cursor) = cursor else {
            return serde_json::to_string(&combined.unwrap()).map_err(|error| error.to_string());
        };
        if !cursors.insert(cursor.clone()) {
            return Err("Catalog repeated its pagination cursor.".into());
        }
        path = format!(
            "/morphs/catalog?limit=100&cursor={}",
            encode_path_segment(&cursor)
        );
    }
    Err("Catalog has too many pages.".into())
}

fn fetch_http_text(base_url: &Url, path: &str) -> Result<String, String> {
    let url = http_url(base_url, path)?;
    debug!("resolved morph catalog URL: {}", url);
    let mut response = ureq::get(url.as_str())
        .call()
        .map_err(|error| format!("morph catalog request failed: {error}"))?;
    response
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("morph catalog response failed: {error}"))
}

fn fetch_http_bytes(base_url: &Url, raw_url: &str) -> Result<Vec<u8>, String> {
    let url = if raw_url.starts_with('/') {
        http_url(base_url, raw_url)?
    } else {
        Url::parse(raw_url).map_err(|error| format!("morph pack URL is invalid: {error}"))?
    };
    debug!("resolved morph pack URL: {}", url);
    let mut response = ureq::get(url.as_str())
        .call()
        .map_err(|error| format!("morph pack request failed: {error}"))?;
    response
        .body_mut()
        .read_to_vec()
        .map_err(|error| format!("morph pack response failed: {error}"))
}

fn disconnect(
    socket: &mut Option<Socket>,
    connected_world_id: &mut Option<String>,
    events: &Sender<BackendEvent>,
) {
    if socket.take().is_some() {
        *connected_world_id = None;
        let _ = events.send(BackendEvent::Disconnected);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_packs_require_both_the_expected_hash_and_length() {
        let bytes = b"test pack";
        let mut artifact = Artifact {
            url: String::new(),
            sha256: format!("{:x}", Sha256::digest(bytes)),
            bytes: bytes.len(),
        };
        assert!(verify_artifact(bytes, &artifact).is_ok());
        assert!(verify_artifact(b"bad bytes", &artifact).is_err());
        artifact.bytes += 1;
        assert!(verify_artifact(bytes, &artifact).is_err());
    }

    #[test]
    fn catalog_pagination_preserves_query_and_base_path() {
        let base = parse_backend_url("http://127.0.0.1:8787/api").unwrap();
        let url = http_url(
            &base,
            "/morphs/catalog?limit=100&cursor=eyJvZmZzZXQiOjEwMH0",
        )
        .unwrap();
        assert_eq!(url.path(), "/api/morphs/catalog");
        assert_eq!(
            url.query_pairs().find(|(key, _)| key == "limit").unwrap().1,
            "100"
        );
    }

    #[test]
    fn backend_http_url_becomes_local_websocket_url() {
        let base = parse_backend_url("http://127.0.0.1:8787").expect("valid backend URL");
        let socket = socket_url(&base, "survival-101", "lobby").expect("valid socket URL");
        assert_eq!(
            socket.as_str(),
            "ws://127.0.0.1:8787/world/lobby?client=web&game=survival-101"
        );
    }

    #[test]
    fn http_url_preserves_hash_addressed_morph_pack_path() {
        let base = parse_backend_url("http://127.0.0.1:8787").expect("valid backend URL");
        let url = http_url(
            &base,
            "/morphs/packs/sha256/9d/9db38c0adca688564d80db45c02427f3de440f8c59a1ab56356942c48ce47244.morphpack",
        )
        .expect("valid morph URL");
        assert_eq!(
            url.as_str(),
            "http://127.0.0.1:8787/morphs/packs/sha256/9d/9db38c0adca688564d80db45c02427f3de440f8c59a1ab56356942c48ce47244.morphpack"
        );
    }

    #[test]
    fn backend_https_url_becomes_secure_websocket_url() {
        let base = parse_backend_url("https://api.cubacadabra.com").expect("valid backend URL");
        let socket = socket_url(&base, "first-game", "real-game").expect("valid socket URL");
        assert_eq!(
            socket.as_str(),
            "wss://api.cubacadabra.com/world/real-game?client=web&game=first-game"
        );
    }
}
