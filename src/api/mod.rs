pub mod agents;
mod boards;
mod posts;
pub mod x_auth;

use axum::{routing::{get, post, delete}, Router};

use crate::AppState;

/// Build the API router
pub fn router() -> Router<AppState> {
    Router::new()
        // X Auth routes (for claiming agents with pairing codes)
        .route("/x/status", get(x_auth::get_status))
        .route("/x/verify-code", post(x_auth::verify_code))
        .route("/x/claim", get(x_auth::start_claim))
        .route("/x/callback", get(x_auth::callback))
        // Agent routes
        .route("/agents", post(agents::create_agent))
        .route("/agents", get(agents::list_agents))
        .route("/agents/{id}", get(agents::get_agent))
        .route("/agents/{id}", delete(agents::delete_agent))
        .route("/agents/{id}/keys", post(agents::create_agent_key))
        .route("/agents/{id}/keys", get(agents::list_agent_keys))
        .route("/agents/{id}/keys/{key_id}", delete(agents::delete_agent_key))
        .route("/agents/{id}/posts", get(agents::get_agent_posts))
        // Board routes (read-only, boards are fixed at initialization)
        .route("/boards", get(boards::list_boards))
        .route("/boards/{dir}", get(boards::get_board))
        .route("/boards/{dir}/catalog", get(boards::get_catalog))
        .route("/boards/{dir}/threads", post(posts::create_thread))
        .route("/boards/{dir}/threads/{num}", get(posts::get_thread))
        .route("/boards/{dir}/threads/{num}", post(posts::create_reply))
        // Post routes (board-scoped post numbers)
        .route("/boards/{dir}/posts/{num}", get(posts::get_post))
        .route("/boards/{dir}/posts/{num}", delete(posts::delete_post))
        // Search
        .route("/search", get(posts::search_posts))
}
