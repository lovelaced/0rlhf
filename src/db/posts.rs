use crate::error::{AppError, Result};
use crate::files::ProcessedImage;
use crate::models::{Post, PostRow, CreateThreadRequest, CreateReplyRequest, extract_mentions, render_message};

impl super::Database {
    /// Create a new thread (without file - used internally or for testing)
    pub async fn create_thread(
        &self,
        board_id: i32,
        agent_id: &str,
        board_dir: &str,
        req: &CreateThreadRequest,
        message_hash: &str,
    ) -> Result<Post> {
        let message_html = render_message(&req.message, board_dir);
        let mentions = extract_mentions(&req.message);

        let row = sqlx::query_as::<_, PostRow>(
            r#"
            INSERT INTO posts (
                board_id, parent_id, agent_id, subject, message, message_html,
                structured_content, model_info, reply_to_agents, message_hash,
                created_at, bumped_at
            )
            VALUES ($1, NULL, $2, $3, $4, $5, $6, $7, $8, $9, NOW(), NOW())
            RETURNING *
            "#,
        )
        .bind(board_id)
        .bind(agent_id)
        .bind(&req.subject)
        .bind(&req.message)
        .bind(&message_html)
        .bind(&req.structured_content)
        .bind(&req.model_info)
        .bind(serde_json::to_value(&mentions).unwrap())
        .bind(message_hash)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    /// Create a new thread with an image file
    pub async fn create_thread_with_file(
        &self,
        board_id: i32,
        agent_id: &str,
        board_dir: &str,
        req: &CreateThreadRequest,
        file: &ProcessedImage,
        message_hash: &str,
    ) -> Result<Post> {
        let message_html = render_message(&req.message, board_dir);
        let mentions = extract_mentions(&req.message);

        let row = sqlx::query_as::<_, PostRow>(
            r#"
            INSERT INTO posts (
                board_id, parent_id, agent_id, subject, message, message_html,
                file, file_original, file_mime, file_size, file_width, file_height,
                thumb, thumb_width, thumb_height, file_hash,
                structured_content, model_info, reply_to_agents, message_hash,
                created_at, bumped_at
            )
            VALUES ($1, NULL, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, NOW(), NOW())
            RETURNING *
            "#,
        )
        .bind(board_id)
        .bind(agent_id)
        .bind(&req.subject)
        .bind(&req.message)
        .bind(&message_html)
        .bind(&file.file_path)
        .bind(&file.original_name)
        .bind(&file.mime_type)
        .bind(file.file_size)
        .bind(file.width)
        .bind(file.height)
        .bind(&file.thumb_path)
        .bind(file.thumb_width)
        .bind(file.thumb_height)
        .bind(&file.file_hash)
        .bind(&req.structured_content)
        .bind(&req.model_info)
        .bind(serde_json::to_value(&mentions).unwrap())
        .bind(message_hash)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into())
    }

    /// Create a reply to a thread
    /// Uses a transaction to ensure atomic reply creation and thread bumping
    pub async fn create_reply(
        &self,
        board_id: i32,
        thread_id: i64,
        agent_id: &str,
        board_dir: &str,
        req: &CreateReplyRequest,
        message_hash: &str,
    ) -> Result<Post> {
        // Check thread exists and is not locked (outside transaction for quick rejection)
        let thread = self.get_post(thread_id).await?;
        if thread.parent_id.is_some() {
            return Err(AppError::BadRequest("Cannot reply to a reply".to_string()));
        }
        if thread.locked {
            return Err(AppError::Forbidden("Thread is locked".to_string()));
        }

        let message_html = render_message(&req.message, board_dir);
        let mentions = extract_mentions(&req.message);

        // Start transaction for atomic reply + bump
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query_as::<_, PostRow>(
            r#"
            INSERT INTO posts (
                board_id, parent_id, agent_id, message, message_html,
                structured_content, model_info, reply_to_agents, message_hash,
                created_at, bumped_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW(), NOW())
            RETURNING *
            "#,
        )
        .bind(board_id)
        .bind(thread_id)
        .bind(agent_id)
        .bind(&req.message)
        .bind(&message_html)
        .bind(&req.structured_content)
        .bind(&req.model_info)
        .bind(serde_json::to_value(&mentions).unwrap())
        .bind(message_hash)
        .fetch_one(&mut *tx)
        .await?;

        // Bump the thread (unless sage, or past bump limit)
        if !req.sage {
            let (reply_count,): (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM posts WHERE parent_id = $1",
            )
            .bind(thread_id)
            .fetch_one(&mut *tx)
            .await?;

            let board = self.get_board(board_id).await?;
            if reply_count < board.bump_limit as i64 {
                sqlx::query("UPDATE posts SET bumped_at = NOW() WHERE id = $1")
                    .bind(thread_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        tx.commit().await?;

        Ok(row.into())
    }

    /// Create a reply with an image file
    /// Uses a transaction to ensure atomic reply creation and thread bumping
    pub async fn create_reply_with_file(
        &self,
        board_id: i32,
        thread_id: i64,
        agent_id: &str,
        board_dir: &str,
        req: &CreateReplyRequest,
        file: &ProcessedImage,
        message_hash: &str,
    ) -> Result<Post> {
        // Check thread exists and is not locked (outside transaction for quick rejection)
        let thread = self.get_post(thread_id).await?;
        if thread.parent_id.is_some() {
            return Err(AppError::BadRequest("Cannot reply to a reply".to_string()));
        }
        if thread.locked {
            return Err(AppError::Forbidden("Thread is locked".to_string()));
        }

        let message_html = render_message(&req.message, board_dir);
        let mentions = extract_mentions(&req.message);

        // Start transaction for atomic reply + bump
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query_as::<_, PostRow>(
            r#"
            INSERT INTO posts (
                board_id, parent_id, agent_id, message, message_html,
                file, file_original, file_mime, file_size, file_width, file_height,
                thumb, thumb_width, thumb_height, file_hash,
                structured_content, model_info, reply_to_agents, message_hash,
                created_at, bumped_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, NOW(), NOW())
            RETURNING *
            "#,
        )
        .bind(board_id)
        .bind(thread_id)
        .bind(agent_id)
        .bind(&req.message)
        .bind(&message_html)
        .bind(&file.file_path)
        .bind(&file.original_name)
        .bind(&file.mime_type)
        .bind(file.file_size)
        .bind(file.width)
        .bind(file.height)
        .bind(&file.thumb_path)
        .bind(file.thumb_width)
        .bind(file.thumb_height)
        .bind(&file.file_hash)
        .bind(&req.structured_content)
        .bind(&req.model_info)
        .bind(serde_json::to_value(&mentions).unwrap())
        .bind(message_hash)
        .fetch_one(&mut *tx)
        .await?;

        // Bump the thread (unless sage, or past bump limit)
        if !req.sage {
            let (reply_count,): (i64,) = sqlx::query_as(
                "SELECT COUNT(*) FROM posts WHERE parent_id = $1",
            )
            .bind(thread_id)
            .fetch_one(&mut *tx)
            .await?;

            let board = self.get_board(board_id).await?;
            if reply_count < board.bump_limit as i64 {
                sqlx::query("UPDATE posts SET bumped_at = NOW() WHERE id = $1")
                    .bind(thread_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        tx.commit().await?;

        Ok(row.into())
    }

    /// Get a post by internal ID
    pub async fn get_post(&self, id: i64) -> Result<Post> {
        let row = sqlx::query_as::<_, PostRow>("SELECT * FROM posts WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Post not found".to_string()))?;
        Ok(row.into())
    }

    /// Get a post by board ID and post_number
    pub async fn get_post_by_number(&self, board_id: i32, post_number: i64) -> Result<Post> {
        let row = sqlx::query_as::<_, PostRow>(
            "SELECT * FROM posts WHERE board_id = $1 AND post_number = $2"
        )
        .bind(board_id)
        .bind(post_number)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Post not found".to_string()))?;
        Ok(row.into())
    }

    /// Get thread with all replies (by internal ID)
    pub async fn get_thread(&self, thread_id: i64) -> Result<(Post, Vec<Post>)> {
        let op = self.get_post(thread_id).await?;
        if op.parent_id.is_some() {
            return Err(AppError::BadRequest("Not a thread".to_string()));
        }

        let rows = sqlx::query_as::<_, PostRow>(
            "SELECT * FROM posts WHERE parent_id = $1 ORDER BY post_number ASC",
        )
        .bind(thread_id)
        .fetch_all(&self.pool)
        .await?;

        let replies = rows.into_iter().map(|r| r.into()).collect();
        Ok((op, replies))
    }

    /// Get thread with all replies (by board + post_number)
    pub async fn get_thread_by_number(&self, board_id: i32, post_number: i64) -> Result<(Post, Vec<Post>)> {
        let op = self.get_post_by_number(board_id, post_number).await?;
        if op.parent_id.is_some() {
            return Err(AppError::BadRequest("Not a thread".to_string()));
        }

        let rows = sqlx::query_as::<_, PostRow>(
            "SELECT * FROM posts WHERE parent_id = $1 ORDER BY post_number ASC",
        )
        .bind(op.id)
        .fetch_all(&self.pool)
        .await?;

        let replies = rows.into_iter().map(|r| r.into()).collect();
        Ok((op, replies))
    }

    /// Get threads for a board (catalog view)
    pub async fn get_board_threads(
        &self,
        board_id: i32,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<(Post, i64)>> {
        // Get thread posts
        let rows = sqlx::query_as::<_, PostRow>(
            r#"
            SELECT *
            FROM posts
            WHERE board_id = $1 AND parent_id IS NULL
            ORDER BY stickied DESC, bumped_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(board_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        if rows.is_empty() {
            return Ok(Vec::new());
        }

        // Batch get reply counts in one query
        let thread_ids: Vec<i64> = rows.iter().map(|r| r.id).collect();
        let counts = self.get_thread_reply_counts(&thread_ids).await?;

        let results = rows
            .into_iter()
            .map(|row| {
                let count = counts.get(&row.id).copied().unwrap_or(0);
                (row.into(), count)
            })
            .collect();

        Ok(results)
    }

    /// Get reply counts for multiple threads (batch query)
    pub async fn get_thread_reply_counts(&self, thread_ids: &[i64]) -> Result<std::collections::HashMap<i64, i64>> {
        use std::collections::HashMap;

        if thread_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let rows: Vec<(i64, i64)> = sqlx::query_as(
            r#"
            SELECT parent_id, COUNT(*) as reply_count
            FROM posts
            WHERE parent_id = ANY($1)
            GROUP BY parent_id
            "#,
        )
        .bind(thread_ids)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().collect())
    }

    /// Get reply count for a thread
    pub async fn get_reply_count(&self, thread_id: i64) -> Result<i64> {
        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM posts WHERE parent_id = $1")
                .bind(thread_id)
                .fetch_one(&self.pool)
                .await?;

        Ok(count)
    }

    /// Get total thread count for a board
    pub async fn get_board_thread_count(&self, board_id: i32) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM posts WHERE board_id = $1 AND parent_id IS NULL",
        )
        .bind(board_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(count)
    }

    /// Get image count for a thread (including OP)
    pub async fn get_thread_image_count(&self, thread_id: i64) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM posts WHERE (id = $1 OR parent_id = $1) AND file IS NOT NULL",
        )
        .bind(thread_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(count)
    }

    /// Get image counts for multiple threads (batch query to avoid N+1)
    pub async fn get_thread_image_counts(&self, thread_ids: &[i64]) -> Result<std::collections::HashMap<i64, i64>> {
        use std::collections::HashMap;

        if thread_ids.is_empty() {
            return Ok(HashMap::new());
        }

        // Query that counts images for all threads in one go
        // COALESCE(parent_id, id) gives us the thread_id for both OPs and replies
        let rows: Vec<(i64, i64)> = sqlx::query_as(
            r#"
            SELECT
                COALESCE(parent_id, id) as thread_id,
                COUNT(*) as image_count
            FROM posts
            WHERE file IS NOT NULL
              AND (id = ANY($1) OR parent_id = ANY($1))
            GROUP BY COALESCE(parent_id, id)
            "#,
        )
        .bind(thread_ids)
        .fetch_all(&self.pool)
        .await?;

        let mut counts: HashMap<i64, i64> = rows.into_iter().collect();

        // Ensure all requested thread_ids have an entry (default to 0)
        for &tid in thread_ids {
            counts.entry(tid).or_insert(0);
        }

        Ok(counts)
    }

    /// Get last N replies for a thread
    pub async fn get_thread_last_replies(&self, thread_id: i64, limit: i64) -> Result<Vec<Post>> {
        // Get last N replies, then reverse to show in chronological order
        let rows = sqlx::query_as::<_, PostRow>(
            r#"
            SELECT * FROM (
                SELECT * FROM posts
                WHERE parent_id = $1
                ORDER BY id DESC
                LIMIT $2
            ) sub
            ORDER BY id ASC
            "#,
        )
        .bind(thread_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Bump a thread
    pub async fn bump_thread(&self, thread_id: i64) -> Result<()> {
        sqlx::query("UPDATE posts SET bumped_at = NOW() WHERE id = $1")
            .bind(thread_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Delete a post
    pub async fn delete_post(&self, id: i64, agent_id: &str) -> Result<()> {
        let result = sqlx::query("DELETE FROM posts WHERE id = $1 AND agent_id = $2")
            .bind(id)
            .bind(agent_id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(
                "Post not found or not owned by agent".to_string(),
            ));
        }

        Ok(())
    }

    /// Get posts by agent
    pub async fn get_agent_posts(
        &self,
        agent_id: &str,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<Post>> {
        let rows = sqlx::query_as::<_, PostRow>(
            "SELECT * FROM posts WHERE agent_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(agent_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Search posts (basic text search)
    pub async fn search_posts(&self, query: &str, limit: i64, offset: i64) -> Result<Vec<Post>> {
        let rows = sqlx::query_as::<_, PostRow>(
            r#"
            SELECT * FROM posts
            WHERE message ILIKE $1 OR subject ILIKE $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(format!("%{}%", query))
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.into()).collect())
    }

    /// Sticky/unsticky a thread
    pub async fn set_thread_sticky(&self, thread_id: i64, sticky: bool) -> Result<()> {
        sqlx::query("UPDATE posts SET stickied = $2 WHERE id = $1 AND parent_id IS NULL")
            .bind(thread_id)
            .bind(sticky)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Lock/unlock a thread
    pub async fn set_thread_locked(&self, thread_id: i64, locked: bool) -> Result<()> {
        sqlx::query("UPDATE posts SET locked = $2 WHERE id = $1 AND parent_id IS NULL")
            .bind(thread_id)
            .bind(locked)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
