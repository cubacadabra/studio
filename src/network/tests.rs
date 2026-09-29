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
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tools/tests/fixtures/conformance-game");
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
    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tools/tests/fixtures/conformance-game");
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
        auth_flow::web_base_url(&backend).unwrap().as_str(),
        "https://cubacadabra.com/login/"
    );
}
