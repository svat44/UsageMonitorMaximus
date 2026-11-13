import { describe, expect, it } from "vitest";
import { describeMeasurement, formatRelative, formatValue } from "./format";
import { measurement } from "@/mocks/fixtures";

describe("formatValue", () => {
  it("formats each unit", () => {
    expect(formatValue(23.5, "percent")).toBe("23.5%");
    expect(formatValue(8, "percent")).toBe("8%");
    expect(formatValue(0.0123, "usd")).toBe("$0.01");
    expect(formatValue(120000, "tokens")).toBe("120,000");
    expect(formatValue(2, "count")).toBe("2");
  });
});

describe("describeMeasurement", () => {
  it("never renders an unavailable measurement as zero", () => {
    const m = measurement({ id: "subscription_five_hour_utilization", category: "subscription_usage", unit: "percent", value: { state: "unavailable", reason: "no_data_yet" } });
    const text = describeMeasurement(m);
    expect(text).toBe("No data yet");
    expect(text).not.toMatch(/0/);
  });
  it("renders a real zero as zero", () => {
    const m = measurement({ id: "local_active_sessions", category: "local_claude_code", unit: "count", value: { state: "available", value: 0 } });
    expect(describeMeasurement(m)).toBe("0");
  });
});

describe("formatRelative", () => {
  const now = new Date("2026-06-01T12:00:00Z");
  it("handles future and past", () => {
    expect(formatRelative("2026-06-01T14:14:00Z", now)).toBe("in 2h 14m");
    expect(formatRelative("2026-06-04T12:00:00Z", now)).toBe("in 3d 0h");
    expect(formatRelative("2026-06-01T11:55:00Z", now)).toBe("5m ago");
    expect(formatRelative("2026-06-01T12:00:30Z", now)).toBe("in 30s");
  });
});
