//! Normalised usage data model.
//!
//! Design rules (see docs/PROVIDER_CONTRACTS.md):
//! * A measurement's **category is derived from its id** — it cannot be set independently,
//!   so an API token count can never be labelled subscription usage.
//! * Absent data is [`MeasurementValue::Unavailable`], never `0`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Where a measurement comes from / what it means. The three categories never mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MeasurementCategory {
    /// Claude Free/Pro/Max/Team subscription limits (claude.ai plan windows).
    SubscriptionUsage,
    /// Anthropic API consumption (tokens/requests/cost billed to an API organisation).
    ApiConsumption,
    /// Observed from the local Claude Code installation (context window, session cost…).
    LocalClaudeCode,
}

impl MeasurementCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SubscriptionUsage => "subscription_usage",
            Self::ApiConsumption => "api_consumption",
            Self::LocalClaudeCode => "local_claude_code",
        }
    }
}

/// Identifiers of measurements implemented by verified sources (or by the simulator).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum MeasurementId {
    /// % of the rolling 5-hour subscription window consumed.
    SubscriptionFiveHourUtilization,
    /// % of the 7-day subscription window consumed.
    SubscriptionWeeklyUtilization,
    /// Input tokens billed to an API organisation (Admin API; no real provider yet).
    ApiInputTokens,
    /// Output tokens billed to an API organisation (Admin API; no real provider yet).
    ApiOutputTokens,
    /// % of one Claude Code session's *context window* in use. NOT a quota.
    LocalContextWindowUtilization,
    /// Client-side list-price cost estimate of one Claude Code session. NOT a bill.
    LocalSessionCostEstimate,
    /// Number of Claude Code sessions seen recently.
    LocalActiveSessions,
}

impl MeasurementId {
    pub const ALL: [MeasurementId; 7] = [
        Self::SubscriptionFiveHourUtilization,
        Self::SubscriptionWeeklyUtilization,
        Self::ApiInputTokens,
        Self::ApiOutputTokens,
        Self::LocalContextWindowUtilization,
        Self::LocalSessionCostEstimate,
        Self::LocalActiveSessions,
    ];

    pub fn category(self) -> MeasurementCategory {
        use MeasurementId::*;
        match self {
            SubscriptionFiveHourUtilization | SubscriptionWeeklyUtilization => {
                MeasurementCategory::SubscriptionUsage
            }
            ApiInputTokens | ApiOutputTokens => MeasurementCategory::ApiConsumption,
            LocalContextWindowUtilization | LocalSessionCostEstimate | LocalActiveSessions => {
                MeasurementCategory::LocalClaudeCode
            }
        }
    }

    pub fn unit(self) -> Unit {
        use MeasurementId::*;
        match self {
            SubscriptionFiveHourUtilization
            | SubscriptionWeeklyUtilization
            | LocalContextWindowUtilization => Unit::Percent,
            ApiInputTokens | ApiOutputTokens => Unit::Tokens,
            LocalSessionCostEstimate => Unit::Usd,
            LocalActiveSessions => Unit::Count,
        }
    }

    pub fn window(self) -> Option<UsageWindow> {
        match self {
            Self::SubscriptionFiveHourUtilization => Some(UsageWindow::FiveHour),
            Self::SubscriptionWeeklyUtilization => Some(UsageWindow::SevenDay),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        use MeasurementId::*;
        match self {
            SubscriptionFiveHourUtilization => "subscription_five_hour_utilization",
            SubscriptionWeeklyUtilization => "subscription_weekly_utilization",
            ApiInputTokens => "api_input_tokens",
            ApiOutputTokens => "api_output_tokens",
            LocalContextWindowUtilization => "local_context_window_utilization",
            LocalSessionCostEstimate => "local_session_cost_estimate",
            LocalActiveSessions => "local_active_sessions",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.as_str() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Unit {
    Percent,
    Usd,
    Tokens,
    Count,
}

impl Unit {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Percent => "percent",
            Self::Usd => "usd",
            Self::Tokens => "tokens",
            Self::Count => "count",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        [Self::Percent, Self::Usd, Self::Tokens, Self::Count]
            .into_iter()
            .find(|u| u.as_str() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UsageWindow {
    FiveHour,
    SevenDay,
}

impl UsageWindow {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FiveHour => "five_hour",
            Self::SevenDay => "seven_day",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        [Self::FiveHour, Self::SevenDay]
            .into_iter()
            .find(|w| w.as_str() == s)
    }
}

/// How well the value is verified against its origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Verification {
    /// Documented local interface of the official client (e.g. Claude Code status line).
    OfficialLocalInterface,
    /// Documented Anthropic API.
    OfficialApi,
    /// Generated by a simulator/mock. Not real.
    Simulated,
}

impl Verification {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OfficialLocalInterface => "official_local_interface",
            Self::OfficialApi => "official_api",
            Self::Simulated => "simulated",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        [Self::OfficialLocalInterface, Self::OfficialApi, Self::Simulated]
            .into_iter()
            .find(|v| v.as_str() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum UnavailableReason {
    /// The provider cannot supply this measurement at all.
    NotSupportedByProvider,
    /// The source has produced no data yet (e.g. no Claude Code session seen).
    NoDataYet,
    /// The source produced data, but not this field (e.g. API-key user has no `rate_limits`).
    NotReportedBySource,
    /// The source reported a window that has since reset; the new value is unknown.
    WindowExpired,
    /// The provider response omitted this measurement.
    NotReported,
}

impl UnavailableReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotSupportedByProvider => "not_supported_by_provider",
            Self::NoDataYet => "no_data_yet",
            Self::NotReportedBySource => "not_reported_by_source",
            Self::WindowExpired => "window_expired",
            Self::NotReported => "not_reported",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        [
            Self::NotSupportedByProvider,
            Self::NoDataYet,
            Self::NotReportedBySource,
            Self::WindowExpired,
            Self::NotReported,
        ]
        .into_iter()
        .find(|r| r.as_str() == s)
    }
}

/// A value, or an explicit statement that there isn't one. Missing ≠ zero.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export)]
pub enum MeasurementValue {
    Available { value: f64 },
    Unavailable { reason: UnavailableReason },
}

impl MeasurementValue {
    pub fn value(&self) -> Option<f64> {
        match self {
            Self::Available { value } => Some(*value),
            Self::Unavailable { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Measurement {
    pub id: MeasurementId,
    /// Always `id.category()`; kept explicit so the frontend never has to infer it.
    pub category: MeasurementCategory,
    pub value: MeasurementValue,
    pub unit: Unit,
    pub provider_id: String,
    /// Discriminates repeated measurements of the same id (e.g. the Claude Code session id).
    pub scope: Option<String>,
    pub collected_at: DateTime<Utc>,
    /// When the *source* produced the value, when it says so.
    pub source_at: Option<DateTime<Utc>>,
    pub verification: Verification,
    pub simulated: bool,
    pub estimated: bool,
    /// Last known value that could not be refreshed, or older than the provider's freshness limit.
    pub stale: bool,
    /// Loaded from persisted history at startup; not observed in this run.
    pub restored: bool,
    pub resets_at: Option<DateTime<Utc>>,
    pub window: Option<UsageWindow>,
}

impl Measurement {
    pub fn new(
        id: MeasurementId,
        provider_id: impl Into<String>,
        value: MeasurementValue,
        collected_at: DateTime<Utc>,
        verification: Verification,
    ) -> Self {
        Self {
            id,
            category: id.category(),
            value,
            unit: id.unit(),
            provider_id: provider_id.into(),
            scope: None,
            collected_at,
            source_at: None,
            verification,
            simulated: verification == Verification::Simulated,
            estimated: false,
            stale: false,
            restored: false,
            resets_at: None,
            window: id.window(),
        }
    }

    pub fn available(
        id: MeasurementId,
        provider_id: impl Into<String>,
        value: f64,
        collected_at: DateTime<Utc>,
        verification: Verification,
    ) -> Self {
        Self::new(
            id,
            provider_id,
            MeasurementValue::Available { value },
            collected_at,
            verification,
        )
    }

    pub fn unavailable(
        id: MeasurementId,
        provider_id: impl Into<String>,
        reason: UnavailableReason,
        collected_at: DateTime<Utc>,
        verification: Verification,
    ) -> Self {
        Self::new(
            id,
            provider_id,
            MeasurementValue::Unavailable { reason },
            collected_at,
            verification,
        )
    }

    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }
    pub fn with_source_at(mut self, t: DateTime<Utc>) -> Self {
        self.source_at = Some(t);
        self
    }
    pub fn with_resets_at(mut self, t: DateTime<Utc>) -> Self {
        self.resets_at = Some(t);
        self
    }
    pub fn with_estimated(mut self, v: bool) -> Self {
        self.estimated = v;
        self
    }

    /// Identity of "the same measurement over time".
    pub fn key(&self) -> MeasurementKey {
        MeasurementKey {
            provider_id: self.provider_id.clone(),
            id: self.id,
            scope: self.scope.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MeasurementKey {
    pub provider_id: String,
    pub id: MeasurementId,
    pub scope: Option<String>,
}

// ---- Coverage: what the user may legitimately believe about each category ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum CoverageState {
    /// At least one real (non-simulated) value is available and fresh.
    Live,
    /// Real values exist but all are stale/restored.
    Stale,
    /// Only simulated values exist.
    Simulated,
    /// No real data. The UI must say this category is "not connected".
    NotConnected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CategoryCoverage {
    pub category: MeasurementCategory,
    pub state: CoverageState,
}

/// Derive per-category coverage purely from measurements, so UI claims can't drift
/// from the data. Simulated values never count as real.
pub fn compute_coverage(measurements: &[Measurement]) -> Vec<CategoryCoverage> {
    [
        MeasurementCategory::SubscriptionUsage,
        MeasurementCategory::ApiConsumption,
        MeasurementCategory::LocalClaudeCode,
    ]
    .into_iter()
    .map(|category| {
        let avail: Vec<&Measurement> = measurements
            .iter()
            .filter(|m| m.category == category && m.value.value().is_some())
            .collect();
        let real: Vec<&&Measurement> = avail.iter().filter(|m| !m.simulated).collect();
        let state = if !real.is_empty() {
            if real.iter().any(|m| !m.stale && !m.restored) {
                CoverageState::Live
            } else {
                CoverageState::Stale
            }
        } else if !avail.is_empty() {
            CoverageState::Simulated
        } else {
            CoverageState::NotConnected
        };
        CategoryCoverage { category, state }
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn category_is_derived_from_id() {
        let m = Measurement::available(
            MeasurementId::ApiInputTokens,
            "p",
            5.0,
            now(),
            Verification::OfficialApi,
        );
        assert_eq!(m.category, MeasurementCategory::ApiConsumption);
        let c = Measurement::available(
            MeasurementId::LocalContextWindowUtilization,
            "p",
            50.0,
            now(),
            Verification::OfficialLocalInterface,
        );
        assert_eq!(c.category, MeasurementCategory::LocalClaudeCode);
        assert_ne!(c.category, MeasurementCategory::SubscriptionUsage);
    }

    #[test]
    fn ids_round_trip_through_strings() {
        for id in MeasurementId::ALL {
            assert_eq!(MeasurementId::parse(id.as_str()), Some(id));
            // and through serde, which must agree with as_str()
            let json = serde_json::to_string(&id).unwrap();
            assert_eq!(json, format!("\"{}\"", id.as_str()));
        }
    }

    #[test]
    fn unavailable_is_not_zero_in_json() {
        let m = Measurement::unavailable(
            MeasurementId::SubscriptionFiveHourUtilization,
            "p",
            UnavailableReason::NoDataYet,
            now(),
            Verification::OfficialLocalInterface,
        );
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["value"]["state"], "unavailable");
        assert!(v["value"].get("value").is_none());
        assert_eq!(m.value.value(), None);
    }

    #[test]
    fn coverage_never_counts_simulated_as_real() {
        let sim = Measurement::available(
            MeasurementId::SubscriptionFiveHourUtilization,
            "sim",
            30.0,
            now(),
            Verification::Simulated,
        );
        let cov = compute_coverage(&[sim]);
        let sub = cov
            .iter()
            .find(|c| c.category == MeasurementCategory::SubscriptionUsage)
            .unwrap();
        assert_eq!(sub.state, CoverageState::Simulated);
        let api = cov
            .iter()
            .find(|c| c.category == MeasurementCategory::ApiConsumption)
            .unwrap();
        assert_eq!(api.state, CoverageState::NotConnected);
    }

    #[test]
    fn coverage_distinguishes_live_and_stale() {
        let mut m = Measurement::available(
            MeasurementId::LocalContextWindowUtilization,
            "p",
            10.0,
            now(),
            Verification::OfficialLocalInterface,
        );
        assert_eq!(compute_coverage(&[m.clone()])[2].state, CoverageState::Live);
        m.stale = true;
        assert_eq!(compute_coverage(&[m])[2].state, CoverageState::Stale);
    }
}
