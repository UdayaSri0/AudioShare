use std::{
    fs::File,
    io::{self, BufRead, BufReader, Read, Write},
    net::{IpAddr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::metadata;

const BROWSER_JOIN_TOKEN_BYTES: usize = 18;
const BROWSER_JOIN_TOKEN_TTL_SECS: u64 = 15 * 60;
const BROWSER_JOIN_ACCEPT_POLL_MS: u64 = 100;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserJoinSnapshot {
    pub active: bool,
    pub bind_addr: Option<String>,
    pub join_url: Option<String>,
    pub token_expires_at_unix_ms: Option<u64>,
    pub requests_served: u64,
    pub last_request_path: Option<String>,
    pub last_request_at_unix_ms: Option<u64>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedJoinLink {
    pub url: String,
    pub expires_at_unix_ms: u64,
}

pub struct BrowserJoinPrototypeService {
    state: Arc<Mutex<BrowserJoinState>>,
}

#[derive(Debug)]
struct BrowserJoinState {
    snapshot: BrowserJoinSnapshot,
    current_token: Option<String>,
    stop_flag: Option<Arc<AtomicBool>>,
    worker: Option<JoinHandle<()>>,
}

impl BrowserJoinPrototypeService {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(BrowserJoinState {
                snapshot: BrowserJoinSnapshot::default(),
                current_token: None,
                stop_flag: None,
                worker: None,
            })),
        }
    }

    pub fn snapshot(&self) -> BrowserJoinSnapshot {
        self.state
            .lock()
            .map(|state| state.snapshot.clone())
            .unwrap_or_default()
    }

    pub fn current_join_url(&self) -> Option<String> {
        self.snapshot().join_url
    }

    pub fn generate_join_link(
        &mut self,
        preferred_host: Option<IpAddr>,
    ) -> Result<GeneratedJoinLink, io::Error> {
        let bind_addr = self.ensure_running()?;
        let host_ip = preferred_host
            .filter(|host| is_browser_join_host_override(*host))
            .or_else(detect_local_join_host)
            .unwrap_or(bind_addr.ip());
        let token = generate_join_token()?;
        let expires_at_unix_ms = now_unix_ms().saturating_add(BROWSER_JOIN_TOKEN_TTL_SECS * 1_000);
        let join_url = format!("http://{}:{}/join/{}", host_ip, bind_addr.port(), token);

        if let Ok(mut state) = self.state.lock() {
            state.current_token = Some(token);
            state.snapshot.join_url = Some(join_url.clone());
            state.snapshot.token_expires_at_unix_ms = Some(expires_at_unix_ms);
            state.snapshot.last_error = None;
        }

        tracing::info!(
            bind_addr = %bind_addr,
            join_url = %join_url,
            expires_at_unix_ms,
            "generated browser join prototype link"
        );

        Ok(GeneratedJoinLink {
            url: join_url,
            expires_at_unix_ms,
        })
    }

    pub fn stop(&mut self) -> Result<(), io::Error> {
        let (stop_flag, worker) = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| io::Error::other("browser join state mutex poisoned"))?;
            state.snapshot.active = false;
            state.snapshot.join_url = None;
            state.snapshot.token_expires_at_unix_ms = None;
            state.current_token = None;
            (state.stop_flag.take(), state.worker.take())
        };

        if let Some(stop_flag) = stop_flag {
            stop_flag.store(true, Ordering::SeqCst);
        }
        if let Some(worker) = worker {
            worker
                .join()
                .map_err(|_| io::Error::other("browser join worker thread panicked"))?;
        }

        Ok(())
    }

    fn ensure_running(&mut self) -> Result<SocketAddr, io::Error> {
        if let Ok(state) = self.state.lock() {
            if state.snapshot.active {
                if let Some(bind_addr) = &state.snapshot.bind_addr {
                    if let Ok(bind_addr) = bind_addr.parse::<SocketAddr>() {
                        return Ok(bind_addr);
                    }
                }
            }
        }

        let listener = TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 0)))?;
        listener.set_nonblocking(true)?;
        let bind_addr = listener.local_addr()?;
        let stop_flag = Arc::new(AtomicBool::new(false));
        let state = Arc::clone(&self.state);
        let worker_stop_flag = Arc::clone(&stop_flag);
        let worker = thread::spawn(move || {
            browser_join_listener_loop(listener, state, worker_stop_flag);
        });

        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("browser join state mutex poisoned"))?;
        state.snapshot.active = true;
        state.snapshot.bind_addr = Some(bind_addr.to_string());
        state.snapshot.last_error = None;
        state.stop_flag = Some(stop_flag);
        state.worker = Some(worker);
        Ok(bind_addr)
    }
}

impl Drop for BrowserJoinPrototypeService {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn browser_join_listener_loop(
    listener: TcpListener,
    state: Arc<Mutex<BrowserJoinState>>,
    stop_flag: Arc<AtomicBool>,
) {
    while !stop_flag.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _peer_addr)) => {
                if let Err(error) = handle_browser_join_request(stream, &state) {
                    tracing::warn!(error = %error, "browser join prototype request failed");
                    if let Ok(mut state) = state.lock() {
                        state.snapshot.last_error = Some(error.to_string());
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(BROWSER_JOIN_ACCEPT_POLL_MS));
            }
            Err(error) => {
                tracing::warn!(error = %error, "browser join prototype listener accept failed");
                if let Ok(mut state) = state.lock() {
                    state.snapshot.last_error = Some(error.to_string());
                }
                thread::sleep(Duration::from_millis(BROWSER_JOIN_ACCEPT_POLL_MS));
            }
        }
    }
}

fn handle_browser_join_request(
    mut stream: TcpStream,
    state: &Arc<Mutex<BrowserJoinState>>,
) -> Result<(), io::Error> {
    let mut request_line = String::new();
    {
        let mut reader = BufReader::new(stream.try_clone()?);
        reader.read_line(&mut request_line)?;
        if request_line.trim().is_empty() {
            return Ok(());
        }

        loop {
            let mut header = String::new();
            let bytes_read = reader.read_line(&mut header)?;
            if bytes_read == 0 || header == "\r\n" {
                break;
            }
        }
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");
    let (token, valid_token, expires_at_unix_ms) = {
        let mut state = state
            .lock()
            .map_err(|_| io::Error::other("browser join state mutex poisoned"))?;
        state.snapshot.requests_served = state.snapshot.requests_served.saturating_add(1);
        state.snapshot.last_request_path = Some(path.to_string());
        state.snapshot.last_request_at_unix_ms = Some(now_unix_ms());
        let token = state.current_token.clone();
        let valid_token = token
            .as_deref()
            .zip(path.rsplit('/').next())
            .is_some_and(|(token, path_token)| token == path_token)
            && state
                .snapshot
                .token_expires_at_unix_ms
                .is_some_and(|expires_at| expires_at >= now_unix_ms());
        (token, valid_token, state.snapshot.token_expires_at_unix_ms)
    };

    if method != "GET" {
        return write_http_response(
            &mut stream,
            "405 Method Not Allowed",
            "text/plain; charset=utf-8",
            b"Only GET is supported by the browser join prototype.\n",
        );
    }

    match path {
        "/" => write_http_response(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            prototype_home_page(token.as_deref(), expires_at_unix_ms).as_bytes(),
        ),
        path if path.starts_with("/join/") => {
            if valid_token {
                write_http_response(
                    &mut stream,
                    "200 OK",
                    "text/html; charset=utf-8",
                    prototype_join_page(expires_at_unix_ms).as_bytes(),
                )
            } else {
                write_http_response(
                    &mut stream,
                    "403 Forbidden",
                    "text/html; charset=utf-8",
                    prototype_denied_page().as_bytes(),
                )
            }
        }
        path if path.starts_with("/api/join/") => {
            let body = serde_json::json!({
                "app_name": metadata::APP_NAME,
                "app_version": metadata::APP_VERSION,
                "prototype": true,
                "browser_audio_streaming_implemented": false,
                "valid_token": valid_token,
                "token_expires_at_unix_ms": expires_at_unix_ms,
                "message": "This endpoint is only a signaling/session prototype boundary. Browser playback is not implemented in this build."
            });
            write_http_response(
                &mut stream,
                if valid_token {
                    "200 OK"
                } else {
                    "403 Forbidden"
                },
                "application/json; charset=utf-8",
                serde_json::to_string_pretty(&body)?.as_bytes(),
            )
        }
        _ => write_http_response(
            &mut stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            b"Browser join prototype route not found.\n",
        ),
    }
}

fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> Result<(), io::Error> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)?;
    stream.flush()
}

fn prototype_home_page(token: Option<&str>, expires_at_unix_ms: Option<u64>) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{name} Browser Join Prototype</title></head><body><h1>{name} Browser Join Prototype</h1><p>This page is an honest prototype boundary for future no-install guest joining.</p><p>Browser audio playback is not implemented in this build.</p><p>Current token: {token}</p><p>Expires: {expires}</p></body></html>",
        name = metadata::APP_NAME,
        token = token.unwrap_or("not generated"),
        expires = expires_at_unix_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "not generated".to_string()),
    )
}

fn prototype_join_page(expires_at_unix_ms: Option<u64>) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{name} Guest Join Prototype</title></head><body><h1>{name} Guest Join Prototype</h1><p>This join link is valid and the host-side prototype endpoint is reachable.</p><p>Browser playback has not been implemented yet, so this page does not start audio.</p><p>Token expiry: {expires}</p><p>Future work: signaling, WebRTC transport, AudioWorklet playback, and guest diagnostics export.</p></body></html>",
        name = metadata::APP_NAME,
        expires = expires_at_unix_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
    )
}

fn prototype_denied_page() -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{name} Join Link Rejected</title></head><body><h1>Join Link Rejected</h1><p>The token is missing, invalid, or expired.</p><p>This prototype does not provide browser playback yet.</p></body></html>",
        name = metadata::APP_NAME,
    )
}

fn generate_join_token() -> Result<String, io::Error> {
    let mut file = File::open("/dev/urandom")?;
    let mut bytes = [0_u8; BROWSER_JOIN_TOKEN_BYTES];
    file.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn detect_local_join_host() -> Option<IpAddr> {
    preferred_local_ip_for("8.8.8.8:9")
        .or_else(|| preferred_local_ip_for("[2001:4860:4860::8888]:9"))
        .or_else(preferred_local_ip_for_hostname)
}

fn preferred_local_ip_for(target: &str) -> Option<IpAddr> {
    let target = target.to_socket_addrs().ok()?.next()?;
    let bind_addr = match target {
        SocketAddr::V4(_) => "0.0.0.0:0",
        SocketAddr::V6(_) => "[::]:0",
    };
    let socket = UdpSocket::bind(bind_addr).ok()?;
    socket.connect(target).ok()?;
    let local_ip = socket.local_addr().ok()?.ip();
    is_browser_join_host_candidate(local_ip).then_some(local_ip)
}

fn preferred_local_ip_for_hostname() -> Option<IpAddr> {
    let hostname = std::env::var("HOSTNAME").ok()?;
    format!("{hostname}:0")
        .to_socket_addrs()
        .ok()?
        .map(|address| address.ip())
        .find(|address| is_browser_join_host_candidate(*address))
}

fn is_browser_join_host_candidate(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            !(address.is_loopback() || address.is_unspecified() || address.is_multicast())
        }
        IpAddr::V6(address) => {
            !(address.is_loopback() || address.is_unspecified() || address.is_multicast())
        }
    }
}

fn is_browser_join_host_override(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => !(address.is_unspecified() || address.is_multicast()),
        IpAddr::V6(address) => !(address.is_unspecified() || address.is_multicast()),
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_join_link_returns_tokenized_url() {
        let mut service = BrowserJoinPrototypeService::new();

        let link = service
            .generate_join_link(Some(IpAddr::from([127, 0, 0, 1])))
            .expect("join link generation should succeed");

        assert!(link.url.starts_with("http://127.0.0.1:"));
        assert!(link.url.contains("/join/"));
        assert!(service.snapshot().active);
        assert_eq!(
            service.snapshot().join_url.as_deref(),
            Some(link.url.as_str())
        );
    }

    #[test]
    fn prototype_join_route_serves_honest_placeholder_page() {
        let mut service = BrowserJoinPrototypeService::new();
        let link = service
            .generate_join_link(Some(IpAddr::from([127, 0, 0, 1])))
            .expect("join link generation should succeed");

        let (host, port, path) = parse_http_url(&link.url).expect("join url should parse");
        let mut stream =
            TcpStream::connect((host.as_str(), port)).expect("prototype listener should accept");
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
        )
        .expect("request should write");
        let mut body = String::new();
        stream
            .read_to_string(&mut body)
            .expect("response should read");

        assert!(body.contains("200 OK"));
        assert!(body.contains("Browser playback has not been implemented yet"));
    }

    #[test]
    fn prototype_join_route_rejects_invalid_token() {
        let mut service = BrowserJoinPrototypeService::new();
        let link = service
            .generate_join_link(Some(IpAddr::from([127, 0, 0, 1])))
            .expect("join link generation should succeed");

        let (host, port, _) = parse_http_url(&link.url).expect("join url should parse");
        let mut stream =
            TcpStream::connect((host.as_str(), port)).expect("prototype listener should accept");
        write!(
            stream,
            "GET /join/not-the-right-token HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
        )
        .expect("request should write");
        let mut body = String::new();
        stream
            .read_to_string(&mut body)
            .expect("response should read");

        assert!(body.contains("403 Forbidden"));
        assert!(body.contains("Join Link Rejected"));
    }

    fn parse_http_url(url: &str) -> Option<(String, u16, String)> {
        let without_scheme = url.strip_prefix("http://")?;
        let (host_port, path) = without_scheme.split_once('/')?;
        let (host, port) = host_port.rsplit_once(':')?;
        Some((host.to_string(), port.parse().ok()?, format!("/{}", path)))
    }
}
