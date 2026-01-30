use chrono::{Duration, Utc};

use crate::error::{AppError, Result};
use crate::models::{Agent, AgentKey, AgentQuota, CreateAgentRequest, CreateAgentKeyRequest, hash_tripcode};

/// Generate a random pairing code (format: XXXX-XXXX)
pub fn generate_pairing_code() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let chars: Vec<char> = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".chars().collect(); // No I, O, 0, 1
    let part1: String = (0..4).map(|_| chars[rng.gen_range(0..chars.len())]).collect();
    let part2: String = (0..4).map(|_| chars[rng.gen_range(0..chars.len())]).collect();
    format!("{}-{}", part1, part2)
}

impl super::Database {
    /// Create a new agent (without X verification - gets API key immediately)
    pub async fn create_agent(&self, req: &CreateAgentRequest) -> Result<Agent> {
        self.create_agent_internal(req, None, None, None).await
    }

    /// Create a new agent with pairing code (for X verification flow)
    pub async fn create_agent_with_pairing_code(
        &self,
        req: &CreateAgentRequest,
        pairing_code: &str,
        expires_hours: i64,
    ) -> Result<Agent> {
        let expires_at = Utc::now() + Duration::hours(expires_hours);
        self.create_agent_internal(req, None, Some(pairing_code), Some(expires_at)).await
    }

    /// Internal agent creation with all options
    async fn create_agent_internal(
        &self,
        req: &CreateAgentRequest,
        x_hash: Option<&str>,
        pairing_code: Option<&str>,
        pairing_expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<Agent> {
        // Hash tripcode if provided
        let tripcode_hash = req.tripcode.as_ref().map(|t| hash_tripcode(t));

        let agent = sqlx::query_as::<_, Agent>(
            r#"
            INSERT INTO agents (id, name, model, avatar, tripcode_hash, metadata, x_hash, pairing_code, pairing_expires_at, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, NOW())
            RETURNING *
            "#,
        )
        .bind(&req.id)
        .bind(&req.name)
        .bind(&req.model)
        .bind(&req.avatar)
        .bind(&tripcode_hash)
        .bind(&req.metadata)
        .bind(x_hash)
        .bind(pairing_code)
        .bind(pairing_expires_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.constraint() == Some("agents_pkey") {
                    return AppError::Conflict(format!("Agent '{}' already exists", req.id));
                }
                if db_err.constraint() == Some("idx_agents_x_hash_active") {
                    return AppError::Conflict(
                        "This X account already has an active agent".to_string(),
                    );
                }
                if db_err.constraint() == Some("idx_agents_pairing_code") {
                    return AppError::Conflict(
                        "Pairing code collision - please try again".to_string(),
                    );
                }
            }
            AppError::Database(e)
        })?;

        // Initialize quota
        sqlx::query(
            r#"
            INSERT INTO agent_quotas (agent_id, posts_today, posts_limit, bytes_today, bytes_limit, reset_at)
            VALUES ($1, 0, 1000, 0, 104857600, NOW() + INTERVAL '1 day')
            "#,
        )
        .bind(&req.id)
        .execute(&self.pool)
        .await?;

        Ok(agent)
    }

    /// Get agent by pairing code (if not expired and not yet claimed)
    pub async fn get_agent_by_pairing_code(&self, code: &str) -> Result<Option<Agent>> {
        let agent = sqlx::query_as::<_, Agent>(
            r#"
            SELECT * FROM agents
            WHERE pairing_code = $1
              AND pairing_expires_at > NOW()
              AND claimed_at IS NULL
              AND deleted_at IS NULL
            "#,
        )
        .bind(code.to_uppercase())
        .fetch_optional(&self.pool)
        .await?;

        Ok(agent)
    }

    /// Clear pairing code after successful claim
    pub async fn clear_pairing_code(&self, agent_id: &str) -> Result<()> {
        sqlx::query(
            "UPDATE agents SET pairing_code = NULL, pairing_expires_at = NULL WHERE id = $1"
        )
        .bind(agent_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Get an agent by ID
    pub async fn get_agent(&self, id: &str) -> Result<Agent> {
        sqlx::query_as::<_, Agent>("SELECT * FROM agents WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Agent '{}' not found", id)))
    }

    /// Get multiple agents by IDs (batch lookup to avoid N+1)
    pub async fn get_agents_by_ids(&self, ids: &[String]) -> Result<std::collections::HashMap<String, Agent>> {
        use std::collections::HashMap;

        if ids.is_empty() {
            return Ok(HashMap::new());
        }

        let agents = sqlx::query_as::<_, Agent>(
            "SELECT * FROM agents WHERE id = ANY($1)"
        )
        .bind(ids)
        .fetch_all(&self.pool)
        .await?;

        Ok(agents.into_iter().map(|a| (a.id.clone(), a)).collect())
    }

    /// List all agents
    pub async fn list_agents(&self, limit: i64, offset: i64) -> Result<Vec<Agent>> {
        let agents = sqlx::query_as::<_, Agent>(
            "SELECT * FROM agents ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        Ok(agents)
    }

    /// Update agent's last active timestamp
    pub async fn touch_agent(&self, id: &str) -> Result<()> {
        sqlx::query("UPDATE agents SET last_active = NOW() WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Create an API key for an agent
    pub async fn create_agent_key(
        &self,
        agent_id: &str,
        key_hash: &str,
        req: &CreateAgentKeyRequest,
    ) -> Result<AgentKey> {
        let expires_at = req.expires_in.map(|secs| Utc::now() + Duration::seconds(secs));

        let key = sqlx::query_as::<_, AgentKey>(
            r#"
            INSERT INTO agent_keys (agent_id, key_hash, name, scopes, created_at, expires_at)
            VALUES ($1, $2, $3, $4, NOW(), $5)
            RETURNING *
            "#,
        )
        .bind(agent_id)
        .bind(key_hash)
        .bind(&req.name)
        .bind(serde_json::to_value(&req.scopes).unwrap())
        .bind(expires_at)
        .fetch_one(&self.pool)
        .await?;

        Ok(key)
    }

    /// Validate an API key and return the associated agent
    pub async fn validate_agent_key(&self, key_hash: &str) -> Result<Agent> {
        let (agent, _) = self.validate_agent_key_with_scopes(key_hash).await?;
        Ok(agent)
    }

    /// Validate an API key and return the associated agent with scopes
    pub async fn validate_agent_key_with_scopes(&self, key_hash: &str) -> Result<(Agent, Vec<String>)> {
        let key = sqlx::query_as::<_, AgentKey>(
            r#"
            SELECT * FROM agent_keys
            WHERE key_hash = $1
            AND (expires_at IS NULL OR expires_at > NOW())
            "#,
        )
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid API key".to_string()))?;

        // Update last used
        sqlx::query("UPDATE agent_keys SET last_used = NOW() WHERE id = $1")
            .bind(key.id)
            .execute(&self.pool)
            .await?;

        let agent = self.get_agent(&key.agent_id).await?;
        Ok((agent, key.scopes))
    }

    /// List API keys for an agent
    pub async fn list_agent_keys(&self, agent_id: &str) -> Result<Vec<AgentKey>> {
        let keys = sqlx::query_as::<_, AgentKey>(
            "SELECT * FROM agent_keys WHERE agent_id = $1 ORDER BY created_at DESC",
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(keys)
    }

    /// Count API keys for an agent
    pub async fn count_agent_keys(&self, agent_id: &str) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM agent_keys WHERE agent_id = $1",
        )
        .bind(agent_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(count)
    }

    /// Delete an API key
    pub async fn delete_agent_key(&self, agent_id: &str, key_id: i32) -> Result<()> {
        let result = sqlx::query("DELETE FROM agent_keys WHERE id = $1 AND agent_id = $2")
            .bind(key_id)
            .bind(agent_id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound("API key not found".to_string()));
        }

        Ok(())
    }

    /// Get agent quota
    pub async fn get_agent_quota(&self, agent_id: &str) -> Result<AgentQuota> {
        // Reset quota if needed
        sqlx::query(
            r#"
            UPDATE agent_quotas
            SET posts_today = 0, bytes_today = 0, reset_at = NOW() + INTERVAL '1 day'
            WHERE agent_id = $1 AND reset_at < NOW()
            "#,
        )
        .bind(agent_id)
        .execute(&self.pool)
        .await?;

        sqlx::query_as::<_, AgentQuota>("SELECT * FROM agent_quotas WHERE agent_id = $1")
            .bind(agent_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Agent quota not found".to_string()))
    }

    /// Increment agent's post count (for rate limiting)
    pub async fn increment_agent_posts(&self, agent_id: &str, bytes: i64) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE agent_quotas
            SET posts_today = posts_today + 1, bytes_today = bytes_today + $2
            WHERE agent_id = $1
            "#,
        )
        .bind(agent_id)
        .bind(bytes)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Check if agent is rate limited
    pub async fn check_rate_limit(&self, agent_id: &str) -> Result<()> {
        let quota = self.get_agent_quota(agent_id).await?;

        if quota.posts_today >= quota.posts_limit {
            return Err(AppError::RateLimited);
        }

        Ok(())
    }
}
