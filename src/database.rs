//! Local-first SQLite persistence (via `rusqlite` with the bundled engine, so
//! there's zero external database to install). Stores characters, their genres,
//! and optional visual embeddings.

use crate::models::Character;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Database { conn };
        db.init()?;
        Ok(db)
    }

    /// In-memory database, for tests.
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let db = Database { conn };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS characters (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT NOT NULL UNIQUE,
                archetype   TEXT NOT NULL,
                role        TEXT,
                hair_color  TEXT,
                eye_color   TEXT,
                gender      TEXT,
                age         INTEGER,
                series      TEXT,
                genres      TEXT,            -- JSON array
                popularity  INTEGER,
                image_url   TEXT,
                description TEXT,
                source      TEXT,
                source_url  TEXT,
                anilist_id  INTEGER,
                embedding   BLOB             -- little-endian f32 vector, optional
            );
            CREATE INDEX IF NOT EXISTS idx_characters_archetype ON characters(archetype);
            "#,
        )?;
        Ok(())
    }

    /// Inserts or updates a character by name. Preserves an existing embedding
    /// when the incoming record doesn't carry one (re-`add` shouldn't wipe it).
    pub fn upsert(&self, c: &Character) -> Result<i64> {
        let genres = serde_json::to_string(&c.genres)?;
        self.conn.execute(
            r#"
            INSERT INTO characters
                (name, archetype, role, hair_color, eye_color, gender, age,
                 series, genres, popularity, image_url, description,
                 source, source_url, anilist_id)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)
            ON CONFLICT(name) DO UPDATE SET
                archetype=excluded.archetype,
                role=excluded.role,
                hair_color=excluded.hair_color,
                eye_color=excluded.eye_color,
                gender=excluded.gender,
                age=excluded.age,
                series=excluded.series,
                genres=excluded.genres,
                popularity=excluded.popularity,
                image_url=COALESCE(excluded.image_url, characters.image_url),
                description=COALESCE(excluded.description, characters.description),
                source=excluded.source,
                source_url=excluded.source_url,
                anilist_id=excluded.anilist_id
            "#,
            params![
                c.name,
                c.archetype,
                c.role,
                c.hair_color,
                c.eye_color,
                c.gender,
                c.age,
                c.series,
                genres,
                c.popularity,
                c.image_url,
                c.description,
                c.source,
                c.source_url,
                c.anilist_id,
            ],
        )?;
        Ok(self
            .conn
            .query_row(
                "SELECT id FROM characters WHERE name = ?1",
                params![c.name],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0))
    }

    pub fn all(&self) -> Result<Vec<Character>> {
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT {COLS} FROM characters ORDER BY name"))?;
        let rows = stmt.query_map([], row_to_character)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Case-insensitive exact lookup by name.
    pub fn get(&self, name: &str) -> Result<Option<Character>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {COLS} FROM characters WHERE name = ?1 COLLATE NOCASE"
        ))?;
        Ok(stmt.query_row(params![name], row_to_character).optional()?)
    }

    pub fn remove(&self, name: &str) -> Result<bool> {
        let n = self.conn.execute(
            "DELETE FROM characters WHERE name = ?1 COLLATE NOCASE",
            params![name],
        )?;
        Ok(n > 0)
    }

    pub fn count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM characters", [], |r| r.get(0))?)
    }

    // ── Embeddings ──────────────────────────────────────────────────────────

    pub fn set_embedding(&self, name: &str, vec: &[f32]) -> Result<()> {
        let mut bytes = Vec::with_capacity(vec.len() * 4);
        for v in vec {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
        self.conn.execute(
            "UPDATE characters SET embedding = ?1 WHERE name = ?2 COLLATE NOCASE",
            params![bytes, name],
        )?;
        Ok(())
    }

    pub fn embedding(&self, name: &str) -> Result<Option<Vec<f32>>> {
        let blob: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT embedding FROM characters WHERE name = ?1 COLLATE NOCASE",
                params![name],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        Ok(blob.map(|b| bytes_to_f32(&b)))
    }

    /// Names of characters that don't yet have an embedding.
    pub fn names_missing_embedding(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM characters WHERE embedding IS NULL ORDER BY name")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
}

const COLS: &str = "id, name, archetype, role, hair_color, eye_color, gender, age, \
                    series, genres, popularity, image_url, description, source, source_url, anilist_id";

fn row_to_character(r: &rusqlite::Row) -> rusqlite::Result<Character> {
    let genres_json: Option<String> = r.get("genres")?;
    let genres = genres_json
        .and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok())
        .unwrap_or_default();
    Ok(Character {
        id: r.get("id")?,
        name: r.get("name")?,
        archetype: r.get("archetype")?,
        role: r.get("role")?,
        hair_color: r.get("hair_color")?,
        eye_color: r.get("eye_color")?,
        gender: r.get("gender")?,
        age: r.get("age")?,
        series: r.get("series")?,
        genres,
        popularity: r.get("popularity")?,
        image_url: r.get("image_url")?,
        description: r.get("description")?,
        source: r.get("source")?,
        source_url: r.get("source_url")?,
        anilist_id: r.get("anilist_id")?,
        match_score: 0.0,
    })
}

fn bytes_to_f32(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Character {
        let mut c = Character::new("Taiga Aisaka");
        c.archetype = "Tsundere".into();
        c.role = Some("Protagonist".into());
        c.hair_color = Some("Brown".into());
        c.genres = vec!["Romance".into(), "Comedy".into()];
        c.source = Some("seed".into());
        c
    }

    #[test]
    fn upsert_and_get_roundtrip() {
        let db = Database::in_memory().unwrap();
        db.upsert(&sample()).unwrap();
        let got = db.get("taiga aisaka").unwrap().unwrap();
        assert_eq!(got.archetype, "Tsundere");
        assert_eq!(got.genres, vec!["Romance", "Comedy"]);
        assert_eq!(db.count().unwrap(), 1);
    }

    #[test]
    fn upsert_is_idempotent_and_preserves_embedding() {
        let db = Database::in_memory().unwrap();
        db.upsert(&sample()).unwrap();
        db.set_embedding("Taiga Aisaka", &[0.1, 0.2, 0.3]).unwrap();
        // Re-add (no embedding on the incoming record) must keep the stored one.
        db.upsert(&sample()).unwrap();
        assert_eq!(db.count().unwrap(), 1);
        assert_eq!(
            db.embedding("Taiga Aisaka").unwrap().unwrap(),
            vec![0.1, 0.2, 0.3]
        );
    }

    #[test]
    fn remove_works() {
        let db = Database::in_memory().unwrap();
        db.upsert(&sample()).unwrap();
        assert!(db.remove("Taiga Aisaka").unwrap());
        assert!(!db.remove("Taiga Aisaka").unwrap());
        assert_eq!(db.count().unwrap(), 0);
    }
}
