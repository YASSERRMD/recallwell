//! Persistent ask history with SQLite + FTS5 search.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AskRecord {
    pub id: String,
    pub library: String,
    pub question: String,
    pub answer: String,
    #[serde(default)]
    pub citations_json: serde_json::Value,
    #[serde(default)]
    pub trace_json: serde_json::Value,
    pub created_at: i64,
}

pub struct History {
    pool: SqlitePool,
}

impl History {
    pub async fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await
            .with_context(|| format!("opening history db at {}", path.display()))?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS asks (
                id TEXT PRIMARY KEY,
                library TEXT NOT NULL,
                question TEXT NOT NULL,
                answer TEXT NOT NULL,
                citations_json TEXT NOT NULL DEFAULT '[]',
                trace_json TEXT NOT NULL DEFAULT '{}',
                created_at INTEGER NOT NULL
            );
            "#,
        )
        .execute(&pool)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_asks_created ON asks(created_at DESC);")
            .execute(&pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_asks_library ON asks(library);")
            .execute(&pool)
            .await?;
        sqlx::query(
            "CREATE VIRTUAL TABLE IF NOT EXISTS asks_fts USING fts5(\
                question, answer, content='asks', content_rowid='rowid');",
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    pub async fn record(&self, ask: &AskRecord) -> Result<()> {
        let citations = serde_json::to_string(&ask.citations_json)?;
        let trace = serde_json::to_string(&ask.trace_json)?;
        sqlx::query(
            "INSERT INTO asks (id, library, question, answer, citations_json, trace_json, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(&ask.id)
        .bind(&ask.library)
        .bind(&ask.question)
        .bind(&ask.answer)
        .bind(citations)
        .bind(trace)
        .bind(ask.created_at)
        .execute(&self.pool)
        .await?;
        // Mirror into FTS.
        sqlx::query("INSERT INTO asks_fts(rowid, question, answer) SELECT rowid, question, answer FROM asks WHERE id = ?1")
            .bind(&ask.id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list(
        &self,
        library: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<AskRecord>> {
        let limit = limit.min(500) as i64;
        let offset = offset as i64;
        let rows = if let Some(lib) = library {
            sqlx::query(
                "SELECT id, library, question, answer, citations_json, trace_json, created_at \
                 FROM asks WHERE library = ?1 ORDER BY created_at DESC LIMIT ?2 OFFSET ?3",
            )
            .bind(lib)
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT id, library, question, answer, citations_json, trace_json, created_at \
                 FROM asks ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
            )
            .bind(limit)
            .bind(offset)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(rows.into_iter().map(row_to_record).collect())
    }

    pub async fn get(&self, id: &str) -> Result<Option<AskRecord>> {
        let row = sqlx::query(
            "SELECT id, library, question, answer, citations_json, trace_json, created_at \
             FROM asks WHERE id = ?1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(row_to_record))
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<AskRecord>> {
        let limit = limit.min(500) as i64;
        let rows = sqlx::query(
            "SELECT asks.id, asks.library, asks.question, asks.answer, \
                    asks.citations_json, asks.trace_json, asks.created_at \
             FROM asks_fts JOIN asks ON asks.rowid = asks_fts.rowid \
             WHERE asks_fts MATCH ?1 ORDER BY asks.created_at DESC LIMIT ?2",
        )
        .bind(query)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(row_to_record).collect())
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM asks WHERE id = ?1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM asks_fts WHERE rowid IN (SELECT rowid FROM asks WHERE id = ?1)")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn clear(&self, library: Option<&str>) -> Result<u64> {
        let res = if let Some(lib) = library {
            sqlx::query("DELETE FROM asks WHERE library = ?1")
                .bind(lib)
                .execute(&self.pool)
                .await?
        } else {
            sqlx::query("DELETE FROM asks").execute(&self.pool).await?
        };
        sqlx::query("DELETE FROM asks_fts")
            .execute(&self.pool)
            .await
            .ok();
        Ok(res.rows_affected())
    }
}

fn row_to_record(row: sqlx::sqlite::SqliteRow) -> AskRecord {
    let id: String = row.try_get("id").unwrap_or_default();
    let library: String = row.try_get("library").unwrap_or_default();
    let question: String = row.try_get("question").unwrap_or_default();
    let answer: String = row.try_get("answer").unwrap_or_default();
    let citations_str: String = row.try_get("citations_json").unwrap_or_default();
    let trace_str: String = row.try_get("trace_json").unwrap_or_default();
    let created_at: i64 = row.try_get("created_at").unwrap_or_default();
    AskRecord {
        id,
        library,
        question,
        answer,
        citations_json: serde_json::from_str(&citations_str).unwrap_or(serde_json::json!([])),
        trace_json: serde_json::from_str(&trace_str).unwrap_or(serde_json::json!({})),
        created_at,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample(id: &str, library: &str, q: &str, a: &str, ts: i64) -> AskRecord {
        AskRecord {
            id: id.into(),
            library: library.into(),
            question: q.into(),
            answer: a.into(),
            citations_json: serde_json::json!([]),
            trace_json: serde_json::json!({}),
            created_at: ts,
        }
    }

    #[tokio::test]
    async fn record_list_get_delete_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("h.db");
        let h = History::open(&path).await.unwrap();
        h.record(&sample("1", "reading", "what is x?", "x is y", 100))
            .await
            .unwrap();
        h.record(&sample("2", "work", "how did we do z?", "we did q", 200))
            .await
            .unwrap();

        let all = h.list(None, 10, 0).await.unwrap();
        assert_eq!(all.len(), 2);
        // Sorted by created_at DESC.
        assert_eq!(all[0].id, "2");

        let just_reading = h.list(Some("reading"), 10, 0).await.unwrap();
        assert_eq!(just_reading.len(), 1);
        assert_eq!(just_reading[0].library, "reading");

        let one = h.get("1").await.unwrap();
        assert!(one.is_some());

        h.delete("1").await.unwrap();
        assert!(h.get("1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn fts_search_finds_questions() {
        let dir = tempdir().unwrap();
        let h = History::open(&dir.path().join("h.db")).await.unwrap();
        h.record(&sample(
            "1",
            "reading",
            "consensus algorithms",
            "raft and paxos",
            1,
        ))
        .await
        .unwrap();
        h.record(&sample("2", "reading", "carbon policy", "lots of trees", 2))
            .await
            .unwrap();

        let hits = h.search("consensus", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "1");

        let hits = h.search("trees", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "2");
    }
}
