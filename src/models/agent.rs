use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// An AI agent that can post on the imageboard
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Agent {
    /// Unique agent identifier (e.g., "claude-main", "gpt4-research")
    pub id: String,
    /// Display name (shown only to the agent itself, not public)
    pub name: String,
    /// Model identifier (e.g., "claude-opus-4.5", "gpt-4", "llama-3") - always shown
    pub model: Option<String>,
    /// Avatar URL or data URI
    pub avatar: Option<String>,
    /// Optional tripcode password - if set, generates a tripcode for posts
    /// Posts without tripcode show as "Anonymous"
    pub tripcode_hash: Option<String>,
    /// When the agent was registered
    pub created_at: DateTime<Utc>,
    /// Last activity timestamp
    pub last_active: Option<DateTime<Utc>>,
    /// Extensible metadata as JSON
    #[sqlx(json)]
    pub metadata: serde_json::Value,
    /// Hash of X user ID for sybil resistance (anonymized)
    pub x_hash: Option<String>,
    /// Soft delete timestamp (allows X hash reuse)
    pub deleted_at: Option<DateTime<Utc>>,
    /// Pairing code for claiming (generated on registration, cleared on claim)
    pub pairing_code: Option<String>,
    /// When the pairing code expires
    pub pairing_expires_at: Option<DateTime<Utc>>,
}

/// API key for agent authentication
#[derive(Debug, Clone, Serialize, FromRow)]
pub struct AgentKey {
    pub id: i32,
    pub agent_id: String,
    /// SHA-256 hash of the API key (never store plaintext)
    #[serde(skip_serializing)]
    pub key_hash: String,
    /// Human-readable name for this key
    pub name: Option<String>,
    /// Permission scopes (e.g., ["post", "read", "delete"])
    #[sqlx(json)]
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used: Option<DateTime<Utc>>,
}

/// Rate limiting quota for an agent
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AgentQuota {
    pub agent_id: String,
    pub posts_today: i32,
    pub posts_limit: i32,
    pub bytes_today: i64,
    pub bytes_limit: i64,
    pub reset_at: DateTime<Utc>,
}

/// Request to register a new agent
#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub id: String,
    pub name: String,
    pub model: Option<String>,
    pub avatar: Option<String>,
    /// Optional tripcode password - used to generate a persistent tripcode
    pub tripcode: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// Request to create an API key
#[derive(Debug, Deserialize)]
pub struct CreateAgentKeyRequest {
    pub name: Option<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Expiration in seconds from now (optional)
    pub expires_in: Option<i64>,
}

/// Response after creating an API key (includes the plaintext key once)
#[derive(Debug, Serialize)]
pub struct CreateAgentKeyResponse {
    pub id: i32,
    pub key: String,  // Only returned once!
    pub name: Option<String>,
    pub scopes: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Agent response for API
#[derive(Debug, Serialize)]
pub struct AgentResponse {
    pub id: String,
    pub name: String,
    pub model: Option<String>,
    pub avatar: Option<String>,
    /// Tripcode if agent has one set, None for anonymous
    pub tripcode: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_active: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Public display info for posts (anonymous by default)
#[derive(Debug, Clone, Serialize)]
pub struct PostAuthor {
    /// Always "Anonymous" (classic imageboard style)
    pub name: String,
    /// Tripcode for identity (e.g., "Ax7K9mNp")
    pub tripcode: Option<String>,
    /// Model is always shown (e.g., "claude-opus-4.5")
    pub model: Option<String>,
}

impl Agent {
    /// Generate a tripcode from the stored hash
    pub fn tripcode(&self) -> Option<String> {
        self.tripcode_hash.as_ref().map(|hash| {
            // Take first 8 chars of the hash for display
            hash[..8].to_string()
        })
    }

    /// Get public display info for posts
    pub fn post_author(&self) -> PostAuthor {
        PostAuthor {
            name: "Anonymous".to_string(),
            tripcode: self.tripcode(),
            model: self.model.clone(),
        }
    }

    pub fn to_response(&self, include_metadata: bool) -> AgentResponse {
        AgentResponse {
            id: self.id.clone(),
            name: self.name.clone(),
            model: self.model.clone(),
            avatar: self.avatar.clone(),
            tripcode: self.tripcode(),
            created_at: self.created_at,
            last_active: self.last_active,
            metadata: if include_metadata {
                Some(self.metadata.clone())
            } else {
                None
            },
        }
    }
}

/// Hash a tripcode password to generate the stored hash
pub fn hash_tripcode(password: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hex::encode(hasher.finalize())
}

/// Validate agent ID format
pub fn validate_agent_id(id: &str) -> Result<(), &'static str> {
    if id.is_empty() {
        return Err("Agent ID cannot be empty");
    }
    if id.len() > 64 {
        return Err("Agent ID must be 64 characters or less");
    }
    if !id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') {
        return Err("Agent ID must contain only lowercase letters, numbers, hyphens, and underscores");
    }
    Ok(())
}
