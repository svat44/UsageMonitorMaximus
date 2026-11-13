//! Bridge ⇄ app file format for Claude Code's documented status-line JSON
//! (<https://code.claude.com/docs/en/statusline>).
//!
//! Data minimisation: the bridge keeps ONLY the fields listed below. It drops
//! `transcript_path`, `cwd`, workspace paths, PR info, etc.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;
const MAX_FILE_BYTES: u64 = 256 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StatusLinePayload {
    pub session_id: Option<String>,
    pub version: Option<String>,
    pub model: Option<ModelInfo>,
    pub context_window: Option<ContextWindow>,
    pub cost: Option<Cost>,
    pub rate_limits: Option<RateLimits>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContextWindow {
    pub used_percentage: Option<f64>,
    pub context_window_size: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Cost {
    pub total_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RateLimits {
    pub five_hour: Option<RateWindow>,
    pub seven_day: Option<RateWindow>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RateWindow {
    /// 0–100 per the docs.
    pub used_percentage: Option<f64>,
    /// Unix epoch seconds per the docs.
    pub resets_at: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotEnvelope {
    pub schema_version: u32,
    pub received_at: DateTime<Utc>,
    pub payload: StatusLinePayload,
}

impl StatusLinePayload {
    /// Parse Claude Code's raw stdin JSON, keeping only the fields we use.
    pub fn from_claude_code_json(raw: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(raw)
    }
}

/// Make a session id safe to use as a file name.
pub fn sanitize_session_id(id: &str) -> String {
    let s: String = id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    if s.is_empty() {
        "unknown".into()
    } else {
        s
    }
}

pub fn snapshot_file_name(session_id: Option<&str>) -> String {
    format!("{}.json", sanitize_session_id(session_id.unwrap_or("unknown")))
}

/// Atomically (temp file + rename) write the snapshot. Returns the final path.
pub fn write_snapshot(dir: &Path, env: &SnapshotEnvelope) -> std::io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let name = snapshot_file_name(env.payload.session_id.as_deref());
    let final_path = dir.join(&name);
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(&serde_json::to_vec(env).map_err(std::io::Error::other)?)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, &final_path).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })?;
    Ok(final_path)
}

#[derive(Debug, Default)]
pub struct ReadOutcome {
    pub snapshots: Vec<SnapshotEnvelope>,
    /// `*.json` files that could not be parsed or had an unknown schema.
    pub malformed: usize,
}

/// Read every `*.json` snapshot in `dir`. A missing directory is "no data", not an error.
pub fn read_snapshots(dir: &Path) -> std::io::Result<ReadOutcome> {
    let mut out = ReadOutcome::default();
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e),
    };
    for entry in rd.flatten() {
        let path = entry.path();
        let is_json = path.extension().is_some_and(|e| e == "json")
            && !entry.file_name().to_string_lossy().starts_with('.');
        if !is_json {
            continue;
        }
        let ok = entry
            .metadata()
            .map(|m| m.is_file() && m.len() <= MAX_FILE_BYTES)
            .unwrap_or(false);
        let parsed = ok
            .then(|| fs::read(&path).ok())
            .flatten()
            .and_then(|b| serde_json::from_slice::<SnapshotEnvelope>(&b).ok())
            .filter(|e| e.schema_version == SCHEMA_VERSION);
        match parsed {
            Some(env) => out.snapshots.push(env),
            None => out.malformed += 1,
        }
    }
    Ok(out)
}

/// Delete snapshot files not modified for `max_age`. Returns the number removed.
pub fn prune_old(dir: &Path, max_age: std::time::Duration) -> usize {
    let Ok(rd) = fs::read_dir(dir) else { return 0 };
    let mut n = 0;
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_some_and(|x| x == "json" || x == "tmp") {
            let old = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age > max_age);
            if old && fs::remove_file(&p).is_ok() {
                n += 1;
            }
        }
    }
    n
}

/// One-line text the bridge prints back to Claude Code's status bar.
pub fn summary_line(p: &StatusLinePayload) -> String {
    let mut parts = Vec::new();
    if let Some(rl) = &p.rate_limits {
        if let Some(v) = rl.five_hour.as_ref().and_then(|w| w.used_percentage) {
            parts.push(format!("5h {:.0}%", v));
        }
        if let Some(v) = rl.seven_day.as_ref().and_then(|w| w.used_percentage) {
            parts.push(format!("7d {:.0}%", v));
        }
    }
    if let Some(v) = p.context_window.as_ref().and_then(|c| c.used_percentage) {
        parts.push(format!("ctx {:.0}%", v));
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC_EXAMPLE: &str = r#"{
      "session_id": "abc123",
      "transcript_path": "/secret/path.jsonl",
      "cwd": "/home/me/project",
      "model": {"id": "claude-x", "display_name": "X"},
      "cost": {"total_cost_usd": 0.01234},
      "context_window": {"used_percentage": 8, "context_window_size": 200000},
      "rate_limits": {
        "five_hour": {"used_percentage": 23.5, "resets_at": 1738425600},
        "seven_day": {"used_percentage": 41.2, "resets_at": 1738857600}
      }
    }"#;

    #[test]
    fn parses_documented_schema_and_drops_private_fields() {
        let p = StatusLinePayload::from_claude_code_json(DOC_EXAMPLE).unwrap();
        assert_eq!(p.session_id.as_deref(), Some("abc123"));
        let rl = p.rate_limits.unwrap();
        assert_eq!(rl.five_hour.unwrap().used_percentage, Some(23.5));
        let again =
            serde_json::to_string(&StatusLinePayload::from_claude_code_json(DOC_EXAMPLE).unwrap()).unwrap();
        assert!(!again.contains("transcript"));
        assert!(!again.contains("/home/me"));
    }

    #[test]
    fn absent_rate_limits_stay_absent() {
        let p = StatusLinePayload::from_claude_code_json(r#"{"session_id":"s"}"#).unwrap();
        assert!(p.rate_limits.is_none());
        assert_eq!(summary_line(&p), "");
    }

    #[test]
    fn sanitises_session_ids() {
        assert_eq!(sanitize_session_id("../../etc/passwd"), "______etc_passwd");
        assert_eq!(sanitize_session_id(""), "unknown");
        assert_eq!(sanitize_session_id(&"a".repeat(200)).len(), 64);
    }

    #[test]
    fn write_then_read_round_trips_and_ignores_garbage() {
        let d = tempfile::tempdir().unwrap();
        let env = SnapshotEnvelope {
            schema_version: SCHEMA_VERSION,
            received_at: Utc::now(),
            payload: StatusLinePayload::from_claude_code_json(DOC_EXAMPLE).unwrap(),
        };
        write_snapshot(d.path(), &env).unwrap();
        fs::write(d.path().join("bad.json"), b"{not json").unwrap();
        fs::write(d.path().join(".hidden.json"), b"{}").unwrap();
        let out = read_snapshots(d.path()).unwrap();
        assert_eq!(out.snapshots.len(), 1);
        assert_eq!(out.malformed, 1);
        assert_eq!(
            summary_line(&out.snapshots[0].payload),
            "5h 24% · 7d 41% · ctx 8%"
        );
    }

    #[test]
    fn missing_dir_is_no_data() {
        let d = tempfile::tempdir().unwrap();
        let out = read_snapshots(&d.path().join("nope")).unwrap();
        assert!(out.snapshots.is_empty());
    }
}
