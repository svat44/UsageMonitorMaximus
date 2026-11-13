//! `usage-monitor-statusline` — configure as Claude Code's `statusLine.command`.
//!
//! Reads Claude Code's documented status-line JSON from stdin, stores the fields the desktop app
//! needs in `<data dir>/statusline/<session>.json` (atomic write), and prints a short one-line
//! summary for Claude Code's own status bar. No network. No credentials. Never fails Claude Code:
//! all errors are swallowed and the exit code is always 0.

use std::io::Read;
use std::path::PathBuf;

use usage_core::paths;
use usage_core::statusline_protocol::*;

const MAX_STDIN: u64 = 1024 * 1024;

fn data_dir_from_args() -> PathBuf {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        if a == "--data-dir" {
            if let Some(p) = args.next() {
                return PathBuf::from(p);
            }
        }
    }
    paths::default_data_dir()
}

/// Core logic, separated from process I/O for testing. Returns the line to print.
pub fn handle(raw: &str, data_dir: &std::path::Path) -> String {
    let Ok(payload) = StatusLinePayload::from_claude_code_json(raw) else {
        return String::new();
    };
    let line = summary_line(&payload);
    let env = SnapshotEnvelope {
        schema_version: SCHEMA_VERSION,
        received_at: chrono::Utc::now(),
        payload,
    };
    let _ = write_snapshot(&paths::statusline_dir(data_dir), &env);
    line
}

fn main() {
    let mut raw = String::new();
    let _ = std::io::stdin().take(MAX_STDIN).read_to_string(&mut raw);
    println!("{}", handle(&raw, &data_dir_from_args()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_snapshot_and_returns_summary() {
        let d = tempfile::tempdir().unwrap();
        let out = handle(
            r#"{"session_id":"s/../1","transcript_path":"/private","rate_limits":{"five_hour":{"used_percentage":50,"resets_at":4102444800}}}"#,
            d.path(),
        );
        assert_eq!(out, "5h 50%");
        let files: Vec<_> = std::fs::read_dir(d.path().join("statusline"))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(files.len(), 1);
        let text = std::fs::read_to_string(files[0].path()).unwrap();
        assert!(!text.contains("/private"), "private fields are not persisted");
        assert!(files[0].file_name().to_string_lossy().ends_with(".json"));
    }

    #[test]
    fn garbage_input_is_swallowed() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(handle("not json", d.path()), "");
        assert!(!d.path().join("statusline").exists());
    }
}
