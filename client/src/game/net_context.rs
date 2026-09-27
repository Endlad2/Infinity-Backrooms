//! NetContext — ресурс, описывающий, находимся ли мы в сетевой игре.
//! Содержит HTTP-URL хоста, наш player_id, реестр чанков.

use bevy::prelude::Resource;

use crate::game::chunk_registry::ChunkRegistry;

/// Роль в мультиплеере.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetRole {
    Host,
    Client,
}

/// Сетевой контекст для игрового цикла.
#[derive(Resource, Debug)]
pub struct NetContext {
    pub role: NetRole,
    /// HTTP base_url ХОСТА (например, "http://192.168.1.42:27016").
    pub host_base_url: String,
    pub level_number: u32,
    pub net_port: u16,
    pub http_port: u16,
    /// Реестр чанков — какой чанк кто сгенерировал.
    pub chunks: ChunkRegistry,
}

impl NetContext {
    pub fn host(net_port: u16, http_port: u16, level_number: u32) -> Self {
        Self {
            role: NetRole::Host,
            host_base_url: crate::net::http_server::base_url_for_self(http_port),
            level_number,
            net_port,
            http_port,
            chunks: ChunkRegistry::new(0), // хост = player_id 0
        }
    }

    pub fn client(host_base_url: String, level_number: u32) -> Self {
        Self {
            role: NetRole::Client,
            host_base_url,
            level_number,
            net_port: 27015,
            http_port: 27016,
            chunks: ChunkRegistry::new(0),
        }
    }
}
