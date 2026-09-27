//! Реестр чанков в мультиплеере: кто какой чанк сгенерировал, что уже
//! скачано, что нужно запросить у других участников.
//!
//! Логика:
//!   * Каждый чанк идентифицируется парой (template_id, gx, gy, gz).
//!   * Если игрок первым заходит в область и там нет чанка — он его генерирует
//!     и рассылает ChunkAnnounce.
//!   * Остальные получают ChunkAnnounce и записывают в registry.
//!   * Когда нам нужно прогрузить чанк, мы сначала смотрим registry:
//!       - есть → берём данные от автора (HTTP);
//!       - нет  → генерируем сами и рассылаем ChunkAnnounce.

use std::collections::HashMap;

/// Ключ чанка: (template_id, gx, gy, gz).
pub type ChunkKey = (String, i32, i32, i32);

/// Запись о чанке в реестре.
#[derive(Debug, Clone)]
pub struct ChunkEntry {
    pub key: ChunkKey,
    /// Кто сгенерировал: 0 = хост, u32 = player_id.
    pub author_player_id: u32,
    /// Seed, использованный при генерации.
    pub seed: u64,
    /// Мы уже скачали этот чанк локально?
    pub downloaded: bool,
}

/// Реестр чанков. Одна инстанция на игру.
#[derive(Debug, Default)]
pub struct ChunkRegistry {
    entries: HashMap<ChunkKey, ChunkEntry>,
    /// Наш player_id, чтобы не запрашивать чанк у себя же.
    pub my_player_id: u32,
}

impl ChunkRegistry {
    pub fn new(my_player_id: u32) -> Self {
        Self { entries: HashMap::new(), my_player_id }
    }

    pub fn contains(&self, key: &ChunkKey) -> bool {
        self.entries.contains_key(key)
    }

    pub fn get(&self, key: &ChunkKey) -> Option<&ChunkEntry> {
        self.entries.get(key)
    }

    /// Зарегистрировать чанк (после ChunkAnnounce или собственной генерации).
    pub fn register(&mut self, key: ChunkKey, author_player_id: u32, seed: u64) {
        self.entries.entry(key.clone()).or_insert(ChunkEntry {
            key,
            author_player_id,
            seed,
            downloaded: false,
        });
    }

    /// Пометить, что чанк скачан/сгенерирован локально.
    pub fn mark_downloaded(&mut self, key: &ChunkKey) {
        if let Some(e) = self.entries.get_mut(key) {
            e.downloaded = true;
        }
    }

    /// Список чанков, которые нам нужны, но ещё не скачаны.
    pub fn pending(&self) -> Vec<&ChunkEntry> {
        self.entries.values().filter(|e| !e.downloaded).collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_query() {
        let mut r = ChunkRegistry::new(1);
        let key: ChunkKey = ("chunk_flood_grid".into(), 5, 0, -3);
        assert!(!r.contains(&key));
        r.register(key.clone(), 0, 42);
        assert!(r.contains(&key));
        let e = r.get(&key).unwrap();
        assert_eq!(e.author_player_id, 0);
        assert_eq!(e.seed, 42);
        assert!(!e.downloaded);
    }

    #[test]
    fn mark_downloaded() {
        let mut r = ChunkRegistry::new(1);
        let key: ChunkKey = ("c".into(), 0, 0, 0);
        r.register(key.clone(), 2, 0);
        r.mark_downloaded(&key);
        assert!(r.get(&key).unwrap().downloaded);
        assert!(r.pending().is_empty());
    }
}
