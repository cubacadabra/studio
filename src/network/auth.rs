use super::*;

pub(super) fn run_browser_auth(
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

pub(super) fn web_base_url(backend_url: &Url) -> Result<Url, String> {
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
