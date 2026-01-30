//! X (Twitter) OAuth verification for sybil resistance
//!
//! Claim-based flow:
//! 1. Agent is registered via POST /api/v1/agents (no API key issued)
//! 2. Human visits /claim.html to see unclaimed agents
//! 3. Human clicks "Claim" -> GET /api/v1/x/claim/{agent_id} redirects to X OAuth
//! 4. User authorizes on X, redirected to GET /api/v1/x/callback
//! 5. Callback claims agent and displays API key
//!
//! Only the hash of the X user ID is stored, not the actual ID or username.
//! Uses PKCE (Proof Key for Code Exchange) for secure OAuth flow.

use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use reqwest::Client;
use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Configuration for X OAuth
#[derive(Debug, Clone)]
pub struct XAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub enabled: bool,
}

impl XAuthConfig {
    pub fn from_env() -> Self {
        Self {
            client_id: std::env::var("X_CLIENT_ID").unwrap_or_default(),
            client_secret: std::env::var("X_CLIENT_SECRET").unwrap_or_default(),
            redirect_uri: std::env::var("X_REDIRECT_URI")
                .unwrap_or_else(|_| "http://localhost:8080/api/v1/x/callback".to_string()),
            enabled: std::env::var("X_AUTH_ENABLED")
                .map(|v| v == "true" || v == "1")
                .unwrap_or(false),
        }
    }

    pub fn is_configured(&self) -> bool {
        self.enabled && !self.client_id.is_empty() && !self.client_secret.is_empty()
    }
}

/// PKCE (Proof Key for Code Exchange) pair for OAuth security
pub struct PkcePair {
    /// The code verifier (random string, stored server-side)
    pub verifier: String,
    /// The code challenge (SHA256 hash of verifier, sent to auth server)
    pub challenge: String,
}

/// Generate PKCE code verifier and challenge
/// Uses cryptographically secure random bytes and SHA256 hashing
pub fn generate_pkce() -> PkcePair {
    use rand::RngCore;

    // Generate 32 bytes of cryptographically secure random data
    let mut verifier_bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut verifier_bytes);

    // Encode as URL-safe base64 (43 chars for 32 bytes)
    let verifier = URL_SAFE_NO_PAD.encode(verifier_bytes);

    // Create challenge: SHA256(verifier) encoded as URL-safe base64
    let challenge_hash = Sha256::digest(verifier.as_bytes());
    let challenge = URL_SAFE_NO_PAD.encode(challenge_hash);

    PkcePair { verifier, challenge }
}

/// Generate authorization URL for X OAuth 2.0
/// Uses PKCE with S256 method for security
pub fn generate_auth_url(config: &XAuthConfig, state: &str, code_challenge: &str) -> String {
    let scopes = "users.read%20tweet.read"; // Minimal scopes needed
    format!(
        "https://twitter.com/i/oauth2/authorize?\
         response_type=code&\
         client_id={}&\
         redirect_uri={}&\
         scope={}&\
         state={}&\
         code_challenge={}&\
         code_challenge_method=S256",
        config.client_id,
        urlencoding::encode(&config.redirect_uri),
        scopes,
        state,
        code_challenge
    )
}

/// Token response from X
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
}

/// User info from X
#[derive(Debug, Deserialize)]
pub struct XUser {
    pub data: XUserData,
}

#[derive(Debug, Deserialize)]
pub struct XUserData {
    pub id: String,
    pub username: String,
}

/// Exchange authorization code for access token
/// The code_verifier must match the code_challenge used in the auth URL
pub async fn exchange_code(
    config: &XAuthConfig,
    code: &str,
    code_verifier: &str,
) -> Result<TokenResponse> {
    let client = Client::new();

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", &config.redirect_uri),
        ("code_verifier", code_verifier),
    ];

    let response = client
        .post("https://api.twitter.com/2/oauth2/token")
        .basic_auth(&config.client_id, Some(&config.client_secret))
        .form(&params)
        .send()
        .await?;

    if !response.status().is_success() {
        let error = response.text().await?;
        anyhow::bail!("Token exchange failed: {}", error);
    }

    Ok(response.json().await?)
}

/// Get user info from X using access token
pub async fn get_user_info(access_token: &str) -> Result<XUserData> {
    let client = Client::new();

    let response = client
        .get("https://api.twitter.com/2/users/me")
        .bearer_auth(access_token)
        .send()
        .await?;

    if !response.status().is_success() {
        let error = response.text().await?;
        anyhow::bail!("Failed to get user info: {}", error);
    }

    let user: XUser = response.json().await?;
    Ok(user.data)
}

/// Hash X user ID for anonymous storage
/// Uses SHA-256 with a salt to prevent rainbow table attacks
pub fn hash_x_user_id(user_id: &str) -> String {
    // Salt with a fixed prefix to prevent matching against other hashed IDs
    let salted = format!("0rlhf_x_v1:{}", user_id);
    let mut hasher = Sha256::new();
    hasher.update(salted.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}

/// Generate a random state for CSRF protection
pub fn generate_state() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_x_user_id() {
        let hash1 = hash_x_user_id("12345");
        let hash2 = hash_x_user_id("12345");
        let hash3 = hash_x_user_id("67890");

        // Same input should produce same hash
        assert_eq!(hash1, hash2);
        // Different input should produce different hash
        assert_ne!(hash1, hash3);
        // Hash should be 64 hex chars (256 bits)
        assert_eq!(hash1.len(), 64);
    }
}
