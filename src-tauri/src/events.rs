//! Bridges the core engine's [`EventSink`] to Tauri's native event system.

use tauri::{AppHandle, Emitter};
use usage_core::events::{EventSink, MonitoringEvent};

pub struct TauriEventSink(pub AppHandle);

impl EventSink for TauriEventSink {
    fn emit(&self, event: &MonitoringEvent) {
        if let Err(e) = self.0.emit(event.name(), event.payload()) {
            tracing::warn!(
                operation = "emit_event",
                category = "internal",
                "failed to emit {}: {e}",
                event.name()
            );
        }
    }
}
