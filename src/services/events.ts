/** Event names emitted by the Rust backend. Kept in sync with crates/usage-core/src/events.rs
 *  (a unit test parses the Rust file and fails on drift). */
export const EVENTS = {
  started: "monitoring://started",
  updated: "monitoring://updated",
  providerStatusChanged: "monitoring://provider-status-changed",
  authenticationRequired: "monitoring://authentication-required",
  rateLimited: "monitoring://rate-limited",
  error: "monitoring://error",
  stopped: "monitoring://stopped",
} as const;
