import type { ReactElement } from "react";
import { Notice } from "@/components/Notice";
import type { MonitoringSnapshot } from "@/types";

/** Translates backend state into plain statements. Never claims more than the data supports. */
export function StatusNotices({ s }: { s: MonitoringSnapshot }) {
  const sub = s.coverage.find((c) => c.category === "subscription_usage")?.state;
  const items: ReactElement[] = [];

  if (s.simulated) {
    items.push(
      <Notice key="sim" tone="warn" title="Simulated data">
        These numbers are generated for development. They are not your real Claude usage and do not mean subscription monitoring is connected.
      </Notice>,
    );
  } else if (sub === "not_connected") {
    items.push(
      <Notice key="sub" tone="info" title="Subscription usage is not connected yet">
        Real 5-hour and weekly usage only reaches this app through Claude Code's status line, for Pro/Max/Ultra subscribers, while a Claude Code
        session is active. Set it up under “Connect Claude Code” below, then send a prompt in Claude Code.
      </Notice>,
    );
  }

  switch (s.connection) {
    case "authentication_required":
      items.push(<Notice key="auth" tone="bad" title="Authentication required">{s.latestError?.message}</Notice>);
      break;
    case "rate_limited":
      items.push(
        <Notice key="rl" tone="warn" title="Rate limited">
          Requests are paused until {s.rateLimitedUntil ? new Date(s.rateLimitedUntil).toLocaleTimeString() : "the provider allows it"}. Showing last known values.
        </Notice>,
      );
      break;
    case "temporarily_unavailable":
      items.push(<Notice key="tu" tone="warn" title="Source temporarily unavailable">Showing last known values, marked stale. {s.latestError?.message}</Notice>);
      break;
    case "permanently_unsupported":
      items.push(<Notice key="pu" tone="bad" title="This provider is not supported here">{s.latestError?.message}</Notice>);
      break;
    case "stale":
      items.push(<Notice key="st" tone="warn" title="Data is out of date">The source has not reported recently (Claude Code only reports while you are using it).</Notice>);
      break;
  }
  if (items.length === 0) return null;
  return <div className="space-y-2">{items}</div>;
}
