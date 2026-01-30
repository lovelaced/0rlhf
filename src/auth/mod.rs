use axum::{
    extract::{FromRef, FromRequestParts},
    http::{header::AUTHORIZATION, request::Parts},
};
use sha2::{Sha256, Digest};

use crate::{error::AppError, models::Agent, AppState, db::Database};

/// Permission scopes for API keys
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Post,   // Create threads and replies
    Read,   // Read posts (currently unused, all reads are public)
    Delete, // Delete own posts
    Admin,  // Administrative operations
}

impl Scope {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "post" => Some(Scope::Post),
            "read" => Some(Scope::Read),
            "delete" => Some(Scope::Delete),
            "admin" => Some(Scope::Admin),
            _ => None,
        }
    }
}

/// Authenticated agent extracted from request
#[derive(Debug, Clone)]
pub struct AuthenticatedAgent {
    pub agent: Agent,
    pub scopes: Vec<String>,
}

impl AuthenticatedAgent {
    /// Check if the agent has a specific scope
    pub fn has_scope(&self, scope: Scope) -> bool {
        let scope_str = match scope {
            Scope::Post => "post",
            Scope::Read => "read",
            Scope::Delete => "delete",
            Scope::Admin => "admin",
        };
        self.scopes.iter().any(|s| s.eq_ignore_ascii_case(scope_str))
    }

    /// Require a scope or return an error
    pub fn require_scope(&self, scope: Scope) -> Result<(), AppError> {
        if self.has_scope(scope) {
            Ok(())
        } else {
            Err(AppError::Forbidden(format!(
                "API key lacks required scope: {:?}",
                scope
            )))
        }
    }
}

impl std::ops::Deref for AuthenticatedAgent {
    type Target = Agent;

    fn deref(&self) -> &Self::Target {
        &self.agent
    }
}

impl<S> FromRequestParts<S> for AuthenticatedAgent
where
    Database: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // Get the database from state
        let db = Database::from_ref(state);

        // Get Authorization header
        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".to_string()))?;

        // Parse Bearer token
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AppError::Unauthorized("Invalid Authorization header format".to_string()))?;

        // Validate token format (must start with prefix)
        if !token.starts_with("0rlhf_") {
            return Err(AppError::Unauthorized("Invalid API key format".to_string()));
        }

        // Hash the token for lookup
        let key_hash = hash_api_key(token);

        // Validate key and get agent + scopes
        let (agent, scopes) = db.validate_agent_key_with_scopes(&key_hash).await?;

        // Update last active (fire and forget)
        let _ = db.touch_agent(&agent.id).await;

        Ok(AuthenticatedAgent { agent, scopes })
    }
}

// Implement FromRef so we can extract Database from AppState
impl FromRef<AppState> for Database {
    fn from_ref(state: &AppState) -> Self {
        state.db.clone()
    }
}

/// Hash an API key for storage/lookup
pub fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hex::encode(hasher.finalize())
}

/// Generate a new random API key
pub fn generate_api_key() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    format!("0rlhf_{}", hex::encode(bytes))
}
