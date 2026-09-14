use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command as ProcessCommand, Stdio},
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
    thread,
    time::Duration,
};

const CODEX_PATH_ENV: &str = "CUBACADABRA_CODEX_PATH";
const WORKER_POLL_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatGptAccount {
    pub email: Option<String>,
    pub plan_type: Option<String>,
}

#[derive(Debug)]
pub enum CodexEvent {
    AccountStatus(Option<ChatGptAccount>),
    BrowserOpened,
    LoginCompleted(ChatGptAccount),
    ChatReady,
    AssistantDelta(String),
    AssistantMessage(String),
    ChatTurnCompleted,
    ChatError(String),
    Error(String),
    Unavailable(String),
}

enum CodexCommand {
    BeginChatGptLogin,
    OpenChat,
    SendChatMessage {
        message: String,
        model: String,
        reasoning_effort: String,
    },
    SetProjectRoot(PathBuf),
    Shutdown,
}

enum AppServerOutput {
    Message(Value),
    Invalid(String),
    Closed,
}

#[derive(Clone, Copy)]
enum PendingRequest {
    Initialize,
    Account { completes_login: bool },
    Login,
    ThreadStart,
    TurnStart,
}

struct ProtocolState {
    stdin: ChildStdin,
    next_id: u64,
    pending: BTreeMap<u64, PendingRequest>,
    project_root: PathBuf,
    initialized: bool,
    queued_login: bool,
    login_active: bool,
    chat_requested: bool,
    queued_messages: Vec<QueuedChatMessage>,
    thread_id: Option<String>,
    thread_start_pending: bool,
    turn_active: bool,
}

#[derive(Clone)]
struct QueuedChatMessage {
    message: String,
    model: String,
    reasoning_effort: String,
}

impl ProtocolState {
    fn new(stdin: ChildStdin, project_root: PathBuf) -> Self {
        Self {
            stdin,
            next_id: 1,
            pending: BTreeMap::new(),
            project_root,
            initialized: false,
            queued_login: false,
            login_active: false,
            chat_requested: false,
            queued_messages: Vec::new(),
            thread_id: None,
            thread_start_pending: false,
            turn_active: false,
        }
    }

    fn request(
        &mut self,
        method: &str,
        params: Value,
        pending: PendingRequest,
    ) -> Result<(), String> {
        let id = self.next_id;
        self.next_id += 1;
        write_message(
            &mut self.stdin,
            &json!({ "method": method, "id": id, "params": params }),
        )?;
        self.pending.insert(id, pending);
        Ok(())
    }

    fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        write_message(
            &mut self.stdin,
            &json!({ "method": method, "params": params }),
        )
    }

    fn initialize(&mut self) -> Result<(), String> {
        self.request(
            "initialize",
            json!({
                "clientInfo": {
                    "name": "cubacadabra_studio",
                    "title": "Cubacadabra Studio",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            }),
            PendingRequest::Initialize,
        )
    }

    fn read_account(&mut self, completes_login: bool) -> Result<(), String> {
        self.request(
            "account/read",
            json!({ "refreshToken": false }),
            PendingRequest::Account { completes_login },
        )
    }

    fn begin_login(&mut self) -> Result<(), String> {
        if self.login_active {
            return Ok(());
        }
        self.login_active = true;
        if let Err(error) = self.request(
            "account/login/start",
            json!({
                "type": "chatgpt",
                "useHostedLoginSuccessPage": true,
                "appBrand": "chatgpt",
            }),
            PendingRequest::Login,
        ) {
            self.login_active = false;
            return Err(error);
        }
        Ok(())
    }

    fn open_chat(&mut self) -> Result<(), String> {
        self.chat_requested = true;
        self.ensure_thread()
    }

    fn queue_chat_message(
        &mut self,
        message: String,
        model: String,
        reasoning_effort: String,
    ) -> Result<(), String> {
        self.chat_requested = true;
        self.queued_messages.push(QueuedChatMessage {
            message,
            model,
            reasoning_effort,
        });
        self.ensure_thread()?;
        self.start_queued_turn()
    }

    fn ensure_thread(&mut self) -> Result<(), String> {
        if !self.chat_requested
            || !self.initialized
            || self.thread_id.is_some()
            || self.thread_start_pending
        {
            return Ok(());
        }
        self.thread_start_pending = true;
        let cwd = self.project_root.to_string_lossy().into_owned();
        if let Err(error) = self.request(
            "thread/start",
            json!({
                "cwd": cwd,
                "approvalPolicy": "never",
                "sandbox": "workspace-write",
                "serviceName": "cubacadabra_studio",
            }),
            PendingRequest::ThreadStart,
        ) {
            self.thread_start_pending = false;
            return Err(error);
        }
        Ok(())
    }

    fn start_queued_turn(&mut self) -> Result<(), String> {
        if !self.initialized || self.turn_active {
            return Ok(());
        }
        let Some(thread_id) = self.thread_id.clone() else {
            return Ok(());
        };
        let Some(message) = self.queued_messages.first().cloned() else {
            return Ok(());
        };
        self.turn_active = true;
        let cwd = self.project_root.to_string_lossy().into_owned();
        if let Err(error) = self.request(
            "turn/start",
            json!({
                "threadId": thread_id,
                "input": [{ "type": "text", "text": message.message }],
                "model": message.model,
                "effort": message.reasoning_effort,
                "cwd": cwd,
                "approvalPolicy": "never",
                "sandboxPolicy": {
                    "type": "workspaceWrite",
                    "writableRoots": [cwd],
                    "networkAccess": false,
                },
                "summary": "concise",
            }),
            PendingRequest::TurnStart,
        ) {
            self.turn_active = false;
            return Err(error);
        }
        self.queued_messages.remove(0);
        Ok(())
    }

    fn set_project_root(&mut self, project_root: PathBuf) -> Result<(), String> {
        self.project_root = project_root;
        self.thread_id = None;
        self.thread_start_pending = false;
        self.turn_active = false;
        self.queued_messages.clear();
        self.pending.retain(|_, pending| {
            !matches!(
                pending,
                PendingRequest::ThreadStart | PendingRequest::TurnStart
            )
        });
        self.ensure_thread()
    }
}

pub struct CodexClient {
    commands: Sender<CodexCommand>,
    events: Receiver<CodexEvent>,
    worker: Option<thread::JoinHandle<()>>,
}

impl CodexClient {
    pub fn new(project_root: &Path) -> Result<Self, String> {
        let (command_sender, command_receiver) = mpsc::channel();
        let (event_sender, event_receiver) = mpsc::channel();
        let project_root = project_root.to_owned();
        let worker = thread::Builder::new()
            .name("studio-codex".to_owned())
            .spawn(move || run_worker(&project_root, command_receiver, event_sender))
            .map_err(|error| format!("could not start the ChatGPT connection worker: {error}"))?;

        Ok(Self {
            commands: command_sender,
            events: event_receiver,
            worker: Some(worker),
        })
    }

    pub fn begin_chatgpt_login(&self) -> Result<(), String> {
        self.commands
            .send(CodexCommand::BeginChatGptLogin)
            .map_err(|_| {
                "ChatGPT connection is unavailable. Restart Studio and try again.".to_owned()
            })
    }

    pub fn open_chat(&self) -> Result<(), String> {
        self.commands
            .send(CodexCommand::OpenChat)
            .map_err(|_| "Codex chat is unavailable. Restart Studio and try again.".to_owned())
    }

    pub fn send_chat_message(
        &self,
        message: String,
        model: String,
        reasoning_effort: String,
    ) -> Result<(), String> {
        self.commands
            .send(CodexCommand::SendChatMessage {
                message,
                model,
                reasoning_effort,
            })
            .map_err(|_| "Codex chat is unavailable. Restart Studio and try again.".to_owned())
    }

    pub fn set_project_root(&self, project_root: &Path) -> Result<(), String> {
        self.commands
            .send(CodexCommand::SetProjectRoot(project_root.to_owned()))
            .map_err(|_| "Codex chat is unavailable. Restart Studio and try again.".to_owned())
    }

    pub fn try_recv(&self) -> Option<CodexEvent> {
        self.events.try_recv().ok()
    }
}

impl Drop for CodexClient {
    fn drop(&mut self) {
        let _ = self.commands.send(CodexCommand::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_worker(project_root: &Path, commands: Receiver<CodexCommand>, events: Sender<CodexEvent>) {
    let mut child = match start_app_server(project_root) {
        Ok(child) => child,
        Err(message) => {
            let _ = events.send(CodexEvent::Unavailable(message));
            return;
        }
    };
    let Some(stdin) = child.stdin.take() else {
        let _ = events.send(CodexEvent::Unavailable(
            "Codex App Server did not provide an input stream.".to_owned(),
        ));
        let _ = child.kill();
        return;
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = events.send(CodexEvent::Unavailable(
            "Codex App Server did not provide an output stream.".to_owned(),
        ));
        let _ = child.kill();
        return;
    };

    let (output_sender, output_receiver) = mpsc::channel();
    let _reader = thread::Builder::new()
        .name("studio-codex-output".to_owned())
        .spawn(move || read_app_server_output(stdout, output_sender));

    let mut protocol = ProtocolState::new(stdin, project_root.to_owned());
    if let Err(message) = protocol.initialize() {
        let _ = events.send(CodexEvent::Unavailable(message));
        let _ = child.kill();
        return;
    }

    loop {
        loop {
            match commands.try_recv() {
                Ok(CodexCommand::BeginChatGptLogin) if protocol.initialized => {
                    if let Err(message) = protocol.begin_login() {
                        let _ = events.send(CodexEvent::Error(message));
                    }
                }
                Ok(CodexCommand::BeginChatGptLogin) => protocol.queued_login = true,
                Ok(CodexCommand::OpenChat) if protocol.initialized => {
                    if let Err(message) = protocol.open_chat() {
                        let _ = events.send(CodexEvent::ChatError(message));
                    }
                }
                Ok(CodexCommand::OpenChat) => protocol.chat_requested = true,
                Ok(CodexCommand::SendChatMessage {
                    message,
                    model,
                    reasoning_effort,
                }) if protocol.initialized => {
                    if let Err(message) =
                        protocol.queue_chat_message(message, model, reasoning_effort)
                    {
                        let _ = events.send(CodexEvent::ChatError(message));
                    }
                }
                Ok(CodexCommand::SendChatMessage {
                    message,
                    model,
                    reasoning_effort,
                }) => {
                    protocol.chat_requested = true;
                    protocol.queued_messages.push(QueuedChatMessage {
                        message,
                        model,
                        reasoning_effort,
                    });
                }
                Ok(CodexCommand::SetProjectRoot(project_root)) => {
                    if let Err(message) = protocol.set_project_root(project_root) {
                        let _ = events.send(CodexEvent::ChatError(message));
                    }
                }
                Ok(CodexCommand::Shutdown) | Err(TryRecvError::Disconnected) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return;
                }
                Err(TryRecvError::Empty) => break,
            }
        }

        match output_receiver.recv_timeout(WORKER_POLL_INTERVAL) {
            Ok(AppServerOutput::Message(message)) => {
                if let Err(message) = handle_app_server_message(&mut protocol, message, &events) {
                    protocol.login_active = false;
                    if protocol.initialized {
                        let _ = events.send(CodexEvent::Error(message));
                    } else {
                        let _ = events.send(CodexEvent::Unavailable(message));
                        let _ = child.kill();
                        let _ = child.wait();
                        return;
                    }
                }
            }
            Ok(AppServerOutput::Invalid(message)) => {
                log::warn!("Codex App Server returned invalid JSON: {message}");
            }
            Ok(AppServerOutput::Closed) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                let detail = child
                    .try_wait()
                    .ok()
                    .flatten()
                    .map(|status| format!(" ({status})"))
                    .unwrap_or_default();
                let _ = events.send(CodexEvent::Unavailable(format!(
                    "Codex App Server stopped{detail}. Restart Studio and try again."
                )));
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

fn start_app_server(project_root: &Path) -> Result<Child, String> {
    let executable = codex_executable();
    let mut command = ProcessCommand::new(&executable);
    command
        .arg("app-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if project_root.is_dir() {
        command.current_dir(project_root);
    }
    command.spawn().map_err(|error| {
        format!(
            "Could not start Codex App Server from {}: {error}. Bundle Codex with Studio or set {CODEX_PATH_ENV}.",
            executable.display()
        )
    })
}

fn codex_executable() -> PathBuf {
    if let Some(path) = env::var_os(CODEX_PATH_ENV).filter(|value| !value.is_empty()) {
        return PathBuf::from(path);
    }

    if let Ok(current_executable) = env::current_exe()
        && let Some(executable_dir) = current_executable.parent()
    {
        let filename = if cfg!(target_os = "windows") {
            "codex.exe"
        } else {
            "codex"
        };
        let sibling = executable_dir.join(filename);
        if sibling.is_file() {
            return sibling;
        }
        if let Some(contents_dir) = executable_dir.parent() {
            let macos_resource = contents_dir.join("Resources").join(filename);
            if macos_resource.is_file() {
                return macos_resource;
            }
        }
    }

    PathBuf::from(if cfg!(target_os = "windows") {
        "codex.exe"
    } else {
        "codex"
    })
}

fn read_app_server_output(stdout: std::process::ChildStdout, output: Sender<AppServerOutput>) {
    for line in BufReader::new(stdout).lines() {
        match line {
            Ok(line) if line.trim().is_empty() => continue,
            Ok(line) => match serde_json::from_str(&line) {
                Ok(message) => {
                    if output.send(AppServerOutput::Message(message)).is_err() {
                        return;
                    }
                }
                Err(error) => {
                    let _ = output.send(AppServerOutput::Invalid(error.to_string()));
                }
            },
            Err(error) => {
                let _ = output.send(AppServerOutput::Invalid(error.to_string()));
                break;
            }
        }
    }
    let _ = output.send(AppServerOutput::Closed);
}

fn write_message(stdin: &mut ChildStdin, message: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *stdin, message)
        .map_err(|error| format!("could not encode a Codex App Server request: {error}"))?;
    stdin
        .write_all(b"\n")
        .and_then(|_| stdin.flush())
        .map_err(|error| format!("could not send a request to Codex App Server: {error}"))
}

fn handle_app_server_message(
    protocol: &mut ProtocolState,
    message: Value,
    events: &Sender<CodexEvent>,
) -> Result<(), String> {
    if let Some(id) = message.get("id").and_then(Value::as_u64) {
        let Some(pending) = protocol.pending.remove(&id) else {
            return Ok(());
        };
        if let Some(error) = message.get("error") {
            let detail = format_pending_error(pending, error);
            match pending {
                PendingRequest::Login => protocol.login_active = false,
                PendingRequest::ThreadStart | PendingRequest::TurnStart => {
                    protocol.thread_start_pending = false;
                    protocol.turn_active = false;
                    let _ = events.send(CodexEvent::ChatError(detail));
                    return Ok(());
                }
                PendingRequest::Initialize | PendingRequest::Account { .. } => {}
            }
            return Err(detail);
        }
        let result = message
            .get("result")
            .ok_or_else(|| "Codex App Server returned an incomplete response.".to_owned())?;
        return match pending {
            PendingRequest::Initialize => {
                protocol.notify("initialized", json!({}))?;
                protocol.initialized = true;
                protocol.read_account(false)?;
                if protocol.queued_login {
                    protocol.queued_login = false;
                    protocol.begin_login()?;
                }
                Ok(())
            }
            PendingRequest::Account { completes_login } => {
                let account = parse_account_result(result)?;
                if completes_login && protocol.login_active {
                    let account = account.ok_or_else(|| {
                        "ChatGPT sign-in finished without a connected account.".to_owned()
                    })?;
                    protocol.login_active = false;
                    let _ = events.send(CodexEvent::LoginCompleted(account));
                } else {
                    let _ = events.send(CodexEvent::AccountStatus(account));
                }
                Ok(())
            }
            PendingRequest::Login => {
                let auth_url = result
                    .get("authUrl")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        protocol.login_active = false;
                        "Codex App Server did not return a ChatGPT sign-in URL.".to_owned()
                    })?;
                if !is_allowed_auth_url(auth_url) {
                    protocol.login_active = false;
                    return Err(
                        "Codex App Server returned an invalid ChatGPT sign-in URL.".to_owned()
                    );
                }
                if !open_browser(auth_url) {
                    protocol.login_active = false;
                    return Err(
                        "Could not open the browser for ChatGPT sign-in. Check your browser association and try again."
                            .to_owned(),
                    );
                }
                let _ = events.send(CodexEvent::BrowserOpened);
                Ok(())
            }
            PendingRequest::ThreadStart => {
                let thread_id = result
                    .pointer("/thread/id")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        "Codex App Server returned an incomplete chat thread.".to_owned()
                    })?;
                protocol.thread_start_pending = false;
                protocol.thread_id = Some(thread_id.to_owned());
                let _ = events.send(CodexEvent::ChatReady);
                if let Err(message) = protocol.start_queued_turn() {
                    let _ = events.send(CodexEvent::ChatError(message));
                }
                Ok(())
            }
            PendingRequest::TurnStart => Ok(()),
        };
    }

    match message.get("method").and_then(Value::as_str) {
        Some("account/login/completed") => {
            let params = message.get("params").unwrap_or(&Value::Null);
            if params.get("success").and_then(Value::as_bool) == Some(true) {
                protocol.read_account(true)?;
            } else {
                protocol.login_active = false;
                let detail = params
                    .get("error")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("The ChatGPT sign-in was cancelled or could not be completed.");
                let _ = events.send(CodexEvent::Error(detail.to_owned()));
            }
        }
        Some("account/updated") => {
            let auth_mode = message.pointer("/params/authMode").and_then(Value::as_str);
            if matches!(auth_mode, Some("chatgpt" | "chatgptAuthTokens")) {
                protocol.read_account(protocol.login_active)?;
            } else if !protocol.login_active {
                let _ = events.send(CodexEvent::AccountStatus(None));
            }
        }
        Some("item/agentMessage/delta") => {
            if let Some(delta) = message
                .pointer("/params/delta")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
            {
                let _ = events.send(CodexEvent::AssistantDelta(delta.to_owned()));
            }
        }
        Some("item/completed") => {
            if message.pointer("/params/item/type").and_then(Value::as_str) == Some("agentMessage")
                && let Some(text) = message.pointer("/params/item/text").and_then(Value::as_str)
            {
                let _ = events.send(CodexEvent::AssistantMessage(text.to_owned()));
            }
        }
        Some("turn/completed") => {
            protocol.turn_active = false;
            let status = message
                .pointer("/params/turn/status")
                .and_then(Value::as_str);
            if status == Some("failed") {
                let detail = message
                    .pointer("/params/turn/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("Codex could not complete that request.");
                let _ = events.send(CodexEvent::ChatError(detail.to_owned()));
            } else {
                let _ = events.send(CodexEvent::ChatTurnCompleted);
            }
            protocol.start_queued_turn()?;
        }
        Some("error") => {
            let detail = message
                .pointer("/params/error/message")
                .and_then(Value::as_str)
                .unwrap_or("Codex returned an error.");
            let _ = events.send(CodexEvent::ChatError(detail.to_owned()));
        }
        _ => {}
    }
    Ok(())
}

fn parse_account_result(result: &Value) -> Result<Option<ChatGptAccount>, String> {
    let Some(account) = result.get("account") else {
        return Err("Codex App Server returned an incomplete account status.".to_owned());
    };
    if account.is_null() {
        return Ok(None);
    }
    let account_type = account.get("type").and_then(Value::as_str);
    if !matches!(account_type, Some("chatgpt" | "chatgptAuthTokens")) {
        return Ok(None);
    }
    Ok(Some(ChatGptAccount {
        email: optional_nonempty_string(account.get("email")),
        plan_type: optional_nonempty_string(account.get("planType")),
    }))
}

fn optional_nonempty_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn format_pending_error(pending: PendingRequest, error: &Value) -> String {
    let detail = error
        .get("message")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown error");
    let action = match pending {
        PendingRequest::Initialize => "initialize Codex App Server",
        PendingRequest::Account { .. } => "read the ChatGPT account",
        PendingRequest::Login => "start ChatGPT sign-in",
        PendingRequest::ThreadStart => "start the Codex chat",
        PendingRequest::TurnStart => "send the Codex message",
    };
    format!("Could not {action}: {detail}")
}

fn is_allowed_auth_url(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    if url.scheme() != "https" {
        return false;
    }
    url.host_str().is_some_and(|host| {
        host == "chatgpt.com"
            || host.ends_with(".chatgpt.com")
            || host == "openai.com"
            || host.ends_with(".openai.com")
    })
}

fn open_browser(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    let mut command = ProcessCommand::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = ProcessCommand::new("cmd");
        command.args(["/C", "start", ""]);
        command
    };
    #[cfg(target_os = "linux")]
    let mut command = ProcessCommand::new("xdg-open");
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    return false;
    command.arg(url).spawn().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chatgpt_account_status() {
        let account = parse_account_result(&json!({
            "account": {
                "type": "chatgpt",
                "email": " player@example.com ",
                "planType": "plus"
            },
            "requiresOpenaiAuth": true
        }))
        .unwrap()
        .unwrap();

        assert_eq!(account.email.as_deref(), Some("player@example.com"));
        assert_eq!(account.plan_type.as_deref(), Some("plus"));
    }

    #[test]
    fn ignores_non_chatgpt_accounts() {
        let account = parse_account_result(&json!({
            "account": { "type": "apiKey" },
            "requiresOpenaiAuth": true
        }))
        .unwrap();

        assert_eq!(account, None);
    }

    #[test]
    fn accepts_signed_out_account_status() {
        let account = parse_account_result(&json!({
            "account": null,
            "requiresOpenaiAuth": true
        }))
        .unwrap();

        assert_eq!(account, None);
    }

    #[test]
    fn only_opens_secure_openai_auth_urls() {
        assert!(is_allowed_auth_url("https://chatgpt.com/auth/login"));
        assert!(is_allowed_auth_url("https://auth.openai.com/codex/device"));
        assert!(!is_allowed_auth_url("http://chatgpt.com/auth/login"));
        assert!(!is_allowed_auth_url(
            "https://chatgpt.com.example.com/login"
        ));
        assert!(!is_allowed_auth_url("file:///tmp/login"));
    }
}
