import { expect, test, type Page } from "@playwright/test";

async function emit(page: Page, scenario: "live" | "waiting" | "simulated") {
  await page.evaluate(async (name) => {
    await window.__mock.emitSnapshot(window.__mock.scenarios[name]!());
  }, scenario);
}

test("renders live data in the right categories", async ({ page }) => {
  await page.goto("/");
  const sub = page.getByTestId("cat-subscription_usage");
  await expect(sub.getByTestId("v-subscription_five_hour_utilization")).toHaveText("23.5%");
  await expect(sub.getByTestId("v-subscription_weekly_utilization")).toHaveText("41.2%");
  await expect(sub.getByText("Live")).toBeVisible();
  const local = page.getByTestId("cat-local_claude_code");
  await expect(local.getByTestId("v-local_context_window_utilization")).toHaveText("8%");
  await expect(sub.getByTestId("v-local_context_window_utilization")).toHaveCount(0);
  await expect(page.getByTestId("cat-api_consumption").getByText("Not connected")).toBeVisible();
});

test("backend events update the UI without a reload; unavailable is not zero", async ({ page }) => {
  await page.goto("/?scenario=waiting");
  await expect(page.getByText("Subscription usage is not connected yet")).toBeVisible();
  await expect(page.getByTestId("v-subscription_five_hour_utilization")).toHaveText("No data yet");

  await emit(page, "live");
  await expect(page.getByTestId("v-subscription_five_hour_utilization")).toHaveText("23.5%");
  await expect(page.getByText("Subscription usage is not connected yet")).toHaveCount(0);

  await emit(page, "simulated");
  await expect(page.getByText("Simulated data", { exact: true })).toBeVisible();
});

test("settings: connect snippet is shown and the monitoring toggle talks to the backend", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByTestId("settings-snippet")).toContainText('"statusLine"');
  const sw = page.getByRole("switch", { name: "Monitoring enabled" });
  await expect(sw).toBeChecked();
  await expect(page.getByLabel("Data source")).toHaveValue("claude-code-statusline");
  await page.getByLabel("Data source").selectOption("simulated");
  await expect(page.getByLabel("Data source")).toHaveValue("simulated");
});
