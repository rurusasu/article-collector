use super::token_store::TokenSet;
use anyhow::{bail, Context, Result};
use base64::prelude::{Engine as _, BASE64_URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use rand::RngCore;
use reqwest::StatusCode;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use url::Url;

pub const READ_ONLY_SCOPES: &str = "tweet.read users.read bookmark.read offline.access";
pub const AUTHORIZE_URL: &str = "https://twitter.com/i/oauth2/authorize";
pub const TOKEN_URL: &str = "https://api.x.com/2/oauth2/token";
pub const REDIRECT_URI: &str = "http://127.0.0.1:53681/callback";

#[derive(Debug, Clone)]
pub struct OAuthConfig {
    pub client_id: String,
    pub authorize_url: Url,
    pub token_url: Url,
    pub redirect_uri: Url,
    pub scopes: String,
}

impl OAuthConfig {
    pub fn from_client_id(client_id: impl Into<String>) -> Result<Self> {
        Ok(Self {
            client_id: client_id.into(),
            authorize_url: Url::parse(AUTHORIZE_URL).context("Invalid X OAuth authorize URL")?,
            token_url: Url::parse(TOKEN_URL).context("Invalid X OAuth token URL")?,
            redirect_uri: Url::parse(REDIRECT_URI).context("Invalid X OAuth redirect URI")?,
            scopes: READ_ONLY_SCOPES.to_string(),
        })
    }

    pub fn for_test(
        client_id: &str,
        authorize_url: &str,
        token_url: &str,
        redirect_uri: &str,
    ) -> Result<Self> {
        Ok(Self {
            client_id: client_id.to_string(),
            authorize_url: Url::parse(authorize_url).context("Invalid OAuth authorize URL")?,
            token_url: Url::parse(token_url).context("Invalid OAuth token URL")?,
            redirect_uri: Url::parse(redirect_uri).context("Invalid OAuth redirect URI")?,
            scopes: READ_ONLY_SCOPES.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
    pub url: Url,
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallbackResult {
    StateMismatch { expected: String },
    Denied { reason: String },
    MissingCode,
}

impl std::fmt::Display for CallbackResult {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StateMismatch { .. } => formatter.write_str("OAuth callback state mismatch"),
            Self::Denied { reason } => write!(formatter, "OAuth authorization denied: {reason}"),
            Self::MissingCode => {
                formatter.write_str("OAuth callback did not contain an authorization code")
            }
        }
    }
}

impl std::error::Error for CallbackResult {}

#[derive(Debug, Clone)]
pub struct OAuthClient {
    config: OAuthConfig,
    http: reqwest::Client,
}

impl OAuthClient {
    pub fn new(config: OAuthConfig) -> Result<Self> {
        if config.client_id.trim().is_empty() {
            bail!("X OAuth requires X_OAUTH_CLIENT_ID")
        }
        Ok(Self {
            config,
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .context("Failed to create X OAuth HTTP client")?,
        })
    }

    pub fn authorization_request(&self) -> Result<AuthorizationRequest> {
        let verifier = random_urlsafe(64);
        let challenge = pkce_challenge(&verifier);
        let state = random_urlsafe(32);
        let mut url = self.config.authorize_url.clone();
        url.query_pairs_mut()
            .append_pair("response_type", "code")
            .append_pair("client_id", &self.config.client_id)
            .append_pair("redirect_uri", self.config.redirect_uri.as_str())
            .append_pair("scope", &self.config.scopes)
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(AuthorizationRequest {
            url,
            verifier,
            challenge,
            state,
        })
    }

    pub fn parse_callback(
        &self,
        callback: &Url,
        expected_state: &str,
    ) -> Result<String, CallbackResult> {
        let query: std::collections::HashMap<_, _> = callback.query_pairs().into_owned().collect();
        if query.get("state").map(String::as_str) != Some(expected_state) {
            return Err(CallbackResult::StateMismatch {
                expected: "[REDACTED]".to_string(),
            });
        }
        if let Some(reason) = query.get("error") {
            return Err(CallbackResult::Denied {
                reason: sanitize_oauth_reason(reason),
            });
        }
        query
            .get("code")
            .filter(|code| !code.trim().is_empty())
            .cloned()
            .ok_or(CallbackResult::MissingCode)
    }

    pub async fn exchange_code(&self, code: &str, verifier: &str) -> Result<TokenSet> {
        self.token_request(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", self.config.redirect_uri.as_str()),
            ("client_id", self.config.client_id.as_str()),
            ("code_verifier", verifier),
        ])
        .await
    }

    pub async fn refresh(&self, refresh_token: &str) -> Result<TokenSet> {
        self.token_request(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", self.config.client_id.as_str()),
        ])
        .await
    }

    async fn token_request(&self, form: &[(&str, &str)]) -> Result<TokenSet> {
        let response = self
            .http
            .post(self.config.token_url.clone())
            .form(form)
            .send()
            .await
            .context("X OAuth token request failed")?;
        let status = response.status();
        let body = response
            .json::<TokenResponse>()
            .await
            .context("X OAuth token response was not valid JSON")?;
        if status != StatusCode::OK {
            bail!(
                "X OAuth token request failed with HTTP status {status}: {}",
                body.error.as_deref().unwrap_or("authorization failed")
            )
        }
        let access_token = body
            .access_token
            .filter(|token| !token.is_empty())
            .context("X OAuth token response did not contain an access token")?;
        let expires_in = body.expires_in.unwrap_or(1800).max(1);
        Ok(TokenSet {
            access_token,
            refresh_token: body.refresh_token,
            expires_at: Utc::now() + Duration::seconds(expires_in),
        })
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    error: Option<String>,
}

pub fn pkce_challenge(verifier: &str) -> String {
    BASE64_URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

fn random_urlsafe(bytes: usize) -> String {
    let mut value = vec![0_u8; bytes];
    rand::rng().fill_bytes(&mut value);
    BASE64_URL_SAFE_NO_PAD.encode(value)
}

fn sanitize_oauth_reason(reason: &str) -> String {
    reason
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        .take(64)
        .collect()
}
