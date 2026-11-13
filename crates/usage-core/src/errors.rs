//! Error model. Every failure carries a category the frontend can branch on.
//! Messages must never contain secrets (API keys, tokens, cookies).

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ErrorCategory {
    Network,
    Timeout,
    Authentication,
    Permission,
    RateLimited,
    InvalidResponse,
    UnsupportedCapability,
    ProviderUnavailable,
    Database,
    CredentialStorage,
    Configuration,
    Internal,
}

impl ErrorCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Timeout => "timeout",
            Self::Authentication => "authentication",
            Self::Permission => "permission",
            Self::RateLimited => "rate_limited",
            Self::InvalidResponse => "invalid_response",
            Self::UnsupportedCapability => "unsupported_capability",
            Self::ProviderUnavailable => "provider_unavailable",
            Self::Database => "database",
            Self::CredentialStorage => "credential_storage",
            Self::Configuration => "configuration",
            Self::Internal => "internal",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [
            Self::Network,
            Self::Timeout,
            Self::Authentication,
            Self::Permission,
            Self::RateLimited,
            Self::InvalidResponse,
            Self::UnsupportedCapability,
            Self::ProviderUnavailable,
            Self::Database,
            Self::CredentialStorage,
            Self::Configuration,
            Self::Internal,
        ]
        .into_iter()
        .find(|c| c.as_str() == s)
    }
}

#[derive(Debug, Error)]
pub enum UsageError {
    #[error("network failure: {0}")]
    Network(String),
    #[error("request timed out after {0:?}")]
    Timeout(Duration),
    #[error("authentication failed: {0}")]
    Authentication(String),
    #[error("permission denied: {0}")]
    Permission(String),
    #[error("rate limited{}", .retry_after.map(|d| format!(" (retry after {}s)", d.as_secs())).unwrap_or_default())]
    RateLimited { retry_after: Option<Duration> },
    #[error("invalid provider response: {0}")]
    InvalidResponse(String),
    #[error("unsupported provider capability: {0}")]
    UnsupportedCapability(String),
    #[error("provider unavailable: {0}")]
    ProviderUnavailable(String),
    #[error("database error: {0}")]
    Database(String),
    #[error("credential storage error: {0}")]
    CredentialStorage(String),
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T, E = UsageError> = std::result::Result<T, E>;

impl UsageError {
    pub fn category(&self) -> ErrorCategory {
        match self {
            Self::Network(_) => ErrorCategory::Network,
            Self::Timeout(_) => ErrorCategory::Timeout,
            Self::Authentication(_) => ErrorCategory::Authentication,
            Self::Permission(_) => ErrorCategory::Permission,
            Self::RateLimited { .. } => ErrorCategory::RateLimited,
            Self::InvalidResponse(_) => ErrorCategory::InvalidResponse,
            Self::UnsupportedCapability(_) => ErrorCategory::UnsupportedCapability,
            Self::ProviderUnavailable(_) => ErrorCategory::ProviderUnavailable,
            Self::Database(_) => ErrorCategory::Database,
            Self::CredentialStorage(_) => ErrorCategory::CredentialStorage,
            Self::Configuration(_) => ErrorCategory::Configuration,
            Self::Internal(_) => ErrorCategory::Internal,
        }
    }

    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::RateLimited { retry_after } => *retry_after,
            _ => None,
        }
    }

    /// Convert into the serialisable form sent to the frontend.
    pub fn to_info(
        &self,
        provider_id: Option<&str>,
        operation: &str,
        occurred_at: DateTime<Utc>,
    ) -> ErrorInfo {
        ErrorInfo {
            category: self.category(),
            message: self.to_string(),
            provider_id: provider_id.map(str::to_owned),
            operation: operation.to_owned(),
            occurred_at,
            retry_after_secs: self.retry_after().map(|d| d.as_secs() as u32),
        }
    }
}

#[cfg(feature = "sqlite")]
impl From<sqlx::Error> for UsageError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e.to_string())
    }
}

#[cfg(feature = "sqlite")]
impl From<sqlx::migrate::MigrateError> for UsageError {
    fn from(e: sqlx::migrate::MigrateError) -> Self {
        Self::Database(format!("migration failed: {e}"))
    }
}

impl From<serde_json::Error> for UsageError {
    fn from(e: serde_json::Error) -> Self {
        Self::InvalidResponse(e.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ErrorInfo {
    pub category: ErrorCategory,
    pub message: String,
    pub provider_id: Option<String>,
    pub operation: String,
    pub occurred_at: DateTime<Utc>,
    pub retry_after_secs: Option<u32>,
}
