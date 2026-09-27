//! UDP-клиент лобби. Дополнительно: ChunkAnnounce/Request/Response
//! ретранслируются хостом между всеми участниками.

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use super::protocol::{NetPacket, PlayerState, DEFAULT_PORT, MAX_PACKET_SIZE, PROTOCOL_VERSION};

#[derive(Debug)]
pub enum ClientError {
    Io(std::io::Error),
    Dns(String),
    NotConnected,
    Timeout,
    Rejected(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::Io(e) => write!(f, "io error: {e}"),
            ClientError::Dns(s) => write!(f, "dns error: {s}"),
            ClientError::NotConnected => write!(f, "not connected"),
            ClientError::Timeout => write!(f, "connection timeout"),
            ClientError::Rejected(r) => write!(f, "connection rejected: {r}"),
        }
    }
}
impl std::error::Error for ClientError {}
impl From<std::io::Error> for ClientError { fn from(e: std::io::Error) -> Self { ClientError::Io(e) } }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientState {
    Disconnected,
    Connecting,
    Connected,
}

pub struct Client {
    socket: UdpSocket,
    host_addr: SocketAddr,
    pub state: ClientState,
    pub player_id: Option<u32>,
    pub last_snapshot: Option<super::protocol::WorldSnapshot>,
    pub last_error: Option<String>,
    pub input_seq: u32,
    pub local: PlayerState,
    connect_started: Option<Instant>,
    /// HTTP-порт хоста (приходит в ConnectAck).
    pub host_http_port: Option<u16>,
    /// Номер уровня лобби.
    pub host_level_number: Option<u32>,
}

impl Client {
    pub fn resolve_host(host: &str) -> Result<SocketAddr, ClientError> {
        let with_port = if host.contains(':') { host.to_string() } else { format!("{host}:{DEFAULT_PORT}") };
        let mut addrs = with_port.to_socket_addrs().map_err(|e| ClientError::Dns(e.to_string()))?;
        addrs.next().ok_or_else(|| ClientError::Dns("no address resolved".into()))
    }

    pub fn connect(host: &str, player_name: &str) -> Result<Self, ClientError> {
        let host_addr = Self::resolve_host(host)?;
        let socket = UdpSocket::bind(("0.0.0.0", 0))?;
        socket.set_nonblocking(true)?;

        let client = Client {
            socket,
            host_addr,
            state: ClientState::Connecting,
            player_id: None,
            last_snapshot: None,
            last_error: None,
            input_seq: 0,
            local: PlayerState::new(0),
            connect_started: Some(Instant::now()),
            host_http_port: None,
            host_level_number: None,
        };
        client.send(&NetPacket::Connect {
            version: PROTOCOL_VERSION,
            name: player_name.to_string(),
        })?;
        Ok(client)
    }

    pub fn host_addr(&self) -> SocketAddr { self.host_addr }

    /// Base URL HTTP-сервера хоста, если он известен.
    pub fn host_base_url(&self) -> Option<String> {
        let port = self.host_http_port?;
        let ip = self.host_addr.ip();
        Some(format!("http://{}:{}", ip, port))
    }

    pub fn send(&self, pkt: &NetPacket) -> Result<(), ClientError> {
        let bytes = pkt.encode();
        self.socket.send_to(&bytes, self.host_addr)?;
        Ok(())
    }

    pub fn poll(&mut self) -> Result<Vec<NetPacket>, ClientError> {
        let mut received = Vec::new();
        let mut buf = [0u8; MAX_PACKET_SIZE];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((n, from)) => {
                    if from != self.host_addr { continue; }
                    if let Ok(pkt) = NetPacket::decode(&buf[..n]) {
                        self.handle_incoming(&pkt);
                        received.push(pkt);
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(ClientError::Io(e)),
            }
        }
        Ok(received)
    }

    fn handle_incoming(&mut self, pkt: &NetPacket) {
        match pkt {
            NetPacket::ConnectAck { player_id, http_port, level_number, .. } => {
                self.player_id = Some(*player_id);
                self.local.id = *player_id;
                self.state = ClientState::Connected;
                self.connect_started = None;
                self.host_http_port = Some(*http_port);
                self.host_level_number = Some(*level_number);
            }
            NetPacket::Disconnect { reason } => {
                self.last_error = Some(reason.clone());
                self.state = ClientState::Disconnected;
                self.player_id = None;
            }
            NetPacket::WorldState(snap) => { self.last_snapshot = Some(snap.clone()); }
            _ => {}
        }
    }

    pub fn check_timeout(&mut self, timeout: Duration) -> Result<(), ClientError> {
        if let Some(started) = self.connect_started {
            if started.elapsed() > timeout {
                self.state = ClientState::Disconnected;
                self.connect_started = None;
                return Err(ClientError::Timeout);
            }
        }
        Ok(())
    }

    pub fn send_input(&mut self, yaw: f32, pitch: f32, flags: u8) -> Result<(), ClientError> {
        if self.state != ClientState::Connected { return Err(ClientError::NotConnected); }
        let pid = self.player_id.ok_or(ClientError::NotConnected)?;
        self.input_seq = self.input_seq.wrapping_add(1);
        self.local.yaw = yaw; self.local.pitch = pitch; self.local.flags = flags;
        self.send(&NetPacket::PlayerInput {
            player_id: pid, seq: self.input_seq,
            x: self.local.x, y: self.local.y, z: self.local.z,
            yaw, pitch, flags,
        })
    }

    /// Отправить ChunkAnnounce в лобби (хост ретранслирует всем).
    pub fn announce_chunk(
        &self,
        template_id: &str,
        gx: i32, gy: i32, gz: i32,
        seed: u64,
    ) -> Result<(), ClientError> {
        let pid = self.player_id.unwrap_or(0);
        self.send(&NetPacket::ChunkAnnounce {
            author_player_id: pid,
            template_id: template_id.to_string(),
            gx, gy, gz, seed,
        })
    }

    /// Отправить ChunkRequest конкретному автору через хост.
    pub fn request_chunk(
        &self,
        template_id: &str,
        gx: i32, gy: i32, gz: i32,
    ) -> Result<(), ClientError> {
        let pid = self.player_id.unwrap_or(0);
        self.send(&NetPacket::ChunkRequest {
            requester_player_id: pid,
            template_id: template_id.to_string(),
            gx, gy, gz,
        })
    }

    /// Ответить ChunkResponse (когда кто-то запросил наш чанк).
    pub fn respond_chunk(
        &self,
        template_id: &str,
        gx: i32, gy: i32, gz: i32,
        author_player_id: u32,
    ) -> Result<(), ClientError> {
        self.send(&NetPacket::ChunkResponse {
            template_id: template_id.to_string(),
            gx, gy, gz, author_player_id,
        })
    }

    pub fn disconnect(&mut self, reason: &str) -> Result<(), ClientError> {
        if self.state == ClientState::Connected {
            let _ = self.send(&NetPacket::Disconnect { reason: reason.to_string() });
        }
        self.state = ClientState::Disconnected;
        self.player_id = None;
        Ok(())
    }
}

pub fn connect(host: &str, player_name: &str) -> Result<Client, ClientError> {
    Client::connect(host, player_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::host::Host;

    #[test]
    fn resolve_host_with_port() {
        let addr = Client::resolve_host("127.0.0.1:27015").unwrap();
        assert_eq!(addr.port(), 27015);
    }

    #[test]
    fn connect_receives_http_port() {
        let mut host = Host::bind_port(0, 3, 27016).unwrap();
        let host_port = host.local_addr().unwrap().port();

        let mut client = Client::connect(&format!("127.0.0.1:{host_port}"), "t").unwrap();
        std::thread::sleep(Duration::from_millis(30));
        let _ = host.poll().unwrap();
        std::thread::sleep(Duration::from_millis(30));
        let _ = client.poll().unwrap();

        assert_eq!(client.state, ClientState::Connected);
        assert_eq!(client.host_http_port, Some(27016));
        assert_eq!(client.host_level_number, Some(3));
        assert!(client.host_base_url().unwrap().contains(":27016"));
    }

    #[test]
    fn send_input_requires_connection() {
        let mut client = Client {
            socket: UdpSocket::bind(("127.0.0.1", 0)).unwrap(),
            host_addr: "127.0.0.1:1".parse().unwrap(),
            state: ClientState::Connecting,
            player_id: None, last_snapshot: None, last_error: None,
            input_seq: 0, local: PlayerState::new(0),
            connect_started: None,
            host_http_port: None, host_level_number: None,
        };
        assert!(matches!(client.send_input(0.0, 0.0, 0), Err(ClientError::NotConnected)));
    }

    #[test]
    fn chunk_announce_requires_player_id() {
        let client = Client {
            socket: UdpSocket::bind(("127.0.0.1", 0)).unwrap(),
            host_addr: "127.0.0.1:1".parse().unwrap(),
            state: ClientState::Connected,
            player_id: Some(7), last_snapshot: None, last_error: None,
            input_seq: 0, local: PlayerState::new(7),
            connect_started: None,
            host_http_port: Some(27016), host_level_number: Some(7),
        };
        assert!(client.announce_chunk("c", 0, 0, 0, 1).is_ok());
    }
}
