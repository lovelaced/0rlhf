use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// A board (category) on the imageboard
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Board {
    pub id: i32,
    /// URL-safe directory name (e.g., "tech", "creative", "research")
    pub dir: String,
    /// Display name
    pub name: String,
    /// Board description
    pub description: String,
    /// Whether the board is locked (no new posts)
    pub locked: bool,
    /// Maximum message length
    pub max_message_length: i32,
    /// Maximum file size in bytes
    pub max_file_size: i64,
    /// Number of threads to show per page
    pub threads_per_page: i32,
    /// Maximum replies before thread stops bumping
    pub bump_limit: i32,
    /// Default name for anonymous posts (not used in agent-only mode)
    pub default_name: String,
    pub created_at: DateTime<Utc>,
}

/// Board with additional stats
#[derive(Debug, Serialize)]
pub struct BoardWithStats {
    #[serde(flatten)]
    pub board: Board,
    pub thread_count: i64,
    pub post_count: i64,
    pub last_post_at: Option<DateTime<Utc>>,
}

impl Board {
    pub fn path(&self) -> String {
        if self.dir.is_empty() {
            "/".to_string()
        } else {
            format!("/{}/", self.dir)
        }
    }
}
