//! IP-based rate limiting middleware
//!
//! Supports two backends:
//! - In-memory: Uses a sliding window algorithm with HashMap storage (single instance)
//! - Redis: Uses Redis INCR/EXPIRE for distributed rate limiting (multi-instance)
//!
//! Configure via REDIS_URL environment variable to use Redis backend.

use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

/// Rate limiter that supports both in-memory and Redis backends
#[derive(Clone)]
pub struct RateLimiter {
    inner: RateLimiterInner,
    /// Requests allowed per window
    limit: u32,
    /// Window duration in seconds
    window_secs: u64,
    /// Whether rate limiting is enabled
    enabled: bool,
}

#[derive(Clone)]
enum RateLimiterInner {
    /// In-memory rate limiting (single instance only)
    Memory {
        requests: Arc<RwLock<HashMap<IpAddr, Vec<Instant>>>>,
    },
    /// Redis-backed rate limiting (distributed)
    Redis {
        conn: redis::aio::MultiplexedConnection,
    },
}

impl RateLimiter {
    /// Create a new in-memory rate limiter
    pub fn new_memory(requests_per_minute: u32, enabled: bool) -> Self {
        Self {
            inner: RateLimiterInner::Memory {
                requests: Arc::new(RwLock::new(HashMap::new())),
            },
            limit: requests_per_minute,
            window_secs: 60,
            enabled,
        }
    }

    /// Create a new Redis-backed rate limiter
    pub async fn new_redis(redis_url: &str, requests_per_minute: u32, enabled: bool) -> Result<Self, redis::RedisError> {
        let client = redis::Client::open(redis_url)?;
        let conn = client.get_multiplexed_async_connection().await?;
        Ok(Self {
            inner: RateLimiterInner::Redis { conn },
            limit: requests_per_minute,
            window_secs: 60,
            enabled,
        })
    }

    /// Create rate limiter from configuration
    /// Uses Redis if REDIS_URL is configured, otherwise falls back to in-memory
    pub async fn from_config(
        redis_url: Option<&str>,
        requests_per_minute: u32,
        enabled: bool,
    ) -> Self {
        if let Some(url) = redis_url {
            match tokio::time::timeout(
                Duration::from_secs(5),
                Self::new_redis(url, requests_per_minute, enabled)
            ).await {
                Ok(Ok(limiter)) => {
                    tracing::info!("Using Redis-backed rate limiting");
                    return limiter;
                }
                Ok(Err(e)) => {
                    tracing::warn!("Redis connection failed: {}. Falling back to in-memory.", e);
                }
                Err(_) => {
                    tracing::warn!("Redis connection timed out. Falling back to in-memory.");
                }
            }
        }
        tracing::info!("Using in-memory rate limiting (single instance only)");
        Self::new_memory(requests_per_minute, enabled)
    }

    /// Check if a request is allowed and record it
    pub async fn check_and_record(&self, ip: IpAddr) -> bool {
        if !self.enabled {
            return true;
        }

        match &self.inner {
            RateLimiterInner::Memory { requests } => {
                self.check_and_record_memory(requests, ip).await
            }
            RateLimiterInner::Redis { conn } => {
                self.check_and_record_redis(conn.clone(), ip).await
            }
        }
    }

    async fn check_and_record_memory(
        &self,
        requests: &Arc<RwLock<HashMap<IpAddr, Vec<Instant>>>>,
        ip: IpAddr,
    ) -> bool {
        let now = Instant::now();
        let cutoff = now - Duration::from_secs(self.window_secs);

        let mut requests = requests.write().await;
        let timestamps = requests.entry(ip).or_insert_with(Vec::new);

        // Remove old timestamps outside the window
        timestamps.retain(|&t| t > cutoff);

        if timestamps.len() >= self.limit as usize {
            return false;
        }

        timestamps.push(now);
        true
    }

    async fn check_and_record_redis(&self, mut conn: redis::aio::MultiplexedConnection, ip: IpAddr) -> bool {
        let key = format!("ratelimit:ip:{}", ip);

        let result: Result<bool, redis::RedisError> = async {
            // Use pipeline to send INCR and EXPIRE in a single round-trip
            let mut pipe = redis::pipe();
            pipe.atomic()
                .cmd("INCR").arg(&key)
                .cmd("EXPIRE").arg(&key).arg(self.window_secs).ignore();

            let (count,): (u32,) = pipe.query_async(&mut conn).await?;

            Ok(count <= self.limit)
        }
        .await;

        match result {
            Ok(allowed) => allowed,
            Err(e) => {
                tracing::error!("Redis rate limit check failed: {}. Allowing request.", e);
                // Fail open: allow requests if Redis is unavailable
                true
            }
        }
    }

    /// Cleanup old entries (only needed for in-memory backend)
    pub async fn cleanup(&self) {
        if let RateLimiterInner::Memory { requests } = &self.inner {
            let cutoff = Instant::now() - Duration::from_secs(self.window_secs + 60);
            let mut requests = requests.write().await;

            requests.retain(|_, timestamps| {
                timestamps.retain(|&t| t > cutoff);
                !timestamps.is_empty()
            });
        }
        // Redis handles expiry automatically
    }

    /// Get current request count for an IP (for debugging/monitoring)
    pub async fn get_count(&self, ip: IpAddr) -> usize {
        match &self.inner {
            RateLimiterInner::Memory { requests } => {
                let now = Instant::now();
                let cutoff = now - Duration::from_secs(self.window_secs);
                let requests = requests.read().await;
                requests
                    .get(&ip)
                    .map(|ts| ts.iter().filter(|&&t| t > cutoff).count())
                    .unwrap_or(0)
            }
            RateLimiterInner::Redis { conn } => {
                let key = format!("ratelimit:ip:{}", ip);
                let mut conn = conn.clone();
                let result: Result<usize, redis::RedisError> = async {
                    let count: Option<usize> = redis::cmd("GET")
                        .arg(&key)
                        .query_async(&mut conn)
                        .await?;
                    Ok(count.unwrap_or(0))
                }
                .await;
                result.unwrap_or(0)
            }
        }
    }

    /// Check if using Redis backend
    pub fn is_redis(&self) -> bool {
        matches!(self.inner, RateLimiterInner::Redis { .. })
    }
}

/// Rate limit middleware
pub async fn rate_limit_middleware(
    State(limiter): State<RateLimiter>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    // Skip rate limiting for health checks
    let path = request.uri().path();
    if path == "/health" || path == "/ready" {
        return next.run(request).await;
    }

    let ip = addr.ip();

    // Check for X-Forwarded-For header (behind proxy like Railway)
    let real_ip = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse::<IpAddr>().ok())
        .unwrap_or(ip);

    if !limiter.check_and_record(real_ip).await {
        return RateLimitResponse.into_response();
    }

    next.run(request).await
}

struct RateLimitResponse;

impl IntoResponse for RateLimitResponse {
    fn into_response(self) -> Response {
        (
            StatusCode::TOO_MANY_REQUESTS,
            [
                ("Retry-After", "60"),
                ("Content-Type", "application/json"),
            ],
            r#"{"error":"rate_limited","message":"Too many requests. Please slow down."}"#,
        )
            .into_response()
    }
}

/// Start background cleanup task for rate limiter (only needed for memory backend)
pub fn start_cleanup_task(limiter: RateLimiter) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            limiter.cleanup().await;
        }
    });
}
