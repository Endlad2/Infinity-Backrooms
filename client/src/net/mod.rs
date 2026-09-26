//! Сетевой слой (raw UDP, только LAN, §5 ТЗ).
//! Хост — авторитетная сторона, клиент шлёт ввод и принимает WORLD_STATE/EVENT.

pub mod protocol;
pub mod host;
pub mod client;
pub mod sync;

pub use protocol::{NetPacket, PlayerState, WorldSnapshot, NetError, PROTOCOL_VERSION, DEFAULT_PORT};