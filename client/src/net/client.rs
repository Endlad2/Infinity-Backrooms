//! UDP-клиент лобби (§5.3 ТЗ): шлёт CONNECT на IP хоста, принимает ConnectAck,
//! шлёт PLAYER_INPUT, принимает WORLD_STATE/EVENT/CHAT. Подключение — только LAN.

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use super::protocol::{NetPacket, PlayerState, DEFAULT_PORT, MAX_PACKET_SIZE, PROTOCOL_VERSION};

/// Ошибки клиента.
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

impl From<std::io::Error> for ClientError {
    fn from(e: std::io::Error) -> Self {
        ClientError::Io(e)
    }
}

/// Состояние подключения клиента.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientState {
    Disconnected,
    Connecting,
    Connected,
}

/// Клиент лобби.
pub struct Client {
    socket: UdpSocket,
    host_addr: SocketAddr,
    pub state: ClientState,
    pub player_id: Option<u32>,
    pub last_snapshot: Option<super::protocol::WorldSnapshot>,
    pub last_error: Option<String>,
    /// Монотонный счётчик исходящих PLAYER_INPUT.
    pub input_seq: u32,
    /// Локальное состояние игрока (что мы шлём).
    pub local: PlayerState,
    connect_started: Option<Instant>,
}

impl Client {
    /// Разрешает адрес хоста. Принимает либо "IP", либо "IP:port".
    pub fn resolve_host(host: &str) -> Result<SocketAddr, ClientError> {
        let with_port = if host.contains(':') {
            host.to_string()
        } else {
            format!("{host}:{DEFAULT_PORT}")
        };
        let mut addrs = with_port
            .to_socket_addrs()
            .map_err(|e| ClientError::Dns(e.to_string()))?;
        addrs.next().ok_or_else(|| ClientError::Dns("no address resolved".into()))
    }

    /// Создаёт сокет и отправляет CONNECT. Возвращает клиента в состоянии Connecting.
    pub fn connect(host: &str, player_name: &str) -> Result<Self, ClientError> {
        let host_addr = Self::resolve_host(host)?;
        let socket = UdpSocket::bind(("0.0.0.0", 0))?;
        socket.set_nonblocking(true)?;

        let mut client = Client {
            socket,
            host_addr,
            state: ClientState::Connecting,
            player_id: None,
            last_snapshot: None,
            last_error: None,
            input_seq: 0,
            local: PlayerState::new(0),
            connect_started: Some(Instant::now()),
        };
        client.send(&NetPacket::Connect {
            version: PROTOCOL_VERSION,
            name: player_name.to_string(),
        })?;
        Ok(client)
    }

    /// Адрес хоста.
    pub fn host_addr(&self) -> SocketAddr {
        self.host_addr
    }

    /// Отправляет пакет хосту.
    pub fn send(&self, pkt: &NetPacket) -> Result<(), ClientError> {
        let bytes = pkt.encode();
        self.socket.send_to(&bytes, self.host_addr)?;
        Ok(())
    }

    /// Обрабатывает все доступные датаграммы. Возвращает список принятых пакетов.
    pub fn poll(&mut self) -> Result<Vec<NetPacket>, ClientError> {
        let mut received = Vec::new();
        let mut buf = [0u8; MAX_PACKET_SIZE];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((n, from)) => {
                    // Игнорируем пакеты не от хоста.
                    if from != self.host_addr {
                        continue;
                    }
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
            NetPacket::ConnectAck { player_id, .. } => {
                self.player_id = Some(*player_id);
                self.local.id = *player_id;
                self.state = ClientState::Connected;
                self.connect_started = None;
            }
            NetPacket::Disconnect { reason } => {
                self.last_error = Some(reason.clone());
                self.state = ClientState::Disconnected;
                self.player_id = None;
            }
            NetPacket::WorldState(snap) => {
                self.last_snapshot = Some(snap.clone());
            }
            _ => {}
        }
    }

    /// Проверяет, что подключение не затянулось. Возвращает Err(Timeout) при превышении.
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

    /// Отправляет PLAYER_INPUT с текущим локальным состоянием.
    pub fn send_input(&mut self, yaw: f32, pitch: f32, flags: u8) -> Result<(), ClientError> {
        if self.state != ClientState::Connected {
            return Err(ClientError::NotConnected);
        }
        let pid = self.player_id.ok_or(ClientError::NotConnected)?;
        self.input_seq = self.input_seq.wrapping_add(1);
        self.local.yaw = yaw;
        self.local.pitch = pitch;
        self.local.flags = flags;
        self.send(&NetPacket::PlayerInput {
            player_id: pid,
            seq: self.input_seq,
            x: self.local.x,
            y: self.local.y,
            z: self.local.z,
            yaw,
            pitch,
            flags,
        })
    }

    /// Отправляет DISCONNECT и помечает состояние как Disconnected.
    pub fn disconnect(&mut self, reason: &str) -> Result<(), ClientError> {
        if self.state == ClientState::Connected {
            let _ = self.send(&NetPacket::Disconnect {
                reason: reason.to_string(),
            });
        }
        self.state = ClientState::Disconnected;
        self.player_id = None;
        Ok(())
    }

    /// Отправляет CHAT хосту.
    pub fn send_chat(&self, from: &str, text: &str) -> Result<(), ClientError> {
        self.send(&NetPacket::Chat {
            from: from.to_string(),
            text: text.to_string(),
        })
    }
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
    fn resolve_host_without_port_uses_default() {
        let addr = Client::resolve_host("127.0.0.1").unwrap();
        assert_eq!(addr.port(), DEFAULT_PORT);
    }

    #[test]
    fn connect_and_receive_ack() {
        // Хост на случайном порту.
        let mut host = Host::bind_port(0, 3).unwrap();
        let host_port = host.local_addr().unwrap().port();

        let mut client = Client::connect(&format!("127.0.0.1:{host_port}"), "tester").unwrap();
        assert_eq!(client.state, ClientState::Connecting);

        // Хост принимает CONNECT и отвечает ConnectAck.
        std::thread::sleep(Duration::from_millis(30));
        let _ = host.poll().unwrap();
        assert_eq!(host.peer_count(), 1);

        // Клиент читает ConnectAck.
        std::thread::sleep(Duration::from_millis(30));
        let _ = client.poll().unwrap();
        assert_eq!(client.state, ClientState::Connected);
        assert!(client.player_id.is_some());
    }

    #[test]
    fn send_input_requires_connection() {
        let mut client = Client {
            socket: UdpSocket::bind(("127.0.0.1", 0)).unwrap(),
            host_addr: "127.0.0.1:1".parse().unwrap(),
            state: ClientState::Connecting,
            player_id: None,
            last_snapshot: None,
            last_error: None,
            input_seq: 0,
            local: PlayerState::new(0),
            connect_started: None,
        };
        assert!(matches!(
            client.send_input(0.0, 0.0, 0),
            Err(ClientError::NotConnected)
        ));
    }

    #[test]
    fn check_timeout_fires() {
        let mut client = Client {
            socket: UdpSocket::bind(("127.0.0.1", 0)).unwrap(),
            host_addr: "127.0.0.1:1".parse().unwrap(),
            state: ClientState::Connecting,
            player_id: None,
            last_snapshot: None,
            last_error: None,
            input_seq: 0,
            local: PlayerState::new(0),
            connect_started: Some(Instant::now() - Duration::from_secs(10)),
        };
        assert!(matches!(
            client.check_timeout(Duration::from_secs(5)),
            Err(ClientError::Timeout)
        ));
        assert_eq!(client.state, ClientState::Disconnected);
    }

    #[test]
    fn disconnect_clears_state() {
        let mut client = Client {
            socket: UdpSocket::bind(("127.0.0.1", 0)).unwrap(),
            host_addr: "127.0.0.1:1".parse().unwrap(),
            state: ClientState::Connected,
            player_id: Some(7),
            last_snapshot: None,
            last_error: None,
            input_seq: 0,
            local: PlayerState::new(7),
            connect_started: None,
        };
        client.disconnect("bye").unwrap();
        assert_eq!(client.state, ClientState::Disconnected);
        assert!(client.player_id.is_none());
    }
}


/// Удобная free-функция: подключиться к хосту (обёртка над Client::connect).
pub fn connect(host: &str, player_name: &str) -> Result<Client, ClientError> {
    Client::connect(host, player_name)
}
