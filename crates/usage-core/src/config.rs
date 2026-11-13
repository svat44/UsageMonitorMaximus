//! Persistent application preferences (infrastructure-level only).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::errors::{Result, UsageError};
use crate::providers::RefreshPolicy;

pub const MAX_REFRESH_INTERVAL_SECS: u32 = 24 * 3600;
pub const PREF_KEY_CONFIG: &str = "app_config";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn as_filter(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct AppConfig {
    pub selected_provider: String,
    pub monitoring_enabled: bool,
    /// `None` = use the selected provider's recommended interval.
    pub refresh_interval_secs: Option<u32>,
    /// Register the app with the OS to start at login.
    pub launch_at_startup: bool,
    /// Closing the dashboard hides it to the tray instead of quitting.
    pub minimize_to_tray: bool,
    /// Do not show the dashboard window at launch (tray only).
    pub start_minimized: bool,
    pub log_level: LogLevel,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            selected_provider: crate::providers::statusline::STATUSLINE_PROVIDER_ID.into(),
            monitoring_enabled: true,
            refresh_interval_secs: None,
            launch_at_startup: false,
            minimize_to_tray: true,
            start_minimized: false,
            log_level: LogLevel::Info,
        }
    }
}

impl AppConfig {
    /// Validate against the chosen provider's policy.
    pub fn validate(&self, policy: &RefreshPolicy) -> Result<()> {
        if let Some(s) = self.refresh_interval_secs {
            if s < policy.min_interval_secs {
                return Err(UsageError::Configuration(format!(
                    "refresh interval {s}s is below this provider's minimum of {}s",
                    policy.min_interval_secs
                )));
            }
            if s > MAX_REFRESH_INTERVAL_SECS {
                return Err(UsageError::Configuration(format!(
                    "refresh interval {s}s exceeds the maximum of {MAX_REFRESH_INTERVAL_SECS}s"
                )));
            }
        }
        Ok(())
    }

    /// Effective polling interval for a provider.
    pub fn effective_interval_secs(&self, policy: &RefreshPolicy) -> u32 {
        self.refresh_interval_secs
            .unwrap_or(policy.recommended_interval_secs)
            .max(policy.min_interval_secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> RefreshPolicy {
        RefreshPolicy {
            min_interval_secs: 5,
            recommended_interval_secs: 30,
            polling_supported: true,
            event_driven: false,
            request_timeout_secs: 5,
            stale_after_secs: 60,
            documented_rate_limit: None,
        }
    }

    #[test]
    fn interval_validation_and_defaults() {
        let mut c = AppConfig::default();
        assert_eq!(c.effective_interval_secs(&policy()), 30);
        c.refresh_interval_secs = Some(2);
        assert!(c.validate(&policy()).is_err());
        c.refresh_interval_secs = Some(10);
        assert!(c.validate(&policy()).is_ok());
        assert_eq!(c.effective_interval_secs(&policy()), 10);
        c.refresh_interval_secs = Some(MAX_REFRESH_INTERVAL_SECS + 1);
        assert!(c.validate(&policy()).is_err());
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let c: AppConfig = serde_json::from_str(r#"{"monitoringEnabled": false}"#).unwrap();
        assert!(!c.monitoring_enabled);
        assert!(c.minimize_to_tray);
    }
}
