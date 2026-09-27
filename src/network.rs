use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread,
    time::{Duration, Instant},
};

use crate::wardrobe::Artifact;
use log::{debug, info, warn};
use sha2::{Digest, Sha256};
use tungstenite::{Message, WebSocket, connect, stream::MaybeTlsStream};
use url::Url;

const WEB_URL_ENV: &str = "CUBACADABRA_WEB_URL";
const AUTH_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const AUTH_POLL_INTERVAL: Duration = Duration::from_millis(25);
const MAX_CUBE_ZIP_BYTES: u64 = 25 * 1024 * 1024;
static PUBLISH_BUILD_SERIAL: AtomicU64 = AtomicU64::new(0);

const DEFAULT_BACKEND_URL: &str = match option_env!("CUBACADABRA_BACKEND_URL") {
    Some(url) => url,
    None => "http://127.0.0.1:8787",
};
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
    AuthStarted,
    AuthCompleted {
        user: AuthUser,
    },
    AuthError(String),
    AuthExpired,
    GamePublished(Result<String, String>),
}

#[allow(dead_code)]
#[derive(Clone, Debug, serde::Deserialize)]
pub struct AuthUser {
    pub id: String,
    pub email: Option<String>,
    pub name: String,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct AuthSession {
    pub access_token: String,
    pub refresh_token: String,
    pub user: AuthUser,
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
    BeginBrowserAuth,
    PublishGame(PathBuf),
    Shutdown,
}

pub struct BackendClient {
    commands: Sender<Command>,
    events: Receiver<BackendEvent>,
    worker: Option<thread::JoinHandle<()>>,
    // Kept in memory for the next authenticated catalog/publish requests.
    #[allow(dead_code)]
    auth: Arc<Mutex<Option<AuthSession>>>,
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
        let auth = Arc::new(Mutex::new(None));
        let worker = thread::Builder::new()
            .name("studio-backend".to_owned())
            .spawn({
                let auth = Arc::clone(&auth);
                move || run_worker(backend_url, game_id, command_receiver, event_sender, auth)
            })
            .map_err(|error| format!("could not start backend worker: {error}"))?;

        Ok(Self {
            commands: command_sender,
            events: event_receiver,
            worker: Some(worker),
            auth,
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

    pub fn begin_browser_auth(&self) {
        let _ = self.commands.send(Command::BeginBrowserAuth);
    }

    pub fn publish_game(&self, project_root: PathBuf) {
        let _ = self.commands.send(Command::PublishGame(project_root));
    }

    #[allow(dead_code)]
    pub fn auth_session(&self) -> Option<AuthSession> {
        self.auth.lock().ok().and_then(|session| session.clone())
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
    auth: Arc<Mutex<Option<AuthSession>>>,
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
                Ok(Command::Send(message)) => {
                    if socket.is_some() || !is_live_cube_move(&message) {
                        pending_messages.push_back(message);
                    }
                }
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
                Ok(Command::BeginBrowserAuth) => {
                    let backend_url = backend_url.clone();
                    let events = events.clone();
                    let auth = Arc::clone(&auth);
                    thread::spawn(move || run_browser_auth(&backend_url, &events, &auth));
                }
                Ok(Command::PublishGame(project_root)) => {
                    let backend_url = backend_url.clone();
                    let events = events.clone();
                    let auth = Arc::clone(&auth);
                    thread::spawn(move || {
                        let token = auth.lock().ok().and_then(|session| {
                            session.as_ref().map(|session| session.access_token.clone())
                        });
                        let result = token
                            .ok_or_else(|| "Sign in before publishing a game.".to_owned())
                            .map_err(PublishError::from)
                            .and_then(|token| publish_game(&backend_url, &project_root, &token));
                        let result = result.map_err(|error| {
                            if error.auth_expired {
                                if let Ok(mut session) = auth.lock() {
                                    session.take();
                                }
                                let _ = events.send(BackendEvent::AuthExpired);
                            }
                            error.message
                        });
                        let _ = events.send(BackendEvent::GamePublished(result));
                    });
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
                pending_messages.retain(|message| !is_live_cube_move(message));
                next_connect_at = Instant::now() + RECONNECT_INTERVAL;
            }
        }

        thread::sleep(NETWORK_POLL_INTERVAL);
    }

    disconnect(&mut socket, &mut connected_world_id, &events);
}

fn is_live_cube_move(message: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(message).is_ok_and(|value| {
        value.get("type").and_then(serde_json::Value::as_str) == Some("world_block_move")
    })
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

#[derive(Debug)]
struct PublishError {
    message: String,
    auth_expired: bool,
}

impl From<String> for PublishError {
    fn from(message: String) -> Self {
        Self {
            message,
            auth_expired: false,
        }
    }
}

impl From<&str> for PublishError {
    fn from(message: &str) -> Self {
        Self::from(message.to_owned())
    }
}

fn publish_game(
    backend_url: &Url,
    project_root: &std::path::Path,
    token: &str,
) -> Result<String, PublishError> {
    let working_dir = std::env::temp_dir().join(format!(
        "cubacadabra-publish-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("Could not prepare game build: {error}"))?
            .as_nanos(),
        PUBLISH_BUILD_SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let zip_path = working_dir.join("game.zip");
        cubacadabra_builder::build_game(&cubacadabra_builder::BuildOptions {
            source_root: project_root.join("src"),
            manifest_path: project_root.join("manifest.json"),
            output: working_dir.join("package"),
            zip_path: Some(zip_path.clone()),
        })
        .map_err(|error| format!("Could not build game: {error}"))?;
        let size = std::fs::metadata(&zip_path)
            .map_err(|error| format!("Could not read built ZIP: {error}"))?
            .len();
        if size > MAX_CUBE_ZIP_BYTES {
            return Err("The built ZIP is larger than the 25 MiB upload limit.".into());
        }
        let archive = std::fs::read(&zip_path)
            .map_err(|error| format!("Could not read built ZIP: {error}"))?;
        let endpoint = http_url(backend_url, "/cubes/upload")?;
        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .http_status_as_error(false)
                .timeout_global(Some(Duration::from_secs(120)))
                .build(),
        );
        let mut response = agent
            .post(endpoint.as_str())
            .header("Accept", "application/json")
            .header("Content-Type", "application/zip")
            .header("Authorization", &format!("Bearer {token}"))
            .send(archive.as_slice())
            .map_err(|error| format!("Could not upload game: {error}"))?;
        let source = response
            .body_mut()
            .read_to_string()
            .map_err(|error| format!("Could not read upload response: {error}"))?;
        let value: serde_json::Value = serde_json::from_str(&source)
            .map_err(|error| format!("Upload response was invalid: {error}"))?;
        if !response.status().is_success() {
            let code = value
                .get("error")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            return Err(PublishError {
                message: cube_upload_error(code),
                auth_expired: code == "not_authenticated",
            });
        }
        let cube = value
            .get("cube")
            .ok_or("Upload response did not include a game.")?;
        let name = cube
            .get("displayName")
            .and_then(serde_json::Value::as_str)
            .ok_or("Upload response did not include a game name.")?;
        let version = cube
            .get("version")
            .ok_or("Upload response did not include a game version.")?;
        let version = version
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| version.to_string());
        Ok(format!("{name} {version} was published successfully."))
    })();
    let _ = std::fs::remove_dir_all(&working_dir);
    result
}

fn cube_upload_error(code: &str) -> String {
    match code {
        "not_authenticated" => "Your session has expired. Sign in again.".to_owned(),
        "uploads_not_allowed_in_free_plan" => {
            "Publishing requires a Creator Pro or Studio developer plan.".to_owned()
        }
        "cube_zip_too_large" => "The built ZIP is larger than the 25 MiB upload limit.".to_owned(),
        "cube_already_exists" => {
            "This game version is already published to your account.".to_owned()
        }
        "cube_id_taken" => {
            "This game ID belongs to another creator. Change the ID in manifest.json.".to_owned()
        }
        "invalid_cube_id" => "The game ID in manifest.json is invalid.".to_owned(),
        "invalid_cube_version" => "The game version in manifest.json is invalid.".to_owned(),
        "invalid_display_name" => "The game display name in manifest.json is invalid.".to_owned(),
        "cube_upload_unavailable" => {
            "Publishing is temporarily unavailable. Try again later.".to_owned()
        }
        _ => "The game ZIP was rejected. Check the project and try again.".to_owned(),
    }
}

fn run_browser_auth(
    backend_url: &Url,
    events: &Sender<BackendEvent>,
    auth: &Arc<Mutex<Option<AuthSession>>>,
) {
    let result = browser_auth_url(backend_url).and_then(|(url, redirect_uri, state, listener)| {
        if !open_browser(&url) {
            return Err(
                "Could not open the default browser. Check your browser association and try again."
                    .to_owned(),
            );
        }
        let _ = events.send(BackendEvent::AuthStarted);
        wait_for_browser_callback(backend_url, events, listener, &redirect_uri, &state, auth)
    });
    if let Err(message) = result {
        warn!("browser authentication failed: {}", message);
        let _ = events.send(BackendEvent::AuthError(message));
    }
}

fn browser_auth_url(backend_url: &Url) -> Result<(String, String, String, TcpListener), String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|error| format!("could not start the local login callback: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("could not configure the local login callback: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("could not read the local login callback address: {error}"))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/auth/callback");
    let state = random_state()?;
    let mut login_url = web_base_url(backend_url)?;
    login_url
        .query_pairs_mut()
        .append_pair("app_redirect_uri", &redirect_uri)
        .append_pair("state", &state);
    Ok((login_url.to_string(), redirect_uri, state, listener))
}

fn web_base_url(backend_url: &Url) -> Result<Url, String> {
    let raw = std::env::var(WEB_URL_ENV).ok();
    let mut url = if let Some(raw) = raw.filter(|value| !value.trim().is_empty()) {
        Url::parse(raw.trim())
            .map_err(|error| format!("{WEB_URL_ENV} is not a valid URL: {error}"))?
    } else if backend_url
        .host_str()
        .is_some_and(|host| host == "localhost" || host == "127.0.0.1")
    {
        // Keep the browser login on the same loopback site as the local API.
        // Using localhost here while the API uses 127.0.0.1 makes the session
        // cookie cross-site and can cause /auth/app/redirect to return 401.
        Url::parse("http://127.0.0.1:5173").expect("local web URL must be valid")
    } else {
        Url::parse("https://cubacadabra.com").expect("production web URL must be valid")
    };
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(format!("{WEB_URL_ENV} must be an http(s) URL with a host"));
    }
    url.set_path("/login/");
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

fn random_state() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("could not create login state: {error}"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn open_browser(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", ""]);
        command
    };
    #[cfg(target_os = "linux")]
    let mut command = std::process::Command::new("xdg-open");
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return false;
    command.arg(url).spawn().is_ok()
}

fn wait_for_browser_callback(
    backend_url: &Url,
    events: &Sender<BackendEvent>,
    listener: TcpListener,
    redirect_uri: &str,
    expected_state: &str,
    auth: &Arc<Mutex<Option<AuthSession>>>,
) -> Result<(), String> {
    let deadline = Instant::now() + AUTH_TIMEOUT;
    while Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .map_err(|error| format!("could not configure login callback: {error}"))?;
                let request = read_callback_request(&mut stream)?;
                let callback = Url::parse(&format!("http://127.0.0.1{request}"))
                    .map_err(|error| format!("invalid login callback: {error}"))?;
                if callback.path() != "/auth/callback" {
                    write_browser_response(&mut stream, false);
                    continue;
                }
                if callback
                    .query_pairs()
                    .find(|(key, _)| key == "state")
                    .map(|(_, value)| value.to_string())
                    .as_deref()
                    != Some(expected_state)
                {
                    write_browser_response(&mut stream, false);
                    continue;
                }
                let Some(code) = callback
                    .query_pairs()
                    .find(|(key, _)| key == "code")
                    .map(|(_, value)| value.to_string())
                else {
                    write_browser_response(&mut stream, false);
                    return Err(
                        "The browser login did not return an authorization code.".to_owned()
                    );
                };
                let session = match exchange_browser_code(backend_url, &code, redirect_uri) {
                    Ok(session) => session,
                    Err(message) => {
                        write_browser_response(&mut stream, false);
                        return Err(message);
                    }
                };
                if let Ok(mut current) = auth.lock() {
                    *current = Some(session.clone());
                }
                write_browser_response(&mut stream, true);
                let _ = events.send(BackendEvent::AuthCompleted {
                    user: session.user.clone(),
                });
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(AUTH_POLL_INTERVAL);
            }
            Err(error) => return Err(format!("login callback failed: {error}")),
        }
    }
    Err("Browser login timed out. Try signing in again.".to_owned())
}

fn read_callback_request(stream: &mut TcpStream) -> Result<String, String> {
    let mut bytes = Vec::with_capacity(1024);
    let mut buffer = [0u8; 1024];
    while bytes.len() < 8 * 1024 {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                bytes.extend_from_slice(&buffer[..read]);
                if bytes.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            Err(error) => return Err(format!("could not read login callback: {error}")),
        }
    }
    let request =
        String::from_utf8(bytes).map_err(|_| "login callback was not valid HTTP".to_owned())?;
    let target = request
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("GET "))
        .and_then(|line| line.split_whitespace().next())
        .ok_or_else(|| "login callback was not a GET request".to_owned())?;
    Ok(target.to_owned())
}

fn write_browser_response(stream: &mut TcpStream, success: bool) {
    let (title, message) = if success {
        (
            "Signed in to cubacadabra",
            "You're signed in to cubacadabra Studio. You can close this browser window.",
        )
    } else {
        (
            "Studio sign-in failed",
            "Studio could not finish signing you in. You can close this browser window and try again.",
        )
    };
    let body = format!(
        "<!doctype html><meta name=viewport content=\"width=device-width,initial-scale=1\"><title>{title}</title><style>body{{font:16px system-ui,sans-serif;background:#17181c;color:#f3f4f6;display:grid;place-items:center;min-height:100vh;margin:0}}main{{max-width:34rem;padding:32px}}p{{color:#b6bac5;line-height:1.5}}</style><main><h1>{title}</h1><p>{message}</p></main>"
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn exchange_browser_code(
    backend_url: &Url,
    code: &str,
    redirect_uri: &str,
) -> Result<AuthSession, String> {
    let endpoint = http_url(backend_url, "/auth/app/exchange")?;
    let payload = serde_json::json!({ "code": code, "redirect_uri": redirect_uri });
    let mut response = ureq::post(endpoint.as_str())
        .header("content-type", "application/json")
        .send(payload.to_string())
        .map_err(|error| format!("could not exchange the browser login: {error}"))?;
    let source = response
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("could not read the browser login response: {error}"))?;
    if !response.status().is_success() {
        return Err("The browser login could not be exchanged for a Studio session.".to_owned());
    }
    let value: serde_json::Value = serde_json::from_str(&source)
        .map_err(|error| format!("browser login response was invalid: {error}"))?;
    let user: AuthUser = serde_json::from_value(
        value
            .get("user")
            .cloned()
            .ok_or_else(|| "browser login response did not include a user".to_owned())?,
    )
    .map_err(|error| format!("browser login user was invalid: {error}"))?;
    let access_token = value
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "browser login response did not include an access token".to_owned())?;
    let refresh_token = value
        .get("refresh_token")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "browser login response did not include a refresh token".to_owned())?;
    Ok(AuthSession {
        access_token: access_token.to_owned(),
        refresh_token: refresh_token.to_owned(),
        user,
    })
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
    fn publish_game_builds_zip_and_uploads_with_studio_token() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server_addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("upload request did not arrive: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            let header_end = loop {
                let mut chunk = [0u8; 8192];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&chunk[..count]);
                if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..header_end]).to_ascii_lowercase();
            assert!(headers.starts_with("post /cubes/upload http/1.1"));
            assert!(headers.contains("authorization: bearer test-token\r\n"));
            assert!(headers.contains("content-type: application/zip\r\n"));
            let length = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .unwrap()
                .trim()
                .parse::<usize>()
                .unwrap();
            while request.len() - header_end < length {
                let mut chunk = [0u8; 8192];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&chunk[..count]);
            }
            assert_eq!(&request[header_end..header_end + 2], b"PK");
            let body = br#"{"ok":true,"cube":{"id":"conformance-game","displayName":"Conformance Fixture","version":"0.3.0"}}"#;
            write!(stream, "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            stream.write_all(body).unwrap();
        });
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../tools/tests/fixtures/conformance-game");
        let backend_url = Url::parse(&format!("http://{server_addr}")).unwrap();
        let result = publish_game(&backend_url, &fixture, "test-token");
        let message = result.unwrap();
        assert_eq!(
            message,
            "Conformance Fixture 0.3.0 was published successfully."
        );
        server.join().unwrap();
    }

    #[test]
    fn publish_game_shows_backend_plan_rejection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let server_addr = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(10);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("upload request did not arrive: {error}"),
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            let header_end = loop {
                let mut chunk = [0u8; 8192];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&chunk[..count]);
                if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..header_end]).to_ascii_lowercase();
            let length = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .unwrap()
                .trim()
                .parse::<usize>()
                .unwrap();
            while request.len() - header_end < length {
                let mut chunk = [0u8; 8192];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0);
                request.extend_from_slice(&chunk[..count]);
            }
            let body = br#"{"error":"uploads_not_allowed_in_free_plan"}"#;
            write!(stream, "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            stream.write_all(body).unwrap();
        });
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../tools/tests/fixtures/conformance-game");
        let backend_url = Url::parse(&format!("http://{server_addr}")).unwrap();
        let result = publish_game(&backend_url, &fixture, "test-token");
        let message = result.unwrap_err().message;
        assert_eq!(
            message,
            "Publishing requires a Creator Pro or Studio developer plan."
        );
        server.join().unwrap();
    }

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

    #[test]
    fn production_backend_uses_production_web_login_url() {
        let backend = parse_backend_url("https://api.cubacadabra.com").expect("valid backend URL");
        assert_eq!(
            web_base_url(&backend).unwrap().as_str(),
            "https://cubacadabra.com/login/"
        );
    }
}
