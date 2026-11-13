import type { MeasurementCategory, MeasurementId } from "@/types";

export const MEASUREMENT_LABEL: Record<MeasurementId, string> = {
  subscription_five_hour_utilization: "5-hour window used",
  subscription_weekly_utilization: "Weekly window used",
  api_input_tokens: "API input tokens",
  api_output_tokens: "API output tokens",
  local_context_window_utilization: "Context window used",
  local_session_cost_estimate: "Session cost (estimate)",
  local_active_sessions: "Active Claude Code sessions",
};

export const CATEGORY_INFO: Record<MeasurementCategory, { title: string; blurb: string }> = {
  subscription_usage: {
    title: "Claude subscription usage",
    blurb: "Plan limits on your Claude account (5-hour and weekly windows).",
  },
  local_claude_code: {
    title: "Local Claude Code activity",
    blurb: "Observed from Claude Code on this computer. The context window is not your plan quota.",
  },
  api_consumption: {
    title: "Anthropic API consumption",
    blurb: "Tokens and cost billed to an API organisation. Separate from subscription usage.",
  },
};

export const CATEGORY_ORDER: MeasurementCategory[] = ["subscription_usage", "local_claude_code", "api_consumption"];
