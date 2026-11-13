import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { EVENTS } from "./events";

describe("event contract", () => {
  it("matches the event names declared in the Rust core", () => {
    const rust = readFileSync("crates/usage-core/src/events.rs", "utf8");
    const declared = [...rust.matchAll(/pub const EVENT_[A-Z_]+: &str = "([^"]+)";/g)].map((m) => m[1]).sort();
    expect(declared.length).toBe(7);
    expect(Object.values(EVENTS).sort()).toEqual(declared);
  });
});
