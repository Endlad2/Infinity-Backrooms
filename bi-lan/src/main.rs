use anyhow::{anyhow, Context, Result};
use axum::{
    extract::{ws::{Message as AxumMessage, WebSocket, WebSocketUpgrade}, Query, State},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use bytes::Bytes;
use clap::{Parser, Subcommand};
use futures_util::{SinkExt, StreamExt};
use quinn::{ClientConfig, Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::{Deserialize, Serialize};
use std::{net::{IpAddr, Ipv4Addr, SocketAddr}, sync::{Arc, atomic::{AtomicUsize, Ordering}}};
use tokio::{io::{AsyncBufReadExt, BufReader}, net::TcpListener, sync::{broadcast, mpsc}};
use tokio_tungstenite::{accept_async, connect_async, tungstenite::Message as TungsteniteMessage};

const MAX_PLAYERS: usize = 5;

#[derive(Parser, Debug)]
#[command(name = "bi-lan", about = "Backrooms Infinity LAN sidecar")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Host {
        #[arg(long)] nickname: String,
        #[arg(long, default_value_t = 37600)] ws_port: u16,
        #[arg(long, default_value_t = 37601)] quic_port: u16,
        #[arg(long, default_value_t = 37610)] bridge_port: u16,
    },
    Join {
        #[arg(long)] host: String,
        #[arg(long)] nickname: String,
        #[arg(long, default_value_t = 37600)] ws_port: u16,
        #[arg(long, default_value_t = 37610)] bridge_port: u16,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum LobbyMessage {
    Welcome { quic_port: u16, cert_b64: String, players: usize, max_players: usize },
    PlayerCount { players: usize, max_players: usize },
    Start { level: String },
    Error { message: String },
}

#[derive(Debug, Deserialize)]
struct JoinQuery { nickname: String }

#[derive(Clone)]
struct HostState {
    players: Arc<AtomicUsize>,
    start_tx: broadcast::Sender<String>,
    cert_b64: String,
    quic_port: u16,
    event_tx: mpsc::UnboundedSender<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct StdinCommand { command: String, level: Option<String> }

fn emit(value: serde_json::Value) { println!("{}", value); }

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Host { nickname, ws_port, quic_port, bridge_port } => run_host(nickname, ws_port, quic_port, bridge_port).await,
        Command::Join { host, nickname, ws_port, bridge_port } => run_join(host, nickname, ws_port, bridge_port).await,
    }
}

async fn run_host(nickname: String, ws_port: u16, quic_port: u16, bridge_port: u16) -> Result<()> {
    let cert = rcgen::generate_simple_self_signed(vec!["backrooms-infinity.local".into()])?;
    let cert_der = CertificateDer::from(cert.cert.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(cert.key_pair.serialize_der()));
    let server_config = ServerConfig::with_single_cert(vec![cert_der.clone()], key_der)?;
    let quic_endpoint = Endpoint::server(server_config, SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), quic_port))?;
    let cert_b64 = B64.encode(cert_der.as_ref());

    let (start_tx, _) = broadcast::channel::<String>(8);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<serde_json::Value>();
    let state = HostState {
        players: Arc::new(AtomicUsize::new(1)),
        start_tx: start_tx.clone(), cert_b64, quic_port, event_tx: event_tx.clone(),
    };

    let router = Router::new()
        .route("/health", get(|| async { Json(serde_json::json!({"ok":true,"game":"Backrooms Infinity"})) }))
        .route("/lobby", get(lobby_ws))
        .with_state(state.clone());
    let ws_listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, ws_port)).await?;
    tokio::spawn(async move { let _ = axum::serve(ws_listener, router).await; });

    let (net_tx, _) = broadcast::channel::<Bytes>(256);
    spawn_host_quic(quic_endpoint, net_tx.clone());
    spawn_local_bridge(bridge_port, BridgeRole::Host { net_tx: net_tx.clone() }).await?;

    let ip = local_ip_address::local_ip().unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    emit(serde_json::json!({"event":"host_ready","ip":ip.to_string(),"ws_port":ws_port,"quic_port":quic_port,"bridge":format!("ws://127.0.0.1:{}/client", bridge_port),"nickname":nickname,"players":1,"max_players":MAX_PLAYERS}));

    let start_tx_for_stdin = start_tx.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(tokio::io::stdin()).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(cmd) = serde_json::from_str::<StdinCommand>(&line) {
                if cmd.command == "start" {
                    let level = cmd.level.unwrap_or_else(|| "unknown".into());
                    let _ = start_tx_for_stdin.send(level.clone());
                    emit(serde_json::json!({"event":"start","level":level}));
                }
            }
        }
    });

    while let Some(ev) = event_rx.recv().await { emit(ev); }
    Ok(())
}

async fn lobby_ws(State(state): State<HostState>, Query(query): Query<JoinQuery>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_lobby(socket, query.nickname, state))
}

async fn handle_lobby(mut socket: WebSocket, nickname: String, state: HostState) {
    let current = state.players.fetch_add(1, Ordering::SeqCst) + 1;
    if current > MAX_PLAYERS {
        state.players.fetch_sub(1, Ordering::SeqCst);
        let msg = LobbyMessage::Error { message: "Lobby is full (5 players max)".into() };
        let _ = socket.send(AxumMessage::Text(serde_json::to_string(&msg).unwrap())).await;
        return;
    }
    let _ = state.event_tx.send(serde_json::json!({"event":"player_count","players":current,"max_players":MAX_PLAYERS,"joined":nickname}));
    let welcome = LobbyMessage::Welcome { quic_port: state.quic_port, cert_b64: state.cert_b64.clone(), players: current, max_players: MAX_PLAYERS };
    if socket.send(AxumMessage::Text(serde_json::to_string(&welcome).unwrap())).await.is_err() { state.players.fetch_sub(1, Ordering::SeqCst); return; }

    let mut starts = state.start_tx.subscribe();
    loop {
        tokio::select! {
            Ok(level) = starts.recv() => {
                let msg = LobbyMessage::Start { level };
                let _ = socket.send(AxumMessage::Text(serde_json::to_string(&msg).unwrap())).await;
            }
            msg = socket.recv() => {
                match msg { Some(Ok(AxumMessage::Close(_))) | None | Some(Err(_)) => break, _ => {} }
            }
        }
    }
    let now = state.players.fetch_sub(1, Ordering::SeqCst).saturating_sub(1);
    let _ = state.event_tx.send(serde_json::json!({"event":"player_count","players":now,"max_players":MAX_PLAYERS,"left":nickname}));
}

fn spawn_host_quic(endpoint: Endpoint, net_tx: broadcast::Sender<Bytes>) {
    tokio::spawn(async move {
        while let Some(incoming) = endpoint.accept().await {
            let tx = net_tx.clone();
            tokio::spawn(async move {
                let Ok(conn) = incoming.await else { return; };
                let mut rx = tx.subscribe();
                let send_conn = conn.clone();
                tokio::spawn(async move {
                    while let Ok(data) = rx.recv().await { let _ = send_conn.send_datagram(data); }
                });
                while let Ok(data) = conn.read_datagram().await { let _ = tx.send(data); }
            });
        }
    });
}

enum BridgeRole {
    Host { net_tx: broadcast::Sender<Bytes> },
    Join { connection: quinn::Connection },
}

async fn spawn_local_bridge(port: u16, role: BridgeRole) -> Result<()> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await.context("bind local client bridge")?;
    match role {
        BridgeRole::Host { net_tx } => {
            tokio::spawn(async move {
                loop {
                    let Ok((stream, _)) = listener.accept().await else { break; };
                    let tx = net_tx.clone();
                    tokio::spawn(async move {
                        let Ok(ws) = accept_async(stream).await else { return; };
                        let (mut sink, mut source) = ws.split();
                        let mut rx = tx.subscribe();
                        loop {
                            tokio::select! {
                                Some(Ok(msg)) = source.next() => if let Ok(text) = msg.to_text() { let _ = tx.send(Bytes::copy_from_slice(text.as_bytes())); },
                                Ok(data) = rx.recv() => { if sink.send(TungsteniteMessage::Text(String::from_utf8_lossy(&data).to_string().into())).await.is_err() { break; } }
                                else => break,
                            }
                        }
                    });
                }
            });
        }
        BridgeRole::Join { connection } => {
            tokio::spawn(async move {
                loop {
                    let Ok((stream, _)) = listener.accept().await else { break; };
                    let conn = connection.clone();
                    tokio::spawn(async move {
                        let Ok(ws) = accept_async(stream).await else { return; };
                        let (mut sink, mut source) = ws.split();
                        loop {
                            tokio::select! {
                                Some(Ok(msg)) = source.next() => if let Ok(text) = msg.to_text() { let _ = conn.send_datagram(Bytes::copy_from_slice(text.as_bytes())); },
                                data = conn.read_datagram() => match data { Ok(data) => { if sink.send(TungsteniteMessage::Text(String::from_utf8_lossy(&data).to_string().into())).await.is_err() { break; } }, Err(_) => break },
                                else => break,
                            }
                        }
                    });
                }
            });
        }
    }
    Ok(())
}

async fn run_join(host: String, nickname: String, ws_port: u16, bridge_port: u16) -> Result<()> {
    let ws_url = format!("ws://{}:{}/lobby?nickname={}", host, ws_port, urlencoding::encode(&nickname));
    let (ws, _) = connect_async(&ws_url).await.context("host WebSocket is unreachable")?;
    let (mut ws_sink, mut ws_source) = ws.split();
    ws_sink.send(TungsteniteMessage::Ping(Vec::new().into())).await?;

    let first = ws_source.next().await.ok_or_else(|| anyhow!("host closed lobby"))??;
    let welcome: LobbyMessage = serde_json::from_str(first.to_text()?)?;
    let (quic_port, cert_b64, players, max_players) = match welcome {
        LobbyMessage::Welcome { quic_port, cert_b64, players, max_players } => (quic_port, cert_b64, players, max_players),
        LobbyMessage::Error { message } => return Err(anyhow!(message)),
        _ => return Err(anyhow!("unexpected lobby response")),
    };

    let cert_bytes = B64.decode(cert_b64)?;
    let mut roots = rustls::RootCertStore::empty();
    roots.add(CertificateDer::from(cert_bytes))?;
    let crypto = rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
    let quic_crypto = quinn::crypto::rustls::QuicClientConfig::try_from(crypto)?;
    let mut endpoint = Endpoint::client(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0))?;
    endpoint.set_default_client_config(ClientConfig::new(Arc::new(quic_crypto)));
    let addr: SocketAddr = format!("{}:{}", host, quic_port).parse().context("host must be an IPv4/IPv6 address")?;
    let connection = endpoint.connect(addr, "backrooms-infinity.local")?.await.context("QUIC connect failed")?;
    spawn_local_bridge(bridge_port, BridgeRole::Join { connection }).await?;

    emit(serde_json::json!({"event":"connected","host":host,"players":players,"max_players":max_players,"bridge":format!("ws://127.0.0.1:{}/client", bridge_port)}));

    while let Some(msg) = ws_source.next().await {
        let msg = msg?;
        if let Ok(text) = msg.to_text() {
            match serde_json::from_str::<LobbyMessage>(text) {
                Ok(LobbyMessage::Start { level }) => emit(serde_json::json!({"event":"start","level":level})),
                Ok(LobbyMessage::PlayerCount { players, max_players }) => emit(serde_json::json!({"event":"player_count","players":players,"max_players":max_players})),
                Ok(LobbyMessage::Error { message }) => emit(serde_json::json!({"event":"error","message":message})),
                _ => {}
            }
        }
    }
    Err(anyhow!("host disconnected"))
}
