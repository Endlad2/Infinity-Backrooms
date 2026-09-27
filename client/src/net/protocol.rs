//! Сетевой протокол raw UDP (§5.4 ТЗ) + multicast-подобная рассылка
//! событий о чанках между участниками.
//!
//! Пакеты: CONNECT / DISCONNECT / PLAYER_INPUT / WORLD_STATE / EVENT / CHAT,
//! а также CHUNK_ANNOUNCE / CHUNK_REQUEST для обмена сгенерированными чанками.
//!
//! Сериализация — вручную (big-endian), без serde.

use std::fmt;

pub const PROTOCOL_VERSION: u16 = 1;
pub const DEFAULT_PORT: u16 = 27015;
pub const MAX_PACKET_SIZE: usize = 1200;

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
        PlayerState { id, x: 0.0, y: 0.0, z: 0.0, yaw: 0.0, pitch: 0.0, hp: 100, flags: 0 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorldSnapshot {
    pub tick: u32,
    pub players: Vec<PlayerState>,
}

impl WorldSnapshot {
    pub fn new(tick: u32) -> Self {
        WorldSnapshot { tick, players: Vec::new() }
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
        /// HTTP-порт хоста — клиент потом качает ассеты с него.
        http_port: u16,
        /// Номер уровня лобби.
        level_number: u32,
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
    /// Объявление о том, что некий участник сгенерировал чанк по координатам (gx,gy,gz)
    /// и теперь все остальные должны не генерировать, а скачать его с HTTP-сервера
    /// автора (или с хоста, если автор — хост).
    ///
    /// Поля:
    ///   author_player_id — id участника, сгенерировавшего чанк (0 = хост).
    ///   template_id      — id шаблона чанка из XML (например "chunk_flood_grid").
    ///   gx, gy, gz       — координаты чанка в grid.
    ///   seed             — seed для детерминированной генерации, если клиент
    ///                      всё же решит сгенерировать сам.
    ChunkAnnounce {
        author_player_id: u32,
        template_id: String,
        gx: i32,
        gy: i32,
        gz: i32,
        seed: u64,
    },
    /// Запрос чанка у конкретного клиента (когда мы подошли близко, а чанка нет).
    ChunkRequest {
        requester_player_id: u32,
        template_id: String,
        gx: i32,
        gy: i32,
        gz: i32,
    },
    /// Ответ на ChunkRequest — сам XML-фрагмент чанка (сериализованный
    /// либо путь к файлу на HTTP-сервере автора).
    ChunkResponse {
        template_id: String,
        gx: i32,
        gy: i32,
        gz: i32,
        /// Автор, у которого лежит этот чанк (для HTTP-скачивания).
        author_player_id: u32,
    },
}

// ---------------------------------------------------------------------------
// Теги
// ---------------------------------------------------------------------------
const TAG_CONNECT: u8 = 1;
const TAG_CONNECT_ACK: u8 = 2;
const TAG_DISCONNECT: u8 = 3;
const TAG_PLAYER_INPUT: u8 = 4;
const TAG_WORLD_STATE: u8 = 5;
const TAG_EVENT: u8 = 6;
const TAG_CHAT: u8 = 7;
const TAG_CHUNK_ANNOUNCE: u8 = 8;
const TAG_CHUNK_REQUEST: u8 = 9;
const TAG_CHUNK_RESPONSE: u8 = 10;

struct Writer {
    buf: Vec<u8>,
}
impl Writer {
    fn new() -> Self { Writer { buf: Vec::with_capacity(64) } }
    fn u8(&mut self, v: u8) { self.buf.push(v); }
    fn u16(&mut self, v: u16) { self.buf.extend_from_slice(&v.to_be_bytes()); }
    fn u32(&mut self, v: u32) { self.buf.extend_from_slice(&v.to_be_bytes()); }
    fn u64(&mut self, v: u64) { self.buf.extend_from_slice(&v.to_be_bytes()); }
    fn i32(&mut self, v: i32) { self.buf.extend_from_slice(&v.to_be_bytes()); }
    fn f32(&mut self, v: f32) { self.buf.extend_from_slice(&v.to_be_bytes()); }
    fn str(&mut self, s: &str) -> Result<(), NetError> {
        let bytes = s.as_bytes();
        if bytes.len() > u16::MAX as usize { return Err(NetError::BadString); }
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
    fn new(d: &'a [u8]) -> Self { Reader { data: d, pos: 0 } }
    fn remaining(&self) -> usize { self.data.len().saturating_sub(self.pos) }
    fn u8(&mut self) -> Result<u8, NetError> {
        if self.remaining() < 1 { return Err(NetError::TooShort); }
        let v = self.data[self.pos]; self.pos += 1; Ok(v)
    }
    fn u16(&mut self) -> Result<u16, NetError> {
        if self.remaining() < 2 { return Err(NetError::TooShort); }
        let v = u16::from_be_bytes([self.data[self.pos], self.data[self.pos+1]]);
        self.pos += 2; Ok(v)
    }
    fn u32(&mut self) -> Result<u32, NetError> {
        if self.remaining() < 4 { return Err(NetError::TooShort); }
        let v = u32::from_be_bytes([
            self.data[self.pos], self.data[self.pos+1],
            self.data[self.pos+2], self.data[self.pos+3],
        ]);
        self.pos += 4; Ok(v)
    }
    fn u64(&mut self) -> Result<u64, NetError> {
        if self.remaining() < 8 { return Err(NetError::TooShort); }
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.data[self.pos..self.pos+8]);
        self.pos += 8;
        Ok(u64::from_be_bytes(b))
    }
    fn i32(&mut self) -> Result<i32, NetError> { Ok(self.u32()? as i32) }
    fn f32(&mut self) -> Result<f32, NetError> { Ok(f32::from_bits(self.u32()?)) }
    fn str(&mut self) -> Result<String, NetError> {
        let n = self.u16()? as usize;
        if self.remaining() < n { return Err(NetError::TooShort); }
        let s = std::str::from_utf8(&self.data[self.pos..self.pos+n])
            .map_err(|_| NetError::BadString)?.to_string();
        self.pos += n;
        Ok(s)
    }
}

impl NetPacket {
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        match self {
            NetPacket::Connect { version, name } => {
                w.u8(TAG_CONNECT);
                w.u16(*version);
                let _ = w.str(name);
            }
            NetPacket::ConnectAck { version, player_id, http_port, level_number } => {
                w.u8(TAG_CONNECT_ACK);
                w.u16(*version);
                w.u32(*player_id);
                w.u16(*http_port);
                w.u32(*level_number);
            }
            NetPacket::Disconnect { reason } => {
                w.u8(TAG_DISCONNECT);
                let _ = w.str(reason);
            }
            NetPacket::PlayerInput {
                player_id, seq, x, y, z, yaw, pitch, flags,
            } => {
                w.u8(TAG_PLAYER_INPUT);
                w.u32(*player_id);
                w.u32(*seq);
                w.f32(*x); w.f32(*y); w.f32(*z);
                w.f32(*yaw); w.f32(*pitch);
                w.u8(*flags);
            }
            NetPacket::WorldState(snap) => {
                w.u8(TAG_WORLD_STATE);
                w.u32(snap.tick);
                w.u16(snap.players.len() as u16);
                for p in &snap.players {
                    w.u32(p.id);
                    w.f32(p.x); w.f32(p.y); w.f32(p.z);
                    w.f32(p.yaw); w.f32(p.pitch);
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
            NetPacket::ChunkAnnounce {
                author_player_id, template_id, gx, gy, gz, seed,
            } => {
                w.u8(TAG_CHUNK_ANNOUNCE);
                w.u32(*author_player_id);
                let _ = w.str(template_id);
                w.i32(*gx); w.i32(*gy); w.i32(*gz);
                w.u64(*seed);
            }
            NetPacket::ChunkRequest {
                requester_player_id, template_id, gx, gy, gz,
            } => {
                w.u8(TAG_CHUNK_REQUEST);
                w.u32(*requester_player_id);
                let _ = w.str(template_id);
                w.i32(*gx); w.i32(*gy); w.i32(*gz);
            }
            NetPacket::ChunkResponse {
                template_id, gx, gy, gz, author_player_id,
            } => {
                w.u8(TAG_CHUNK_RESPONSE);
                let _ = w.str(template_id);
                w.i32(*gx); w.i32(*gy); w.i32(*gz);
                w.u32(*author_player_id);
            }
        }
        w.buf
    }

    pub fn decode(data: &[u8]) -> Result<NetPacket, NetError> {
        let mut r = Reader::new(data);
        let tag = r.u8()?;
        match tag {
            TAG_CONNECT => {
                let version = r.u16()?;
                if version != PROTOCOL_VERSION { return Err(NetError::BadVersion(version)); }
                let name = r.str()?;
                Ok(NetPacket::Connect { version, name })
            }
            TAG_CONNECT_ACK => {
                let version = r.u16()?;
                let player_id = r.u32()?;
                let http_port = r.u16()?;
                let level_number = r.u32()?;
                Ok(NetPacket::ConnectAck { version, player_id, http_port, level_number })
            }
            TAG_DISCONNECT => Ok(NetPacket::Disconnect { reason: r.str()? }),
            TAG_PLAYER_INPUT => {
                let player_id = r.u32()?;
                let seq = r.u32()?;
                let x = r.f32()?; let y = r.f32()?; let z = r.f32()?;
                let yaw = r.f32()?; let pitch = r.f32()?;
                let flags = r.u8()?;
                Ok(NetPacket::PlayerInput { player_id, seq, x, y, z, yaw, pitch, flags })
            }
            TAG_WORLD_STATE => {
                let tick = r.u32()?;
                let n = r.u16()? as usize;
                let mut players = Vec::with_capacity(n);
                for _ in 0..n {
                    players.push(PlayerState {
                        id: r.u32()?, x: r.f32()?, y: r.f32()?, z: r.f32()?,
                        yaw: r.f32()?, pitch: r.f32()?, hp: r.i32()?, flags: r.u8()?,
                    });
                }
                Ok(NetPacket::WorldState(WorldSnapshot { tick, players }))
            }
            TAG_EVENT => Ok(NetPacket::Event { name: r.str()?, payload: r.str()? }),
            TAG_CHAT => Ok(NetPacket::Chat { from: r.str()?, text: r.str()? }),
            TAG_CHUNK_ANNOUNCE => {
                let author_player_id = r.u32()?;
                let template_id = r.str()?;
                let gx = r.i32()?; let gy = r.i32()?; let gz = r.i32()?;
                let seed = r.u64()?;
                Ok(NetPacket::ChunkAnnounce { author_player_id, template_id, gx, gy, gz, seed })
            }
            TAG_CHUNK_REQUEST => {
                let requester_player_id = r.u32()?;
                let template_id = r.str()?;
                let gx = r.i32()?; let gy = r.i32()?; let gz = r.i32()?;
                Ok(NetPacket::ChunkRequest { requester_player_id, template_id, gx, gy, gz })
            }
            TAG_CHUNK_RESPONSE => {
                let template_id = r.str()?;
                let gx = r.i32()?; let gy = r.i32()?; let gz = r.i32()?;
                let author_player_id = r.u32()?;
                Ok(NetPacket::ChunkResponse { template_id, gx, gy, gz, author_player_id })
            }
            other => Err(NetError::UnknownTag(other)),
        }
    }
}

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
        roundtrip(NetPacket::Connect { version: PROTOCOL_VERSION, name: "Player One".into() });
    }

    #[test]
    fn connect_ack_roundtrip() {
        roundtrip(NetPacket::ConnectAck { version: PROTOCOL_VERSION, player_id: 42, http_port: 27016, level_number: 7 });
    }

    #[test]
    fn player_input_roundtrip() {
        roundtrip(NetPacket::PlayerInput {
            player_id: 3, seq: 9999,
            x: 1.5, y: -2.25, z: 3.125,
            yaw: 90.0, pitch: -10.0,
            flags: flags::SPRINT | flags::JUMP,
        });
    }

    #[test]
    fn chunk_announce_roundtrip() {
        roundtrip(NetPacket::ChunkAnnounce {
            author_player_id: 2,
            template_id: "chunk_flood_grid".into(),
            gx: 5, gy: 0, gz: -3,
            seed: 1234567,
        });
    }

    #[test]
    fn chunk_request_roundtrip() {
        roundtrip(NetPacket::ChunkRequest {
            requester_player_id: 1,
            template_id: "chunk_pool".into(),
            gx: -2, gy: 1, gz: 4,
        });
    }

    #[test]
    fn chunk_response_roundtrip() {
        roundtrip(NetPacket::ChunkResponse {
            template_id: "chunk_pipe_tunnel".into(),
            gx: 0, gy: 0, gz: 0,
            author_player_id: 1,
        });
    }
}
