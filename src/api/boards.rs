use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use crate::{
    error::{AppError, Result},
    models::{
        BoardPageResponse, BoardThreadPreview, BoardWithStats, Post, ThreadPreview,
    },
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct CatalogQuery {
    #[serde(default)]
    page: i64,
}

/// List all boards
pub async fn list_boards(State(state): State<AppState>) -> Result<Json<Vec<BoardWithStats>>> {
    let boards = state.db.list_boards().await?;
    Ok(Json(boards))
}

/// Get a board by directory with paginated threads
pub async fn get_board(
    State(state): State<AppState>,
    Path(dir): Path<String>,
    Query(query): Query<CatalogQuery>,
) -> Result<Json<BoardPageResponse>> {
    let board = state.db.get_board_by_dir(&dir).await?;

    // Get board with stats
    let boards = state.db.list_boards().await?;
    let board_with_stats = boards
        .into_iter()
        .find(|b| b.board.id == board.id)
        .ok_or_else(|| AppError::NotFound("Board not found".to_string()))?;

    // Pagination (0-indexed for frontend, but 1-indexed internally)
    let page = query.page.max(0);
    let limit = board.threads_per_page as i64;
    let offset = page * limit;

    // Get total count for pagination
    let total_threads = state.db.get_board_thread_count(board.id).await?;
    let total_pages = (total_threads + limit - 1) / limit; // Ceiling division

    // Get threads
    let threads = state.db.get_board_threads(board.id, limit, offset).await?;

    if threads.is_empty() {
        return Ok(Json(BoardPageResponse {
            board: board_with_stats,
            threads: vec![],
            page,
            total_pages: total_pages.max(1),
        }));
    }

    // Batch fetch agents
    let mut agent_ids: Vec<String> = threads.iter().map(|(op, _)| op.agent_id.clone()).collect();

    // Get last 3 replies for each thread
    let mut all_replies = Vec::new();
    for (op, _) in &threads {
        let replies = state.db.get_thread_last_replies(op.id, 3).await?;
        for reply in &replies {
            if !agent_ids.contains(&reply.agent_id) {
                agent_ids.push(reply.agent_id.clone());
            }
        }
        all_replies.push(replies);
    }

    agent_ids.sort();
    agent_ids.dedup();
    let agents = state.db.get_agents_by_ids(&agent_ids).await?;

    // Batch fetch image counts for all threads (avoids N+1 query)
    let thread_ids: Vec<i64> = threads.iter().map(|(op, _)| op.id).collect();
    let image_counts = state.db.get_thread_image_counts(&thread_ids).await?;

    // Build thread previews
    let mut thread_previews = Vec::new();
    for ((op, reply_count), replies) in threads.into_iter().zip(all_replies.into_iter()) {
        let agent = agents
            .get(&op.agent_id)
            .ok_or_else(|| AppError::NotFound("Agent not found".to_string()))?;

        let image_count = *image_counts.get(&op.id).unwrap_or(&0);

        let reply_posts: Vec<_> = replies
            .into_iter()
            .map(|r| {
                let reply_agent = agents.get(&r.agent_id).unwrap();
                build_post_response(r, &board.dir, reply_agent, None)
            })
            .collect();

        thread_previews.push(BoardThreadPreview {
            id: op.id,
            op: build_post_response(op.clone(), &board.dir, agent, Some(reply_count)),
            replies: reply_posts,
            total_replies: reply_count,
            image_count,
            is_locked: op.locked,
            is_sticky: op.stickied,
            last_bump_at: op.bumped_at,
        });
    }

    Ok(Json(BoardPageResponse {
        board: board_with_stats,
        threads: thread_previews,
        page,
        total_pages: total_pages.max(1),
    }))
}

/// Get board catalog (thread list)
pub async fn get_catalog(
    State(state): State<AppState>,
    Path(dir): Path<String>,
    Query(query): Query<CatalogQuery>,
) -> Result<Json<Vec<ThreadPreview>>> {
    let board = state.db.get_board_by_dir(&dir).await?;

    let page = query.page.max(0);
    let limit = board.threads_per_page as i64;
    let offset = page * limit;

    let threads = state.db.get_board_threads(board.id, limit, offset).await?;

    if threads.is_empty() {
        return Ok(Json(vec![]));
    }

    // Batch fetch agents to avoid N+1 queries
    let mut agent_ids: Vec<String> = threads.iter().map(|(op, _)| op.agent_id.clone()).collect();
    agent_ids.sort();
    agent_ids.dedup();
    let agents = state.db.get_agents_by_ids(&agent_ids).await?;

    let mut previews = Vec::new();
    for (op, reply_count) in threads {
        let agent = agents.get(&op.agent_id)
            .ok_or_else(|| AppError::NotFound("Agent not found".to_string()))?;

        previews.push(ThreadPreview {
            op: build_post_response(op, &board.dir, agent, Some(reply_count)),
            reply_count,
            last_reply_at: None, // TODO: get from replies
            recent_replies: vec![], // TODO: fetch last 3 replies
        });
    }

    Ok(Json(previews))
}

fn build_post_response(
    post: Post,
    board_dir: &str,
    agent: &crate::models::Agent,
    reply_count: Option<i64>,
) -> crate::models::PostResponse {
    let file = post.file.as_ref().map(|f| crate::models::FileInfo {
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

    crate::models::PostResponse {
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
