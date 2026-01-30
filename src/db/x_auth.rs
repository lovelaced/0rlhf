//! Database operations for X verification (claim-based flow with pairing codes)

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::error::Result;

/// Pending claim record (used during OAuth flow)
#[derive(Debug, FromRow)]
pub struct XPendingClaim {
    pub id: Uuid,
    pub agent_id: String,
    pub state: String,
    pub pairing_code: Option<String>,
    /// PKCE code verifier for secure OAuth token exchange
    pub code_verifier: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl super::Database {
    /// Create a pending claim for an agent with pairing code (starts OAuth flow)
    /// Also stores the PKCE code_verifier for secure token exchange
    pub async fn create_pending_claim_with_code(
        &self,
        agent_id: &str,
        state: &str,
        pairing_code: &str,
        code_verifier: &str,
    ) -> Result<XPendingClaim> {
        let claim = sqlx::query_as::<_, XPendingClaim>(
            r#"
            INSERT INTO x_pending_claims (agent_id, state, pairing_code, code_verifier)
            VALUES ($1, $2, $3, $4)
            RETURNING *
            "#,
        )
        .bind(agent_id)
        .bind(state)
        .bind(pairing_code)
        .bind(code_verifier)
        .fetch_one(&self.pool)
        .await?;

        Ok(claim)
    }

    /// Get pending claim by state (if not expired)
    pub async fn get_pending_claim(&self, state: &str) -> Result<Option<XPendingClaim>> {
        let claim = sqlx::query_as::<_, XPendingClaim>(
            r#"
            SELECT * FROM x_pending_claims
            WHERE state = $1
              AND expires_at > NOW()
            "#,
        )
        .bind(state)
        .fetch_optional(&self.pool)
        .await?;

        Ok(claim)
    }

    /// Delete a pending claim
    pub async fn delete_pending_claim(&self, id: &Uuid) -> Result<()> {
        sqlx::query("DELETE FROM x_pending_claims WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Clean up expired pending claims
    pub async fn cleanup_expired_pending_claims(&self) -> Result<u64> {
        let result = sqlx::query("DELETE FROM x_pending_claims WHERE expires_at < NOW()")
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }

    /// Claim an agent with X hash (marks as claimed and sets x_hash)
    /// Uses optimistic locking to prevent race conditions
    pub async fn claim_agent(&self, agent_id: &str, x_hash: &str) -> Result<()> {
        use crate::error::AppError;

        // Use UPDATE with WHERE clause to atomically claim only if unclaimed
        let result = sqlx::query(
            r#"
            UPDATE agents
            SET x_hash = $2, claimed_at = NOW()
            WHERE id = $1 AND claimed_at IS NULL AND x_hash IS NULL
            "#,
        )
        .bind(agent_id)
        .bind(x_hash)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::Conflict("Agent was already claimed".to_string()));
        }

        Ok(())
    }

    /// Check if an agent is claimed
    pub async fn is_agent_claimed(&self, agent_id: &str) -> Result<bool> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM agents
            WHERE id = $1
              AND claimed_at IS NOT NULL
            "#,
        )
        .bind(agent_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(count > 0)
    }

    /// Check if an X hash already has an active claimed agent
    pub async fn x_hash_has_active_agent(&self, x_hash: &str) -> Result<bool> {
        let (count,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM agents
            WHERE x_hash = $1
              AND deleted_at IS NULL
            "#,
        )
        .bind(x_hash)
        .fetch_one(&self.pool)
        .await?;

        Ok(count > 0)
    }

    /// Soft delete an agent (allows X hash reuse)
    /// Uses a transaction to ensure atomicity of key deletion and agent update
    pub async fn soft_delete_agent(&self, agent_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await?;

        // Revoke all API keys
        sqlx::query("DELETE FROM agent_keys WHERE agent_id = $1")
            .bind(agent_id)
            .execute(&mut *tx)
            .await?;

        // Soft delete the agent
        sqlx::query(
            r#"
            UPDATE agents
            SET deleted_at = NOW(), x_hash = NULL, claimed_at = NULL
            WHERE id = $1
            "#,
        )
        .bind(agent_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(())
    }

    /// List unclaimed agents (for UI)
    pub async fn list_unclaimed_agents(&self, limit: i64, offset: i64) -> Result<Vec<crate::models::Agent>> {
        let agents = sqlx::query_as::<_, crate::models::Agent>(
            r#"
            SELECT * FROM agents
            WHERE claimed_at IS NULL
              AND deleted_at IS NULL
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(agents)
    }
}
