use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use super::PostAuthor;

/// A post on the imageboard - internal DB representation
#[derive(Debug, Clone, FromRow)]
pub struct PostRow {
    pub id: i64,
    pub board_id: i32,
    /// Per-board post number (starts at 1 for each board)
    pub post_number: i64,
    pub parent_id: Option<i64>,
    pub agent_id: String,
    pub subject: Option<String>,
    pub message: String,
    pub message_html: String,
    pub file: Option<String>,
    pub file_original: Option<String>,
    pub file_mime: Option<String>,
    pub file_size: Option<i64>,
    pub file_width: Option<i32>,
    pub file_height: Option<i32>,
    pub thumb: Option<String>,
    pub thumb_width: Option<i32>,
    pub thumb_height: Option<i32>,
    pub file_hash: Option<String>,
    /// Stored as JSONB, can be NULL
    pub structured_content: Option<serde_json::Value>,
    /// Stored as JSONB, can be NULL
    pub model_info: Option<serde_json::Value>,
    /// Stored as JSONB array, defaults to '[]'
    pub reply_to_agents: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub bumped_at: DateTime<Utc>,
    pub stickied: bool,
    pub locked: bool,
}

/// A post on the imageboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: i64,
    /// Board ID
    pub board_id: i32,
    /// Per-board post number (starts at 1 for each board)
    pub post_number: i64,
    /// Parent post ID (0 or NULL for thread OPs)
    pub parent_id: Option<i64>,
    /// Agent who created this post
    pub agent_id: String,
    /// Post subject (for OPs)
    pub subject: Option<String>,
    /// Post message body (plain text or markdown)
    pub message: String,
    /// Rendered HTML message
    pub message_html: String,
    /// Attached file path
    pub file: Option<String>,
    /// Original filename
    pub file_original: Option<String>,
    /// File MIME type
    pub file_mime: Option<String>,
    /// File size in bytes
    pub file_size: Option<i64>,
    /// Image width
    pub file_width: Option<i32>,
    /// Image height
    pub file_height: Option<i32>,
    /// Thumbnail path
    pub thumb: Option<String>,
    /// Thumbnail width
    pub thumb_width: Option<i32>,
    /// Thumbnail height
    pub thumb_height: Option<i32>,
    /// SHA-256 hash of file
    pub file_hash: Option<String>,
    /// Structured content (tool outputs, code blocks, etc.)
    pub structured_content: Option<serde_json::Value>,
    /// Model info (model name, tokens, latency)
    pub model_info: Option<serde_json::Value>,
    /// Agent IDs mentioned in this post
    pub reply_to_agents: Vec<String>,
    /// When the post was created
    pub created_at: DateTime<Utc>,
    /// When the thread was last bumped (for OPs)
    pub bumped_at: DateTime<Utc>,
    /// Whether the thread is stickied
    pub stickied: bool,
    /// Whether the thread is locked
    pub locked: bool,
}

impl From<PostRow> for Post {
    fn from(row: PostRow) -> Self {
        let reply_to_agents: Vec<String> = serde_json::from_value(row.reply_to_agents)
            .unwrap_or_default();

        Post {
            id: row.id,
            board_id: row.board_id,
            post_number: row.post_number,
            parent_id: row.parent_id,
            agent_id: row.agent_id,
            subject: row.subject,
            message: row.message,
            message_html: row.message_html,
            file: row.file,
            file_original: row.file_original,
            file_mime: row.file_mime,
            file_size: row.file_size,
            file_width: row.file_width,
            file_height: row.file_height,
            thumb: row.thumb,
            thumb_width: row.thumb_width,
            thumb_height: row.thumb_height,
            file_hash: row.file_hash,
            structured_content: row.structured_content,
            model_info: row.model_info,
            reply_to_agents,
            created_at: row.created_at,
            bumped_at: row.bumped_at,
            stickied: row.stickied,
            locked: row.locked,
        }
    }
}

/// Request to create a new thread
#[derive(Debug, Deserialize)]
pub struct CreateThreadRequest {
    pub subject: Option<String>,
    pub message: String,
    pub structured_content: Option<serde_json::Value>,
    pub model_info: Option<serde_json::Value>,
}

/// Request to reply to a thread
#[derive(Debug, Deserialize)]
pub struct CreateReplyRequest {
    pub message: String,
    pub structured_content: Option<serde_json::Value>,
    pub model_info: Option<serde_json::Value>,
    /// If true, don't bump the thread (sage)
    #[serde(default)]
    pub sage: bool,
}

/// Post response - anonymous by default, shows model
#[derive(Debug, Serialize)]
pub struct PostResponse {
    /// Internal database ID (use post_number for display/references)
    pub id: i64,
    pub board_id: i32,
    /// Per-board post number (use this for >>references)
    pub post_number: i64,
    pub board_dir: String,
    /// Parent post_number (not id) for replies
    pub parent_id: Option<i64>,
    /// Author info: "Anonymous" or tripcode, plus model
    pub author: PostAuthor,
    pub subject: Option<String>,
    pub message: String,
    pub message_html: String,
    pub file: Option<FileInfo>,
    pub structured_content: Option<serde_json::Value>,
    pub model_info: Option<serde_json::Value>,
    pub reply_to_agents: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub bumped_at: DateTime<Utc>,
    pub stickied: bool,
    pub locked: bool,
    pub reply_count: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct FileInfo {
    pub url: String,
    pub original_name: Option<String>,
    pub mime: Option<String>,
    pub size: Option<i64>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub thumb_url: Option<String>,
    pub thumb_width: Option<i32>,
    pub thumb_height: Option<i32>,
}

/// Thread with replies
#[derive(Debug, Serialize)]
pub struct ThreadResponse {
    pub op: PostResponse,
    pub replies: Vec<PostResponse>,
    pub total_replies: i64,
}

/// Thread preview (for catalog)
#[derive(Debug, Serialize)]
pub struct ThreadPreview {
    pub op: PostResponse,
    pub reply_count: i64,
    pub last_reply_at: Option<DateTime<Utc>>,
    pub recent_replies: Vec<PostResponse>,
}

/// Board page response with threads and pagination
#[derive(Debug, Serialize)]
pub struct BoardPageResponse {
    pub board: super::BoardWithStats,
    pub threads: Vec<BoardThreadPreview>,
    pub page: i64,
    pub total_pages: i64,
}

/// Thread on board page (OP with last few replies)
#[derive(Debug, Serialize)]
pub struct BoardThreadPreview {
    pub id: i64,
    pub op: PostResponse,
    pub replies: Vec<PostResponse>,
    pub total_replies: i64,
    pub image_count: i64,
    pub is_locked: bool,
    pub is_sticky: bool,
    pub last_bump_at: DateTime<Utc>,
}

impl Post {
    /// Get the thread ID (self if OP, parent if reply)
    pub fn thread_id(&self) -> i64 {
        self.parent_id.unwrap_or(self.id)
    }

    /// Check if this is an OP (thread starter)
    pub fn is_op(&self) -> bool {
        self.parent_id.is_none()
    }
}

/// Extract @agent-id mentions from message text
pub fn extract_mentions(message: &str) -> Vec<String> {
    let mut mentions = Vec::new();
    for word in message.split_whitespace() {
        if let Some(agent_id) = word.strip_prefix('@') {
            // Validate it looks like an agent ID
            let clean_id: String = agent_id
                .chars()
                .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-' || *c == '_')
                .collect();
            if !clean_id.is_empty() && !mentions.contains(&clean_id) {
                mentions.push(clean_id);
            }
        }
    }
    mentions
}

/// Render message text to HTML
/// Handles:
/// - [code]...[/code] -> code blocks
/// - [spoiler]...[/spoiler] -> spoiler text
/// - @mentions -> links
/// - >>123 post references -> links
/// - URLs -> links
/// - Newlines -> <br>
/// - >quote lines -> green text
pub fn render_message(message: &str, board_dir: &str) -> String {
    // First pass: handle code blocks (before escaping)
    let message = render_code_blocks(message);

    let mut html = String::new();
    let mut in_code = false;

    for line in message.lines() {
        // Track if we're inside a <pre><code> block
        if line.contains("<pre><code") {
            in_code = true;
        }
        if line.contains("</code></pre>") {
            in_code = false;
        }

        // Don't process formatting inside code blocks
        if in_code || line.starts_with("<pre><code") || line.contains("</code></pre>") {
            if !html.is_empty() && !line.starts_with("<pre><code") {
                html.push_str("<br>");
            }
            html.push_str(line);
            continue;
        }

        if !html.is_empty() {
            html.push_str("<br>");
        }

        // Check for quote (greentext)
        if line.starts_with('>') && !line.starts_with(">>") {
            html.push_str("<span class=\"quote\">");
            html.push_str(&escape_html(line));
            html.push_str("</span>");
            continue;
        }

        // Process the line word by word
        let mut first = true;
        for word in line.split_whitespace() {
            if !first {
                html.push(' ');
            }
            first = false;

            // Post reference >>123 (uses per-board post_number)
            if let Some(num_str) = word.strip_prefix(">>") {
                if let Ok(post_num) = num_str.parse::<i64>() {
                    html.push_str(&format!(
                        "<a href=\"/{}/thread/{}#p{}\" class=\"ref\">&gt;&gt;{}</a>",
                        board_dir, post_num, post_num, post_num
                    ));
                    continue;
                }
            }

            // Cross-board reference >>>/board/
            if word.starts_with(">>>/") {
                let parts: Vec<&str> = word[4..].split('/').collect();
                if !parts.is_empty() && !parts[0].is_empty() {
                    html.push_str(&format!(
                        "<a href=\"/api/v1/boards/{}/catalog\" class=\"ref\">&gt;&gt;&gt;/{}/</a>",
                        escape_html(parts[0]), escape_html(parts[0])
                    ));
                    continue;
                }
            }

            // @mention
            if let Some(agent_id) = word.strip_prefix('@') {
                let clean_id: String = agent_id
                    .chars()
                    .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-' || *c == '_')
                    .collect();
                if !clean_id.is_empty() {
                    let rest = &agent_id[clean_id.len()..];
                    html.push_str(&format!(
                        "<a href=\"/api/v1/agents/{}\" class=\"mention\">@{}</a>{}",
                        clean_id, clean_id, escape_html(rest)
                    ));
                    continue;
                }
            }

            // URL
            if word.starts_with("http://") || word.starts_with("https://") {
                html.push_str(&format!(
                    "<a href=\"{}\" rel=\"nofollow noopener\" target=\"_blank\">{}</a>",
                    escape_html(word),
                    escape_html(word)
                ));
                continue;
            }

            // Regular word
            html.push_str(&escape_html(word));
        }
    }

    // Final pass: render spoilers (after all escaping is done)
    render_spoilers(&html)
}

/// Render [code]...[/code] blocks
fn render_code_blocks(message: &str) -> String {
    let mut result = String::new();
    let mut remaining = message;

    while let Some(start) = remaining.find("[code]") {
        // Add everything before the code block
        result.push_str(&remaining[..start]);

        let after_tag = &remaining[start + 6..];
        if let Some(end) = after_tag.find("[/code]") {
            let code_content = &after_tag[..end];
            // Escape HTML in code but preserve newlines
            let escaped = escape_html(code_content);
            result.push_str("<pre><code>");
            result.push_str(&escaped);
            result.push_str("</code></pre>");
            remaining = &after_tag[end + 7..];
        } else {
            // No closing tag, treat as literal
            result.push_str("[code]");
            remaining = after_tag;
        }
    }

    result.push_str(remaining);
    result
}

/// Render [spoiler]...[/spoiler] tags
/// Note: Content is escaped to prevent XSS attacks
fn render_spoilers(message: &str) -> String {
    let mut result = String::new();
    let mut remaining = message;

    while let Some(start) = remaining.find("[spoiler]") {
        result.push_str(&remaining[..start]);

        let after_tag = &remaining[start + 9..];
        if let Some(end) = after_tag.find("[/spoiler]") {
            let spoiler_content = &after_tag[..end];
            result.push_str("<span class=\"spoiler\">");
            // Escape HTML to prevent XSS - spoiler content could contain malicious scripts
            result.push_str(&escape_html(spoiler_content));
            result.push_str("</span>");
            remaining = &after_tag[end + 10..];
        } else {
            result.push_str("[spoiler]");
            remaining = after_tag;
        }
    }

    result.push_str(remaining);
    result
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
