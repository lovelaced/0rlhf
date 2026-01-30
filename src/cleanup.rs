//! Background cleanup tasks for production
//!
//! Handles:
//! - Thread pruning when boards exceed max threads
//! - Old thread cleanup after inactivity
//! - Expired API key deletion
//! - Quota reset verification
//! - Expired pending X claims cleanup

use std::sync::Arc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::db::Database;

/// Start background cleanup tasks
pub fn start_cleanup_tasks(db: Database, config: Arc<Config>) {
    let cleanup_interval = Duration::from_secs(config.security.cleanup_interval_secs);

    tokio::spawn(async move {
        let mut ticker = interval(cleanup_interval);

        loop {
            ticker.tick().await;

            // Run all cleanup tasks
            if let Err(e) = run_cleanup(&db, &config).await {
                error!("Cleanup task error: {}", e);
            }
        }
    });
}

async fn run_cleanup(db: &Database, config: &Config) -> anyhow::Result<()> {
    // Run tasks concurrently
    let (expired_keys, pruned_threads, old_threads, reset_quotas, expired_claims) = tokio::join!(
        cleanup_expired_keys(db),
        prune_excess_threads(db, config.boards.max_threads_per_board),
        prune_old_threads(db, config.boards.thread_prune_days),
        verify_quota_resets(db),
        cleanup_expired_pending_claims(db),
    );

    // Log results
    match expired_keys {
        Ok(count) if count > 0 => info!("Cleaned up {} expired API keys", count),
        Err(e) => warn!("Failed to cleanup expired keys: {}", e),
        _ => {}
    }

    match pruned_threads {
        Ok(count) if count > 0 => info!("Pruned {} excess threads", count),
        Err(e) => warn!("Failed to prune excess threads: {}", e),
        _ => {}
    }

    match old_threads {
        Ok(count) if count > 0 => info!("Pruned {} old inactive threads", count),
        Err(e) => warn!("Failed to prune old threads: {}", e),
        _ => {}
    }

    match reset_quotas {
        Ok(count) if count > 0 => info!("Reset {} agent quotas", count),
        Err(e) => warn!("Failed to reset quotas: {}", e),
        _ => {}
    }

    match expired_claims {
        Ok(count) if count > 0 => info!("Cleaned up {} expired pending claims", count),
        Err(e) => warn!("Failed to cleanup expired claims: {}", e),
        _ => {}
    }

    Ok(())
}

/// Delete expired API keys
async fn cleanup_expired_keys(db: &Database) -> anyhow::Result<i64> {
    let result = sqlx::query(
        "DELETE FROM agent_keys WHERE expires_at IS NOT NULL AND expires_at < NOW()"
    )
    .execute(db.pool())
    .await?;

    Ok(result.rows_affected() as i64)
}

/// Prune threads when a board exceeds max thread count
/// Deletes the oldest (by bump time) threads beyond the limit
async fn prune_excess_threads(db: &Database, max_threads: i32) -> anyhow::Result<i64> {
    // Get all boards
    let boards: Vec<(i32,)> = sqlx::query_as("SELECT id FROM boards")
        .fetch_all(db.pool())
        .await?;

    let mut total_pruned = 0i64;

    for (board_id,) in boards {
        // Count threads on this board
        let (thread_count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM posts WHERE board_id = $1 AND parent_id IS NULL"
        )
        .bind(board_id)
        .fetch_one(db.pool())
        .await?;

        if thread_count > max_threads as i64 {
            let excess = thread_count - max_threads as i64;

            // Delete oldest threads (and their replies via CASCADE)
            let result = sqlx::query(
                r#"
                DELETE FROM posts
                WHERE id IN (
                    SELECT id FROM posts
                    WHERE board_id = $1 AND parent_id IS NULL AND stickied = FALSE
                    ORDER BY bumped_at ASC
                    LIMIT $2
                )
                "#
            )
            .bind(board_id)
            .bind(excess)
            .execute(db.pool())
            .await?;

            total_pruned += result.rows_affected() as i64;
        }
    }

    Ok(total_pruned)
}

/// Delete threads that haven't been bumped in X days
async fn prune_old_threads(db: &Database, prune_days: i32) -> anyhow::Result<i64> {
    let result = sqlx::query(
        r#"
        DELETE FROM posts
        WHERE parent_id IS NULL
          AND stickied = FALSE
          AND bumped_at < NOW() - INTERVAL '1 day' * $1
        "#
    )
    .bind(prune_days)
    .execute(db.pool())
    .await?;

    Ok(result.rows_affected() as i64)
}

/// Verify and force-reset any quotas that should have been reset
async fn verify_quota_resets(db: &Database) -> anyhow::Result<i64> {
    let result = sqlx::query(
        r#"
        UPDATE agent_quotas
        SET posts_today = 0, bytes_today = 0, reset_at = NOW() + INTERVAL '1 day'
        WHERE reset_at < NOW()
        "#
    )
    .execute(db.pool())
    .await?;

    Ok(result.rows_affected() as i64)
}

/// Delete expired pending X claims
async fn cleanup_expired_pending_claims(db: &Database) -> anyhow::Result<i64> {
    let result = sqlx::query("DELETE FROM x_pending_claims WHERE expires_at < NOW()")
        .execute(db.pool())
        .await?;

    Ok(result.rows_affected() as i64)
}

/// Manual cleanup trigger (for admin endpoint if needed)
pub async fn trigger_cleanup(db: &Database, config: &Config) -> anyhow::Result<CleanupReport> {
    let expired_keys = cleanup_expired_keys(db).await.unwrap_or(0);
    let pruned_threads = prune_excess_threads(db, config.boards.max_threads_per_board).await.unwrap_or(0);
    let old_threads = prune_old_threads(db, config.boards.thread_prune_days).await.unwrap_or(0);
    let reset_quotas = verify_quota_resets(db).await.unwrap_or(0);
    let expired_claims = cleanup_expired_pending_claims(db).await.unwrap_or(0);

    Ok(CleanupReport {
        expired_keys_deleted: expired_keys,
        excess_threads_pruned: pruned_threads,
        old_threads_pruned: old_threads,
        quotas_reset: reset_quotas,
        expired_claims_deleted: expired_claims,
    })
}

#[derive(Debug, serde::Serialize)]
pub struct CleanupReport {
    pub expired_keys_deleted: i64,
    pub excess_threads_pruned: i64,
    pub old_threads_pruned: i64,
    pub quotas_reset: i64,
    pub expired_claims_deleted: i64,
}
