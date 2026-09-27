//! Сетевой слой (raw UDP + HTTP для ассетов).

pub mod protocol;
pub mod host;
pub mod client;
pub mod sync;
pub mod http_server;

#[allow(unused_imports)]
pub use protocol::{NetPacket, PlayerState, WorldSnapshot, NetError, PROTOCOL_VERSION, DEFAULT_PORT};
#[allow(unused_imports)]
pub use http_server::{HttpServerHandle, start_http_server, base_url_for_self, lan_ip};
