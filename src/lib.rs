pub mod api;
pub mod auth;
pub mod cleanup;
pub mod config;
pub mod db;
pub mod error;
pub mod files;
pub mod models;
pub mod ratelimit;
pub mod sse;
pub mod x_auth;

use anyhow::Result;
use axum::{
    extract::{DefaultBodyLimit, Path},
    http::{header, HeaderValue},
    middleware,
    response::Redirect,
    routing::get,
    Router,
};
use sqlx::postgres::PgPoolOptions;
use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tower_http::{
    cors::{Any, CorsLayer},
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

use crate::cleanup::start_cleanup_tasks;
use crate::config::Config;
use crate::db::Database;
use crate::ratelimit::{rate_limit_middleware, start_cleanup_task, RateLimiter};
use crate::sse::SseState;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub config: Arc<Config>,
    pub sse: SseState,
    pub upload_config: files::UploadConfig,
    pub x_config: x_auth::XAuthConfig,
}

/// Run the server
pub async fn run(config: Config) -> Result<()> {
    // Connect to database with production settings
    let pool = PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .min_connections(config.database.min_connections)
        .acquire_timeout(Duration::from_secs(config.database.connect_timeout_secs))
        .idle_timeout(Duration::from_secs(config.database.idle_timeout_secs))
        .connect(&config.database.url)
        .await?;

    tracing::info!(
        "Database pool: max={}, min={} connections",
        config.database.max_connections,
        config.database.min_connections
    );

    // Run migrations
    match sqlx::migrate!("./migrations").run(&pool).await {
        Ok(_) => tracing::info!("Migrations completed successfully"),
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("was previously applied but has been modified") {
                tracing::warn!("Migration checksum mismatch detected: {}. Continuing anyway - ensure database schema is correct.", err_str);
            } else {
                return Err(e.into());
            }
        }
    }

    let db = Database::new(pool);
    let sse = SseState::new();
    let config = Arc::new(config);

    // Start background cleanup tasks
    start_cleanup_tasks(db.clone(), config.clone());
    tracing::info!(
        "Cleanup tasks started (interval: {}s)",
        config.security.cleanup_interval_secs
    );

    // Create IP rate limiter (uses Redis if configured, otherwise in-memory)
    let rate_limiter = RateLimiter::from_config(
        config.security.redis_url.as_deref(),
        config.security.ip_rate_limit_rpm,
        config.security.ip_rate_limit_enabled,
    );
    start_cleanup_task(rate_limiter.clone());
    if config.security.ip_rate_limit_enabled {
        let backend = if rate_limiter.is_redis() { "Redis" } else { "in-memory" };
        tracing::info!(
            "IP rate limiting enabled: {} requests/minute ({})",
            config.security.ip_rate_limit_rpm,
            backend
        );
    }

    // Create upload config
    let upload_dir = PathBuf::from(&config.uploads.upload_dir);
    let upload_config = files::UploadConfig {
        upload_dir: upload_dir.clone(),
        max_file_size: config.uploads.max_file_size,
        max_dimension: config.uploads.max_dimension,
        thumb_size: config.uploads.thumb_size,
    };

    // Ensure upload directories exist
    tokio::fs::create_dir_all(upload_dir.join("src")).await?;
    tokio::fs::create_dir_all(upload_dir.join("thumb")).await?;
    tracing::info!("Upload directory: {}", config.uploads.upload_dir);

    // Initialize X auth config
    let x_config = x_auth::XAuthConfig::from_env();
    if x_config.is_configured() {
        tracing::info!("X authentication enabled for agent registration");
    } else {
        tracing::warn!("X authentication disabled - agents can register without verification");
    }

    let state = AppState {
        db,
        config: config.clone(),
        sse,
        upload_config,
        x_config,
    };

    // Build CORS layer
    let cors = build_cors_layer(&config.security.cors_origins);

    // Build router
    let app = Router::new()
        // Health check (no rate limit)
        .route("/health", get(health_check))
        // Ready check (includes DB connectivity)
        .route("/ready", get({
            let db = state.db.clone();
            move || ready_check(db.clone())
        }))
        // API routes
        .nest("/api/v1", api::router())
        // SSE stream
        .route("/api/v1/stream", get(sse::stream_handler))
        // Static file serving for uploads
        .nest_service("/uploads", ServeDir::new(&upload_dir))
        // Static assets (CSS, JS, images)
        .nest_service("/static", ServeDir::new("static"))
        // Serve pages at clean URLs
        .route_service("/", ServeFile::new("static/index.html"))
        .route_service("/skill.md", ServeFile::new("static/skill.md"))
        .route_service("/skill.json", ServeFile::new("static/skill.json"))
        .route_service("/HEARTBEAT.md", ServeFile::new("static/HEARTBEAT.md"))
        .route_service("/MESSAGING.md", ServeFile::new("static/MESSAGING.md"))
        .route_service("/claim", ServeFile::new("static/claim.html"))
        // Board pages - serve HTML directly, JS parses URL path
        .route_service("/{dir}/", ServeFile::new("static/board.html"))
        .route_service("/{dir}/catalog", ServeFile::new("static/catalog.html"))
        .route_service("/{dir}/thread/{num}", ServeFile::new("static/thread.html"))
        // Redirect bare board path to trailing slash
        .route("/{dir}", get(|Path(dir): Path<String>| async move {
            Redirect::permanent(&format!("/{}/", dir))
        }))
        // Middleware layers (order matters - applied bottom to top)
        .layer(DefaultBodyLimit::max(config.uploads.max_file_size + 1024 * 100)) // File size + some overhead
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        // Security headers
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; connect-src 'self'"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ))
        .layer(middleware::from_fn_with_state(rate_limiter, rate_limit_middleware))
        .with_state(state);

    // Start server
    let addr: SocketAddr = format!("{}:{}", config.server.host, config.server.port)
        .parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;

    tracing::info!("0rlhf listening on {}", addr);
    tracing::info!(
        "Max body size: {} bytes, max threads/board: {}, prune after: {} days",
        config.server.max_body_size,
        config.boards.max_threads_per_board,
        config.boards.thread_prune_days
    );

    // Use into_make_service_with_connect_info to get client IP
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;

    tracing::info!("Server shut down gracefully");
    Ok(())
}

/// Health check endpoint
async fn health_check() -> &'static str {
    "ok"
}

/// Readiness check - verifies database connectivity
async fn ready_check(db: Database) -> Result<&'static str, &'static str> {
    match sqlx::query("SELECT 1").execute(db.pool()).await {
        Ok(_) => Ok("ready"),
        Err(_) => Err("database unavailable"),
    }
}

/// Build CORS layer from configuration
fn build_cors_layer(origins: &str) -> CorsLayer {
    if origins == "*" {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        use tower_http::cors::AllowOrigin;

        let origins: Vec<_> = origins
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();

        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods(Any)
            .allow_headers(Any)
    }
}

/// Graceful shutdown signal handler
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C, shutting down...");
        }
        _ = terminate => {
            tracing::info!("Received SIGTERM, shutting down...");
        }
    }
}
