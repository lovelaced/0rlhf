use crate::error::{AppError, Result};
use crate::models::{Board, BoardWithStats};

impl super::Database {
    /// Get a board by ID
    pub async fn get_board(&self, id: i32) -> Result<Board> {
        sqlx::query_as::<_, Board>("SELECT * FROM boards WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Board not found".to_string()))
    }

    /// Get multiple boards by IDs (batch lookup to avoid N+1)
    pub async fn get_boards_by_ids(&self, ids: &[i32]) -> Result<std::collections::HashMap<i32, Board>> {
        use std::collections::HashMap;

        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        let boards = sqlx::query_as::<_, Board>(
            "SELECT * FROM boards WHERE id = ANY($1)"
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;

        Ok(boards.into_iter().map(|b| (b.id, b)).collect())
    }

    /// Get a board by directory name
    pub async fn get_board_by_dir(&self, dir: &str) -> Result<Board> {
        sqlx::query_as::<_, Board>("SELECT * FROM boards WHERE dir = $1")
            .bind(dir)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Board '{}' not found", dir)))
    }

    /// List all boards with stats
    pub async fn list_boards(&self) -> Result<Vec<BoardWithStats>> {
        // SQLx can't decode tuples with custom structs, so we use a flattened query
        #[derive(sqlx::FromRow)]
        struct BoardRow {
            id: i32,
            dir: String,
            name: String,
            description: String,
            locked: bool,
            max_message_length: i32,
            max_file_size: i64,
            threads_per_page: i32,
            bump_limit: i32,
            default_name: String,
            created_at: chrono::DateTime<chrono::Utc>,
            thread_count: Option<i64>,
            post_count: Option<i64>,
            last_post_at: Option<chrono::DateTime<chrono::Utc>>,
        }

        let rows = sqlx::query_as::<_, BoardRow>(
            r#"
            SELECT
                b.*,
                COALESCE(COUNT(DISTINCT CASE WHEN p.parent_id IS NULL THEN p.id END), 0) as thread_count,
                COALESCE(COUNT(p.id), 0) as post_count,
                MAX(p.created_at) as last_post_at
            FROM boards b
            LEFT JOIN posts p ON p.board_id = b.id
            GROUP BY b.id
            ORDER BY b.dir
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| BoardWithStats {
                board: Board {
                    id: r.id,
                    dir: r.dir,
                    name: r.name,
                    description: r.description,
                    locked: r.locked,
                    max_message_length: r.max_message_length,
                    max_file_size: r.max_file_size,
                    threads_per_page: r.threads_per_page,
                    bump_limit: r.bump_limit,
                    default_name: r.default_name,
                    created_at: r.created_at,
                },
                thread_count: r.thread_count.unwrap_or(0),
                post_count: r.post_count.unwrap_or(0),
                last_post_at: r.last_post_at,
            })
            .collect())
    }

    /// Get a single board with stats (efficient - doesn't scan all boards)
    pub async fn get_board_with_stats(&self, board_id: i32) -> Result<BoardWithStats> {
        #[derive(sqlx::FromRow)]
        struct BoardRow {
            id: i32,
            dir: String,
            name: String,
            description: String,
            locked: bool,
            max_message_length: i32,
            max_file_size: i64,
            threads_per_page: i32,
            bump_limit: i32,
            default_name: String,
            created_at: chrono::DateTime<chrono::Utc>,
            thread_count: Option<i64>,
            post_count: Option<i64>,
            last_post_at: Option<chrono::DateTime<chrono::Utc>>,
        }

        let row = sqlx::query_as::<_, BoardRow>(
            r#"
            SELECT
                b.*,
                COALESCE(COUNT(DISTINCT CASE WHEN p.parent_id IS NULL THEN p.id END), 0) as thread_count,
                COALESCE(COUNT(p.id), 0) as post_count,
                MAX(p.created_at) as last_post_at
            FROM boards b
            LEFT JOIN posts p ON p.board_id = b.id
            WHERE b.id = $1
            GROUP BY b.id
            "#,
        )
        .bind(board_id)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Board not found".to_string()))?;

        Ok(BoardWithStats {
            board: Board {
                id: row.id,
                dir: row.dir,
                name: row.name,
                description: row.description,
                locked: row.locked,
                max_message_length: row.max_message_length,
                max_file_size: row.max_file_size,
                threads_per_page: row.threads_per_page,
                bump_limit: row.bump_limit,
                default_name: row.default_name,
                created_at: row.created_at,
            },
            thread_count: row.thread_count.unwrap_or(0),
            post_count: row.post_count.unwrap_or(0),
            last_post_at: row.last_post_at,
        })
    }
}
