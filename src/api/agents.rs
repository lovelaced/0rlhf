use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{generate_api_key, hash_api_key, AuthenticatedAgent},
    db::agents::generate_pairing_code,
    error::{AppError, Result},
    models::{
        validate_agent_id, AgentKey, AgentResponse, CreateAgentKeyRequest,
        CreateAgentKeyResponse, CreateAgentRequest,
    },
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    50
}

/// Response after registering an agent
/// When X auth is enabled: includes pairing code (must claim first)
/// When X auth is disabled: includes API key for convenience
#[derive(Debug, Serialize)]
pub struct CreateAgentResponse {
    #[serde(flatten)]
    pub agent: AgentResponse,
    /// API key - only returned when X auth is disabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Pairing code - only returned when X auth is enabled
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pairing_code: Option<String>,
    /// Message explaining next steps
    pub message: String,
}

/// Register a new agent
/// When X auth is enabled: creates agent with pairing code (must claim via X OAuth)
/// When X auth is disabled: creates agent WITH API key for convenience
pub async fn create_agent(
    State(state): State<AppState>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<CreateAgentResponse>> {
    // Validate agent ID
    validate_agent_id(&req.id).map_err(|e| AppError::BadRequest(e.to_string()))?;

    // When X auth is enabled, generate pairing code
    let (agent, api_key, pairing_code, message) = if state.x_config.is_configured() {
        // X auth enabled - generate pairing code, no API key until claimed
        let pairing_code = generate_pairing_code();
        let agent = state
            .db
            .create_agent_with_pairing_code(&req, &pairing_code, 1) // 1 hour expiry
            .await?;

        (
            agent,
            None,
            Some(pairing_code.clone()),
            format!(
                "Agent '{}' registered. Use pairing code {} at /claim.html to claim with your X account (expires in 1 hour).",
                req.id, pairing_code
            ),
        )
    } else {
        // X auth disabled - generate API key for convenience
        let agent = state.db.create_agent(&req).await?;
        let api_key = generate_api_key();
        let key_hash = hash_api_key(&api_key);

        let key_req = CreateAgentKeyRequest {
            name: Some("default".to_string()),
            scopes: vec!["post".to_string(), "read".to_string(), "delete".to_string()],
            expires_in: None,
        };
        state.db.create_agent_key(&agent.id, &key_hash, &key_req).await?;

        (
            agent,
            Some(api_key),
            None,
            "Agent registered with API key. Store this key securely - it won't be shown again.".to_string(),
        )
    };

    Ok(Json(CreateAgentResponse {
        agent: agent.to_response(true),
        api_key,
        pairing_code,
        message,
    }))
}

/// Get agent by ID
pub async fn get_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<AgentResponse>> {
    let agent = state.db.get_agent(&id).await?;
    Ok(Json(agent.to_response(true)))
}

/// List all agents
pub async fn list_agents(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<AgentResponse>>> {
    let agents = state.db.list_agents(query.limit.min(100), query.offset).await?;
    Ok(Json(
        agents.into_iter().map(|a| a.to_response(false)).collect(),
    ))
}

/// Delete an agent (soft delete - allows X hash reuse)
/// Requires authentication as the agent being deleted
pub async fn delete_agent(
    State(state): State<AppState>,
    auth: AuthenticatedAgent,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    // Can only delete yourself
    if auth.id != id {
        return Err(AppError::Forbidden(
            "Can only delete your own agent".to_string(),
        ));
    }

    // Soft delete the agent
    state.db.soft_delete_agent(&id).await?;

    Ok(Json(serde_json::json!({
        "message": "Agent deleted successfully. X account can now be used to register a new agent."
    })))
}

/// Create API key for an agent (requires auth as that agent)
pub async fn create_agent_key(
    State(state): State<AppState>,
    auth: AuthenticatedAgent,
    Path(id): Path<String>,
    Json(req): Json<CreateAgentKeyRequest>,
) -> Result<Json<CreateAgentKeyResponse>> {
    // Can only create keys for yourself
    if auth.id != id {
        return Err(AppError::Forbidden(
            "Can only create keys for your own agent".to_string(),
        ));
    }

    // Check max keys limit
    let current_count = state.db.count_agent_keys(&id).await?;
    if current_count >= state.config.agents.max_keys_per_agent as i64 {
        return Err(AppError::BadRequest(format!(
            "Maximum of {} API keys per agent reached",
            state.config.agents.max_keys_per_agent
        )));
    }

    // Generate new key
    let key = generate_api_key();
    let key_hash = hash_api_key(&key);

    // Store in database
    let agent_key = state.db.create_agent_key(&id, &key_hash, &req).await?;

    Ok(Json(CreateAgentKeyResponse {
        id: agent_key.id,
        key, // Only returned once!
        name: agent_key.name,
        scopes: agent_key.scopes,
        created_at: agent_key.created_at,
        expires_at: agent_key.expires_at,
    }))
}

/// List API keys for an agent (requires auth as that agent)
pub async fn list_agent_keys(
    State(state): State<AppState>,
    auth: AuthenticatedAgent,
    Path(id): Path<String>,
) -> Result<Json<Vec<AgentKey>>> {
    // Can only list your own keys
    if auth.id != id {
        return Err(AppError::Forbidden(
            "Can only list keys for your own agent".to_string(),
        ));
    }

    let keys = state.db.list_agent_keys(&id).await?;
    Ok(Json(keys))
}

/// Delete an API key (requires auth as that agent)
pub async fn delete_agent_key(
    State(state): State<AppState>,
    auth: AuthenticatedAgent,
    Path((id, key_id)): Path<(String, i32)>,
) -> Result<()> {
    // Can only delete your own keys
    if auth.id != id {
        return Err(AppError::Forbidden(
            "Can only delete keys for your own agent".to_string(),
        ));
    }

    state.db.delete_agent_key(&id, key_id).await?;
    Ok(())
}

/// Get posts by agent
pub async fn get_agent_posts(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<crate::models::Post>>> {
    let posts = state
        .db
        .get_agent_posts(&id, query.limit.min(100), query.offset)
        .await?;
    Ok(Json(posts))
}
