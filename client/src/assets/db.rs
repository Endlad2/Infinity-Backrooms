//! SQLite-база ассетов (textures, models).
//! Схема:
//!   textures(id INTEGER PRIMARY KEY, tags TEXT, asset BLOB)
//!   models  (id INTEGER PRIMARY KEY, tags TEXT, asset BLOB)
//!
//! Поиск делается по полному совпадению множества тегов:
//! нормализуем теги (trim + lowercase), сортируем, склеиваем через запятую
//! и сохраняем в колонку tags в том же виде — тогда поиск это просто
//! `WHERE tags = ?` по нормализованной строке.

use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct AssetDb {
    conn: Connection,
}

impl AssetDb {
    /// Открыть/создать БД по пути, создать таблицы.
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }

    /// Открыть in-memory БД (для тестов).
    pub fn open_in_memory() -> rusqlite::Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init_schema(&conn)?;
        Ok(Self { conn })
    }

    fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS textures (
                 id INTEGER PRIMARY KEY,
                 tags TEXT NOT NULL,
                 asset BLOB NOT NULL
             );
             CREATE TABLE IF NOT EXISTS models (
                 id INTEGER PRIMARY KEY,
                 tags TEXT NOT NULL,
                 asset BLOB NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_textures_tags ON textures(tags);
             CREATE INDEX IF NOT EXISTS idx_models_tags   ON models(tags);",
        )
    }

    /// Найти PNG-текстуру по полному набору тегов.
    pub fn find_texture(&self, tags: &[&str]) -> rusqlite::Result<Option<Vec<u8>>> {
        let key = normalize_tags(tags);
        self.conn
            .query_row(
                "SELECT asset FROM textures WHERE tags = ?1 LIMIT 1",
                params![key],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
    }

    /// Найти модель (OBJ/GLB/FBX) по полному набору тегов.
    pub fn find_model(&self, tags: &[&str]) -> rusqlite::Result<Option<Vec<u8>>> {
        let key = normalize_tags(tags);
        self.conn
            .query_row(
                "SELECT asset FROM models WHERE tags = ?1 LIMIT 1",
                params![key],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()
    }

    /// Вставить/заменить текстуру. Возвращает id.
    pub fn insert_texture(&self, tags: &[&str], png: &[u8]) -> rusqlite::Result<i64> {
        let key = normalize_tags(tags);
        self.conn.execute(
            "INSERT INTO textures(tags, asset) VALUES(?1, ?2)
             ON CONFLICT(id) DO UPDATE SET tags=excluded.tags, asset=excluded.asset",
            params![key, png],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Вставить/заменить модель. Возвращает id.
    pub fn insert_model(&self, tags: &[&str], data: &[u8]) -> rusqlite::Result<i64> {
        let key = normalize_tags(tags);
        self.conn.execute(
            "INSERT INTO models(tags, asset) VALUES(?1, ?2)
             ON CONFLICT(id) DO UPDATE SET tags=excluded.tags, asset=excluded.asset",
            params![key, data],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn count_textures(&self) -> rusqlite::Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM textures", [], |r| r.get(0))
    }

    pub fn count_models(&self) -> rusqlite::Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM models", [], |r| r.get(0))
    }
}

/// Нормализация: trim + lowercase + сортировка + склейка через запятую.
pub fn normalize_tags(tags: &[&str]) -> String {
    let mut v: Vec<String> = tags
        .iter()
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    v.sort();
    v.dedup();
    v.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_order_independent() {
        assert_eq!(normalize_tags(&["Wall", "yellow"]), "wall,yellow");
        assert_eq!(normalize_tags(&["yellow", "wall"]), "wall,yellow");
        assert_eq!(normalize_tags(&["  Yellow ", "WALL", "yellow"]), "wall,yellow");
    }

    #[test]
    fn insert_and_find_texture() {
        let db = AssetDb::open_in_memory().unwrap();
        db.insert_texture(&["wall", "yellow"], b"\x89PNG_fake").unwrap();
        let got = db.find_texture(&["yellow", "wall"]).unwrap();
        assert_eq!(got.as_deref(), Some(b"\x89PNG_fake".as_slice()));
        assert!(db.find_texture(&["wall"]).unwrap().is_none());
        assert!(db.find_texture(&["yellow", "wall", "extra"]).unwrap().is_none());
    }

    #[test]
    fn insert_and_find_model() {
        let db = AssetDb::open_in_memory().unwrap();
        db.insert_model(&["chair", "wood"], b"# obj").unwrap();
        assert_eq!(
            db.find_model(&["wood", "chair"]).unwrap().as_deref(),
            Some(b"# obj".as_slice())
        );
    }

    #[test]
    fn counts_and_empty() {
        let db = AssetDb::open_in_memory().unwrap();
        assert_eq!(db.count_textures().unwrap(), 0);
        assert_eq!(db.count_models().unwrap(), 0);
        db.insert_texture(&["a"], b"1").unwrap();
        db.insert_texture(&["b"], b"2").unwrap();
        db.insert_model(&["m"], b"3").unwrap();
        assert_eq!(db.count_textures().unwrap(), 2);
        assert_eq!(db.count_models().unwrap(), 1);
    }
}