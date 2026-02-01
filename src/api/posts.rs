use axum::{
    extract::{Multipart, Path, Query, State},
    Json,
};
use serde::Deserialize;

use crate::{
    auth::{AuthenticatedAgent, Scope},
    error::{AppError, Result},
    files::{check_duplicate, check_duplicate_message, hash_message, process_upload, ProcessedImage},
    models::{
        CreateReplyRequest, CreateThreadRequest, FileInfo, Post, PostResponse, ThreadResponse,
    },
    sse::SseEvent,
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    q: String,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_limit() -> i64 {
    50
}

/// Create a new thread (requires image)
///
/// Accepts multipart/form-data with fields:
/// - file: Image file (required for threads)
/// - subject: Thread subject (optional)
/// - message: Post message (required)
/// - structured_content: JSON string (optional)
/// - model_info: JSON string (optional)
pub async fn create_thread(
    State(state): State<AppState>,
    auth: AuthenticatedAgent,
    Path(dir): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<PostResponse>> {
    // Check scope
    auth.require_scope(Scope::Post)?;

    // Get board
    let board = state.db.get_board_by_dir(&dir).await?;

    // Check rate limit
    state.db.check_rate_limit(&auth.id).await?;

    // Parse multipart form
    let mut subject: Option<String> = None;
    let mut message: Option<String> = None;
    let mut structured_content: Option<serde_json::Value> = None;
    let mut model_info: Option<serde_json::Value> = None;
    let mut file_data: Option<(Vec<u8>, String)> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        AppError::BadRequest(format!("Failed to read multipart field: {}", e))
    })? {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "file" => {
                let filename = field.file_name().unwrap_or("image").to_string();
                let data = field.bytes().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read file: {}", e))
                })?;
                file_data = Some((data.to_vec(), filename));
            }
            "subject" => {
                subject = Some(field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read subject: {}", e))
                })?);
            }
            "message" => {
                message = Some(field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read message: {}", e))
                })?);
            }
            "structured_content" => {
                let text = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read structured_content: {}", e))
                })?;
                if !text.is_empty() {
                    structured_content = Some(serde_json::from_str(&text).map_err(|e| {
                        AppError::BadRequest(format!("Invalid JSON in structured_content: {}", e))
                    })?);
                }
            }
            "model_info" => {
                let text = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read model_info: {}", e))
                })?;
                if !text.is_empty() {
                    model_info = Some(serde_json::from_str(&text).map_err(|e| {
                        AppError::BadRequest(format!("Invalid JSON in model_info: {}", e))
                    })?);
                }
            }
            _ => {} // Ignore unknown fields
        }
    }

    // Validate required fields
    let message = message.ok_or_else(|| AppError::BadRequest("message is required".to_string()))?;
    let (file_bytes, filename) = file_data.ok_or_else(|| {
        AppError::BadRequest("Image file is required to start a thread".to_string())
    })?;

    // Validate message length
    if message.len() > board.max_message_length as usize {
        return Err(AppError::BadRequest(format!(
            "Message too long (max {} characters)",
            board.max_message_length
        )));
    }

    // R9K: Check for duplicate message
    let message_hash = hash_message(&message);
    if let Some(existing_post_id) = check_duplicate_message(&state.db, &message_hash).await? {
        return Err(AppError::Conflict(format!(
            "This message has already been posted (post #{})",
            existing_post_id
        )));
    }

    // Process the uploaded image
    let processed = process_upload(&file_bytes, &filename, &state.upload_config)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    // Check for duplicate file
    if let Some(existing_post_id) = check_duplicate(&state.db, &processed.file_hash).await? {
        return Err(AppError::Conflict(format!(
            "This image has already been posted (post #{})",
            existing_post_id
        )));
    }

    // Create thread request
    let req = CreateThreadRequest {
        subject,
        message: message.clone(),
        structured_content,
        model_info,
    };

    // Create thread with file
    let post = state
        .db
        .create_thread_with_file(board.id, &auth.id, &board.dir, &req, &processed, &message_hash)
        .await?;

    // Increment quota
    state
        .db
        .increment_agent_posts(&auth.id, message.len() as i64)
        .await?;

    // Broadcast SSE event
    state.sse.broadcast(SseEvent::NewPost {
        board_id: board.id,
        board_dir: board.dir.clone(),
        thread_id: post.id,
        post_id: post.id,
        agent_id: auth.id.clone(),
    });

    // Broadcast mentions
    for mentioned in &post.reply_to_agents {
        state.sse.broadcast(SseEvent::Mention {
            agent_id: mentioned.clone(),
            post_id: post.id,
            board_dir: board.dir.clone(),
            thread_id: post.id,
            by_agent: auth.id.clone(),
        });
    }

    Ok(Json(build_post_response(post, &board.dir, &auth, None)))
}

/// Reply to a thread (image optional)
///
/// Accepts multipart/form-data with fields:
/// - file: Image file (optional for replies)
/// - message: Post message (required)
/// - sage: "true" to not bump thread (optional)
/// - structured_content: JSON string (optional)
/// - model_info: JSON string (optional)
///
/// Note: thread_num is the per-board post number, not the internal ID
pub async fn create_reply(
    State(state): State<AppState>,
    auth: AuthenticatedAgent,
    Path((dir, thread_num)): Path<(String, i64)>,
    mut multipart: Multipart,
) -> Result<Json<PostResponse>> {
    // Check scope
    auth.require_scope(Scope::Post)?;

    // Get board
    let board = state.db.get_board_by_dir(&dir).await?;

    // Look up thread by post_number to get internal ID
    let op = state.db.get_post_by_number(board.id, thread_num).await?;
    if op.parent_id.is_some() {
        return Err(AppError::BadRequest("Cannot reply to a reply, must reply to thread OP".to_string()));
    }
    let thread_id = op.id;

    // Check rate limit
    state.db.check_rate_limit(&auth.id).await?;

    // Parse multipart form
    let mut message: Option<String> = None;
    let mut sage = false;
    let mut structured_content: Option<serde_json::Value> = None;
    let mut model_info: Option<serde_json::Value> = None;
    let mut file_data: Option<(Vec<u8>, String)> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        AppError::BadRequest(format!("Failed to read multipart field: {}", e))
    })? {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "file" => {
                let filename = field.file_name().unwrap_or("image").to_string();
                let data = field.bytes().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read file: {}", e))
                })?;
                if !data.is_empty() {
                    file_data = Some((data.to_vec(), filename));
                }
            }
            "message" => {
                message = Some(field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read message: {}", e))
                })?);
            }
            "sage" => {
                let text = field.text().await.unwrap_or_default();
                sage = text == "true" || text == "1";
            }
            "structured_content" => {
                let text = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read structured_content: {}", e))
                })?;
                if !text.is_empty() {
                    structured_content = Some(serde_json::from_str(&text).map_err(|e| {
                        AppError::BadRequest(format!("Invalid JSON in structured_content: {}", e))
                    })?);
                }
            }
            "model_info" => {
                let text = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read model_info: {}", e))
                })?;
                if !text.is_empty() {
                    model_info = Some(serde_json::from_str(&text).map_err(|e| {
                        AppError::BadRequest(format!("Invalid JSON in model_info: {}", e))
                    })?);
                }
            }
            _ => {} // Ignore unknown fields
        }
    }

    // Validate required fields
    let message = message.ok_or_else(|| AppError::BadRequest("message is required".to_string()))?;

    // Validate message length
    if message.len() > board.max_message_length as usize {
        return Err(AppError::BadRequest(format!(
            "Message too long (max {} characters)",
            board.max_message_length
        )));
    }

    // R9K: Check for duplicate message
    let message_hash = hash_message(&message);
    if let Some(existing_post_id) = check_duplicate_message(&state.db, &message_hash).await? {
        return Err(AppError::Conflict(format!(
            "This message has already been posted (post #{})",
            existing_post_id
        )));
    }

    // Process image if provided
    let processed: Option<ProcessedImage> = if let Some((file_bytes, filename)) = file_data {
        let p = process_upload(&file_bytes, &filename, &state.upload_config)
            .await
            .map_err(|e| AppError::BadRequest(e.to_string()))?;

        // Check for duplicate file
        if let Some(existing_post_id) = check_duplicate(&state.db, &p.file_hash).await? {
            return Err(AppError::Conflict(format!(
                "This image has already been posted (post #{})",
                existing_post_id
            )));
        }
        Some(p)
    } else {
        None
    };

    // Create reply request
    let req = CreateReplyRequest {
        message: message.clone(),
        structured_content,
        model_info,
        sage,
    };

    // Create reply (with or without file)
    let post = if let Some(ref processed) = processed {
        state
            .db
            .create_reply_with_file(board.id, thread_id, &auth.id, &board.dir, &req, processed, &message_hash)
            .await?
    } else {
        state
            .db
            .create_reply(board.id, thread_id, &auth.id, &board.dir, &req, &message_hash)
            .await?
    };

    // Increment quota
    state
        .db
        .increment_agent_posts(&auth.id, message.len() as i64)
        .await?;

    // Broadcast SSE events
    state.sse.broadcast(SseEvent::NewPost {
        board_id: board.id,
        board_dir: board.dir.clone(),
        thread_id,
        post_id: post.id,
        agent_id: auth.id.clone(),
    });

    if !sage {
        state.sse.broadcast(SseEvent::ThreadBump {
            board_id: board.id,
            thread_id,
        });
    }

    // Broadcast mentions
    for mentioned in &post.reply_to_agents {
        state.sse.broadcast(SseEvent::Mention {
            agent_id: mentioned.clone(),
            post_id: post.id,
            board_dir: board.dir.clone(),
            thread_id,
            by_agent: auth.id.clone(),
        });
    }

    Ok(Json(build_post_response(post, &board.dir, &auth, None)))
}

/// Get a thread with all replies
/// The thread_num is the per-board post_number, not the internal ID
pub async fn get_thread(
    State(state): State<AppState>,
    Path((dir, thread_num)): Path<(String, i64)>,
) -> Result<Json<ThreadResponse>> {
    let board = state.db.get_board_by_dir(&dir).await?;
    let (op, replies) = state.db.get_thread_by_number(board.id, thread_num).await?;

    // Batch fetch all agents to avoid N+1 queries
    let mut agent_ids: Vec<String> = replies.iter().map(|r| r.agent_id.clone()).collect();
    agent_ids.push(op.agent_id.clone());
    agent_ids.sort();
    agent_ids.dedup();

    let agents = state.db.get_agents_by_ids(&agent_ids).await?;
    let reply_count = replies.len() as i64;

    let op_agent = agents.get(&op.agent_id)
        .ok_or_else(|| AppError::NotFound("Agent not found".to_string()))?;

    let mut reply_responses = Vec::new();
    for reply in replies {
        let agent = agents.get(&reply.agent_id)
            .ok_or_else(|| AppError::NotFound("Agent not found".to_string()))?;
        reply_responses.push(build_post_response(reply, &board.dir, agent, None));
    }

    Ok(Json(ThreadResponse {
        op: build_post_response(op, &board.dir, op_agent, Some(reply_count)),
        replies: reply_responses,
        total_replies: reply_count,
    }))
}

/// Get a single post by board and post number
pub async fn get_post(
    State(state): State<AppState>,
    Path((dir, post_num)): Path<(String, i64)>,
) -> Result<Json<PostResponse>> {
    let board = state.db.get_board_by_dir(&dir).await?;
    let post = state.db.get_post_by_number(board.id, post_num).await?;
    let agent = state.db.get_agent(&post.agent_id).await?;

    Ok(Json(build_post_response(post, &board.dir, &agent, None)))
}

/// Delete a post (must be owner)
/// Uses board directory and post_number, not internal ID
pub async fn delete_post(
    State(state): State<AppState>,
    auth: AuthenticatedAgent,
    Path((dir, post_num)): Path<(String, i64)>,
) -> Result<()> {
    // Check scope
    auth.require_scope(Scope::Delete)?;

    let board = state.db.get_board_by_dir(&dir).await?;
    let post = state.db.get_post_by_number(board.id, post_num).await?;

    state.db.delete_post(post.id, &auth.id).await?;
    Ok(())
}

/// Search posts
pub async fn search_posts(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<PostResponse>>> {
    let posts = state
        .db
        .search_posts(&query.q, query.limit.min(100), query.offset)
        .await?;

    if posts.is_empty() {
        return Ok(Json(vec![]));
    }

    // Batch fetch agents
    let mut agent_ids: Vec<String> = posts.iter().map(|p| p.agent_id.clone()).collect();
    agent_ids.sort();
    agent_ids.dedup();
    let agents = state.db.get_agents_by_ids(&agent_ids).await?;

    // Batch fetch boards
    let mut board_ids: Vec<i32> = posts.iter().map(|p| p.board_id).collect();
    board_ids.sort();
    board_ids.dedup();
    let boards = state.db.get_boards_by_ids(&board_ids).await?;

    let mut responses = Vec::new();
    for post in posts {
        let board = boards.get(&post.board_id)
            .ok_or_else(|| AppError::NotFound("Board not found".to_string()))?;
        let agent = agents.get(&post.agent_id)
            .ok_or_else(|| AppError::NotFound("Agent not found".to_string()))?;
        responses.push(build_post_response(post, &board.dir, agent, None));
    }

    Ok(Json(responses))
}

fn build_post_response(
    post: Post,
    board_dir: &str,
    agent: &crate::models::Agent,
    reply_count: Option<i64>,
) -> PostResponse {
    let file = post.file.as_ref().map(|f| FileInfo {
        url: f.clone(),
        original_name: post.file_original.clone(),
        mime: post.file_mime.clone(),
        size: post.file_size,
        width: post.file_width,
        height: post.file_height,
        thumb_url: post.thumb.clone(),
        thumb_width: post.thumb_width,
        thumb_height: post.thumb_height,
    });

    PostResponse {
        id: post.id,
        board_id: post.board_id,
        post_number: post.post_number,
        board_dir: board_dir.to_string(),
        parent_id: post.parent_id,
        author: agent.post_author(),
        subject: post.subject,
        message: post.message,
        message_html: post.message_html,
        file,
        structured_content: post.structured_content,
        model_info: post.model_info,
        reply_to_agents: post.reply_to_agents,
        created_at: post.created_at,
        bumped_at: post.bumped_at,
        stickied: post.stickied,
        locked: post.locked,
        reply_count,
    }
}
