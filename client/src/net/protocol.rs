//! Сетевой протокол raw UDP (§5.4 ТЗ).
//! Пакеты: CONNECT / DISCONNECT / PLAYER_INPUT / WORLD_STATE / EVENT / CHAT.
//! Сериализация — вручную (big-endian), без serde-зависимостей.

use std::fmt;

/// Версия протокола. Участники с другой версией отбрасываются.
pub const PROTOCOL_VERSION: u16 = 1;

/// Порт лобби по умолчанию (§5.2 ТЗ — например, 27015).
pub const DEFAULT_PORT: u16 = 27015;

/// Максимальный размер UDP-датаграммы, которую мы принимаем.
pub const MAX_PACKET_SIZE: usize = 1200;

/// Ошибки сериализации/десериализации.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    TooShort,
    UnknownTag(u8),
    BadVersion(u16),
    BadString,
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetError::TooShort => write!(f, "packet too short"),
            NetError::UnknownTag(t) => write!(f, "unknown packet tag {t}"),
            NetError::BadVersion(v) => write!(f, "unsupported protocol version {v}"),
            NetError::BadString => write!(f, "invalid utf-8 string"),
        }
    }
}

impl std::error::Error for NetError {}

/// Состояние игрока в снапшоте мира.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerState {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub hp: i32,
    pub flags: u8,
}

impl PlayerState {
    pub fn new(id: u32) -> Self {
        PlayerState {
            id,
            x: 0.0,
            y: 0.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            hp: 100,
            flags: 0,
        }
    }
}

/// Один снапшот состояния мира, отправляемый хостом клиентам.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldSnapshot {
    pub tick: u32,
    pub players: Vec<PlayerState>,
}

impl WorldSnapshot {
    pub fn new(tick: u32) -> Self {
        WorldSnapshot {
            tick,
            players: Vec::new(),
        }
    }
}

/// Сетевой пакет.
#[derive(Debug, Clone, PartialEq)]
pub enum NetPacket {
    Connect {
        version: u16,
        name: String,
    },
    ConnectAck {
        version: u16,
        player_id: u32,
    },
    Disconnect {
        reason: String,
    },
    PlayerInput {
        player_id: u32,
        seq: u32,
        x: f32,
        y: f32,
        z: f32,
        yaw: f32,
        pitch: f32,
        flags: u8,
    },
    WorldState(WorldSnapshot),
    Event {
        name: String,
        payload: String,
    },
    Chat {
        from: String,
        text: String,
    },
}

// ============================================================
// Теги пакетов
// ============================================================
const TAG_CONNECT: u8 = 1;
const TAG_CONNECT_ACK: u8 = 2;
const TAG_DISCONNECT: u8 = 3;
const TAG_PLAYER_INPUT: u8 = 4;
const TAG_WORLD_STATE: u8 = 5;
const TAG_EVENT: u8 = 6;
const TAG_CHAT: u8 = 7;

// ============================================================
// Хелперы для записи/чтения примитивов
// ============================================================
struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn new() -> Self {
        Writer { buf: Vec::with_capacity(64) }
    }
    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }
    fn str(&mut self, s: &str) -> Result<(), NetError> {
        let bytes = s.as_bytes();
        if bytes.len() > u16::MAX as usize {
            return Err(NetError::BadString);
        }
        self.u16(bytes.len() as u16);
        self.buf.extend_from_slice(bytes);
        Ok(())
    }
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }
    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
    fn u8(&mut self) -> Result<u8, NetError> {
        if self.remaining() < 1 {
            return Err(NetError::TooShort);
        }
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }
    fn u16(&mut self) -> Result<u16, NetError> {
        if self.remaining() < 2 {
            return Err(NetError::TooShort);
        }
        let v = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }
    fn u32(&mut self) -> Result<u32, NetError> {
        if self.remaining() < 4 {
            return Err(NetError::TooShort);
        }
        let v = u32::from_be_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }
    fn i32(&mut self) -> Result<i32, NetError> {
        Ok(self.u32()? as i32)
    }
    fn f32(&mut self) -> Result<f32, NetError> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn str(&mut self) -> Result<String, NetError> {
        let n = self.u16()? as usize;
        if self.remaining() < n {
            return Err(NetError::TooShort);
        }
        let s = std::str::from_utf8(&self.data[self.pos..self.pos + n])
            .map_err(|_| NetError::BadString)?
            .to_string();
        self.pos += n;
        Ok(s)
    }
}

impl NetPacket {
    /// Сериализовать пакет в байты.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            NetPacket::Connect { version, name } => {
                w.u8(TAG_CONNECT);
                w.u16(*version);
                let _ = w.str(name);
            }
            NetPacket::ConnectAck { version, player_id } => {
                w.u8(TAG_CONNECT_ACK);
                w.u16(*version);
                w.u32(*player_id);
            }
            NetPacket::Disconnect { reason } => {
                w.u8(TAG_DISCONNECT);
                let _ = w.str(reason);
            }
            NetPacket::PlayerInput {
                player_id,
                seq,
                x,
                y,
                z,
                yaw,
                pitch,
                flags,
            } => {
                w.u8(TAG_PLAYER_INPUT);
                w.u32(*player_id);
                w.u32(*seq);
                w.f32(*x);
                w.f32(*y);
                w.f32(*z);
                w.f32(*yaw);
                w.f32(*pitch);
                w.u8(*flags);
            }
            NetPacket::WorldState(snap) => {
                w.u8(TAG_WORLD_STATE);
                w.u32(snap.tick);
                w.u16(snap.players.len() as u16);
                for p in &snap.players {
                    w.u32(p.id);
                    w.f32(p.x);
                    w.f32(p.y);
                    w.f32(p.z);
                    w.f32(p.yaw);
                    w.f32(p.pitch);
                    w.i32(p.hp);
                    w.u8(p.flags);
                }
            }
            NetPacket::Event { name, payload } => {
                w.u8(TAG_EVENT);
                let _ = w.str(name);
                let _ = w.str(payload);
            }
            NetPacket::Chat { from, text } => {
                w.u8(TAG_CHAT);
                let _ = w.str(from);
                let _ = w.str(text);
            }
        }
        w.buf
    }

    /// Десериализовать пакет из байт.
    pub fn decode(data: &[u8]) -> Result<NetPacket, NetError> {
        let mut r = Reader::new(data);
        let tag = r.u8()?;
        match tag {
            TAG_CONNECT => {
                let version = r.u16()?;
                if version != PROTOCOL_VERSION {
                    return Err(NetError::BadVersion(version));
                }
                let name = r.str()?;
                Ok(NetPacket::Connect { version, name })
            }
            TAG_CONNECT_ACK => {
                let version = r.u16()?;
                let player_id = r.u32()?;
                Ok(NetPacket::ConnectAck { version, player_id })
            }
            TAG_DISCONNECT => {
                let reason = r.str()?;
                Ok(NetPacket::Disconnect { reason })
            }
            TAG_PLAYER_INPUT => {
                let player_id = r.u32()?;
                let seq = r.u32()?;
                let x = r.f32()?;
                let y = r.f32()?;
                let z = r.f32()?;
                let yaw = r.f32()?;
                let pitch = r.f32()?;
                let flags = r.u8()?;
                Ok(NetPacket::PlayerInput {
                    player_id,
                    seq,
                    x,
                    y,
                    z,
                    yaw,
                    pitch,
                    flags,
                })
            }
            TAG_WORLD_STATE => {
                let tick = r.u32()?;
                let n = r.u16()? as usize;
                let mut players = Vec::with_capacity(n);
                for _ in 0..n {
                    players.push(PlayerState {
                        id: r.u32()?,
                        x: r.f32()?,
                        y: r.f32()?,
                        z: r.f32()?,
                        yaw: r.f32()?,
                        pitch: r.f32()?,
                        hp: r.i32()?,
                        flags: r.u8()?,
                    });
                }
                Ok(NetPacket::WorldState(WorldSnapshot { tick, players }))
            }
            TAG_EVENT => {
                let name = r.str()?;
                let payload = r.str()?;
                Ok(NetPacket::Event { name, payload })
            }
            TAG_CHAT => {
                let from = r.str()?;
                let text = r.str()?;
                Ok(NetPacket::Chat { from, text })
            }
            other => Err(NetError::UnknownTag(other)),
        }
    }
}

/// Флаги ввода (для PlayerInput.flags).
pub mod flags {
    pub const SPRINT: u8 = 1 << 0;
    pub const CROUCH: u8 = 1 << 1;
    pub const JUMP: u8 = 1 << 2;
    pub const INTERACT: u8 = 1 << 3;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(p: NetPacket) {
        let bytes = p.encode();
        let back = NetPacket::decode(&bytes).expect("decode");
        assert_eq!(back, p);
    }

    #[test]
    fn connect_roundtrip() {
        roundtrip(NetPacket::Connect {
            version: PROTOCOL_VERSION,
            name: "Player One".into(),
        });
    }

    #[test]
    fn connect_ack_roundtrip() {
        roundtrip(NetPacket::ConnectAck {
            version: PROTOCOL_VERSION,
            player_id: 42,
        });
    }

    #[test]
    fn disconnect_roundtrip() {
        roundtrip(NetPacket::Disconnect {
            reason: "bye".into(),
        });
    }

    #[test]
    fn player_input_roundtrip() {
        roundtrip(NetPacket::PlayerInput {
            player_id: 3,
            seq: 9999,
            x: 1.5,
            y: -2.25,
            z: 3.125,
            yaw: 90.0,
            pitch: -10.0,
            flags: flags::SPRINT | flags::JUMP,
        });
    }

    #[test]
    fn world_state_roundtrip() {
        let mut snap = WorldSnapshot::new(100);
        let mut p1 = PlayerState::new(1);
        p1.x = 5.0;
        p1.hp = 87;
        let mut p2 = PlayerState::new(2);
        p2.z = -3.5;
        snap.players.push(p1);
        snap.players.push(p2);
        roundtrip(NetPacket::WorldState(snap));
    }

    #[test]
    fn event_roundtrip() {
        roundtrip(NetPacket::Event {
            name: "door_opened".into(),
            payload: "{\"id\":\"door_01\"}".into(),
        });
    }

    #[test]
    fn chat_roundtrip() {
        roundtrip(NetPacket::Chat {
            from: "host".into(),
            text: "привет".into(),
        });
    }

    #[test]
    fn bad_version_rejected() {
        let bytes = NetPacket::Connect {
            version: 999,
            name: "x".into(),
        }
        .encode();
        assert!(matches!(
            NetPacket::decode(&bytes),
            Err(NetError::BadVersion(999))
        ));
    }

    #[test]
    fn unknown_tag_rejected() {
        assert!(matches!(
            NetPacket::decode(&[200u8]),
            Err(NetError::UnknownTag(200))
        ));
    }

    #[test]
    fn short_packet_rejected() {
        assert_eq!(NetPacket::decode(&[]), Err(NetError::TooShort));
    }
}