//! UDP-хост лобби (§5.2 ТЗ): открывает сокет на DEFAULT_PORT, принимает CONNECT,
//! рассылает WORLD_STATE/EVENT всем участникам. Хост — авторитетная сторона.

use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};

use super::protocol::{NetPacket, PlayerState, WorldSnapshot, DEFAULT_PORT, MAX_PACKET_SIZE, PROTOCOL_VERSION};

/// Ошибки хоста.
#[derive(Debug)]
pub enum HostError {
    Io(std::io::Error),
}

impl From<std::io::Error> for HostError {
    fn from(e: std::io::Error) -> Self {
        HostError::Io(e)
    }
}

impl std::error::Error for HostError {}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { HostError::Io(e) => write!(f, "host io error: {}", e) }
    }
}

/// Запись об участнике.
#[derive(Debug, Clone)]
pub struct Peer {
    pub addr: SocketAddr,
    pub player_id: u32,
    pub name: String,
    pub last_seen_tick: u32,
}

/// Хост-лобби.
pub struct Host {
    socket: UdpSocket,
    peers: HashMap<SocketAddr, Peer>,
    next_player_id: u32,
    pub tick: u32,
    pub level_number: u32,
}

impl Host {
    /// Создаёт хост на DEFAULT_PORT (0.0.0.0) и привязывается.
    pub fn bind(level_number: u32) -> Result<Self, HostError> {
        Self::bind_port(DEFAULT_PORT, level_number)
    }

    /// Создаёт хост на указанном порту.
    pub fn bind_port(port: u16, level_number: u32) -> Result<Self, HostError> {
        let socket = UdpSocket::bind(("0.0.0.0", port))?;
        socket.set_nonblocking(true)?;
        Ok(Host {
            socket,
            peers: HashMap::new(),
            next_player_id: 1,
            tick: 0,
            level_number,
        })
    }

    /// Локальный адрес сокета (порт может отличаться от запрошенного).
    pub fn local_addr(&self) -> Result<SocketAddr, HostError> {
        let a = self.socket.local_addr()?;
        if a.ip().is_unspecified() {
            Ok(SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), a.port()))
        } else {
            Ok(a)
        }
    }

    /// Количество подключённых участников.
    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    /// Список подключённых участников (для отображения).
    pub fn peers(&self) -> Vec<Peer> {
        self.peers.values().cloned().collect()
    }

    /// Обрабатывает все доступные датаграммы. Возвращает принятые пакеты вместе с адресами.
    pub fn poll(&mut self) -> Result<Vec<(SocketAddr, NetPacket)>, HostError> {
        let mut received = Vec::new();
        let mut buf = [0u8; MAX_PACKET_SIZE];
        loop {
            match self.socket.recv_from(&mut buf) {
                Ok((n, addr)) => {
                    if let Ok(pkt) = NetPacket::decode(&buf[..n]) {
                        self.handle_incoming(addr, &pkt);
                        received.push((addr, pkt));
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(HostError::Io(e)),
            }
        }
        Ok(received)
    }

    /// Внутренняя обработка: CONNECT → добавление пира + ответ ConnectAck.
    fn handle_incoming(&mut self, addr: SocketAddr, pkt: &NetPacket) {
        match pkt {
            NetPacket::Connect { version, name } => {
                if *version != PROTOCOL_VERSION {
                    let _ = self.send_to(addr, &NetPacket::Disconnect {
                        reason: "bad protocol version".into(),
                    });
                    return;
                }
                let peer = self.peers.entry(addr).or_insert_with(|| {
                    let id = self.next_player_id;
                    self.next_player_id += 1;
                    Peer {
                        addr,
                        player_id: id,
                        name: name.clone(),
                        last_seen_tick: self.tick,
                    }
                });
                peer.last_seen_tick = self.tick;
                let pid = peer.player_id;
                let _ = self.send_to(addr, &NetPacket::ConnectAck {
                    version: PROTOCOL_VERSION,
                    player_id: pid,
                });
            }
            NetPacket::Disconnect { .. } => {
                self.peers.remove(&addr);
            }
            NetPacket::PlayerInput { .. } => {
                if let Some(p) = self.peers.get_mut(&addr) {
                    p.last_seen_tick = self.tick;
                }
            }
            _ => {
                if let Some(p) = self.peers.get_mut(&addr) {
                    p.last_seen_tick = self.tick;
                }
            }
        }
    }

    /// Отправляет пакет конкретному адресу.
    pub fn send_to(&self, addr: SocketAddr, pkt: &NetPacket) -> Result<(), HostError> {
        let bytes = pkt.encode();
        self.socket.send_to(&bytes, addr)?;
        Ok(())
    }

    /// Рассылает пакет всем подключённым участникам.
    pub fn broadcast(&self, pkt: &NetPacket) -> Result<usize, HostError> {
        let bytes = pkt.encode();
        let mut sent = 0;
        for peer in self.peers.values() {
            if self.socket.send_to(&bytes, peer.addr).is_ok() {
                sent += 1;
            }
        }
        Ok(sent)
    }

    /// Формирует снапшот мира и рассылает его.
    pub fn broadcast_world_state(&self, players: &[PlayerState]) -> Result<usize, HostError> {
        let snap = WorldSnapshot {
            tick: self.tick,
            players: players.to_vec(),
        };
        self.broadcast(&NetPacket::WorldState(snap))
    }

    /// Рассылает EVENT всем участникам.
    pub fn broadcast_event(&self, name: &str, payload: &str) -> Result<usize, HostError> {
        self.broadcast(&NetPacket::Event {
            name: name.to_string(),
            payload: payload.to_string(),
        })
    }

    /// Инкрементирует тик.
    pub fn advance_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
    }

    /// Удаляет пиров, неактивных более `timeout_ticks` тиков.
    pub fn prune_inactive(&mut self, timeout_ticks: u32) -> usize {
        let tick = self.tick;
        let before = self.peers.len();
        self.peers
            .retain(|_, p| tick.wrapping_sub(p.last_seen_tick) < timeout_ticks);
        before - self.peers.len()
    }
}

/// Определяет LAN-IP хоста (для отображения §5.2 ТЗ — «показать свой LAN-IP»).
/// Использует трюк с connect() к внешнему адресу, чтобы узнать исходящий интерфейс.
pub fn detect_lan_ip() -> Option<std::net::IpAddr> {
    let socket = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    socket.connect(("8.8.8.8", 80)).ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}

/// Форматированный LAN-IP для отображения крупно (например, "192.168.1.42").
pub fn display_lan_ip() -> String {
    match detect_lan_ip() {
        Some(ip) => ip.to_string(),
        None => "0.0.0.0".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::UdpSocket;

    #[test]
    fn host_binds_and_reports_addr() {
        let host = Host::bind_port(0, 5).expect("bind");
        let addr = host.local_addr().expect("addr");
        assert_ne!(addr.port(), 0);
        assert_eq!(host.level_number, 5);
        assert_eq!(host.peer_count(), 0);
    }

    #[test]
    fn connect_adds_peer_and_responds() {
        let mut host = Host::bind_port(0, 0).expect("bind");
        let host_addr = host.local_addr().unwrap();

        let client = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        client.set_nonblocking(true).unwrap();

        let connect = NetPacket::Connect {
            version: PROTOCOL_VERSION,
            name: "tester".into(),
        }
        .encode();
        client.send_to(&connect, host_addr).unwrap();

        // даём хосту принять
        std::thread::sleep(std::time::Duration::from_millis(50));
        let received = host.poll().unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(host.peer_count(), 1);

        // читаем ConnectAck на клиенте
        std::thread::sleep(std::time::Duration::from_millis(50));
        let mut buf = [0u8; MAX_PACKET_SIZE];
        let (n, _) = client.recv_from(&mut buf).expect("ack");
        let ack = NetPacket::decode(&buf[..n]).expect("decode ack");
        match ack {
            NetPacket::ConnectAck { player_id, .. } => assert_eq!(player_id, 1),
            other => panic!("expected ConnectAck, got {:?}", other),
        }
    }

    #[test]
    fn prune_removes_stale_peers() {
        let mut host = Host::bind_port(0, 0).unwrap();
        let addr: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        host.peers.insert(
            addr,
            Peer {
                addr,
                player_id: 1,
                name: "x".into(),
                last_seen_tick: 0,
            },
        );
        host.tick = 1000;
        let removed = host.prune_inactive(60);
        assert_eq!(removed, 1);
        assert_eq!(host.peer_count(), 0);
    }

    #[test]
    fn broadcast_without_peers_is_ok() {
        let host = Host::bind_port(0, 0).unwrap();
        let sent = host.broadcast_event("test", "{}").unwrap();
        assert_eq!(sent, 0);
    }

    #[test]
    fn detect_lan_ip_returns_something_or_none() {
        // В офлайн-среде может вернуть None — это допустимо.
        let _ = detect_lan_ip();
        let _ = display_lan_ip();
    }
}


/// Удобная обёртка для main.rs: создать лобби на указанном порту.
pub fn start_lobby(port: u16) -> Result<Host, HostError> {
    Host::bind_port(port, 0)
}
