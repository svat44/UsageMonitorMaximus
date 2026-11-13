import type { Measurement, UnavailableReason, Unit } from "@/types";

export function formatValue(value: number, unit: Unit): string {
  switch (unit) {
    case "percent":
      return `${Number.isInteger(value) ? value : value.toFixed(1)}%`;
    case "usd":
      return `$${value.toFixed(2)}`;
    case "tokens":
      return Math.round(value).toLocaleString("en-US");
    case "count":
      return String(Math.round(value));
  }
}

export const UNAVAILABLE_TEXT: Record<UnavailableReason, string> = {
  not_supported_by_provider: "Not supported by this provider",
  no_data_yet: "No data yet",
  not_reported_by_source: "Not reported by the source",
  window_expired: "Window reset — new value unknown",
  not_reported: "Not reported",
};

/** Text shown in place of a value. A missing measurement is never rendered as 0. */
export function describeMeasurement(m: Measurement): string {
  return m.value.state === "available" ? formatValue(m.value.value, m.unit) : UNAVAILABLE_TEXT[m.value.reason];
}

export function formatRelative(targetIso: string, now: Date): string {
  const secs = Math.round((new Date(targetIso).getTime() - now.getTime()) / 1000);
  const abs = Math.abs(secs);
  const d = Math.floor(abs / 86400);
  const h = Math.floor((abs % 86400) / 3600);
  const m = Math.floor((abs % 3600) / 60);
  const body = d > 0 ? `${d}d ${h}h` : h > 0 ? `${h}h ${m}m` : m > 0 ? `${m}m` : `${abs}s`;
  return secs >= 0 ? `in ${body}` : `${body} ago`;
}
