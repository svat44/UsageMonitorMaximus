import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import "./index.css";

async function boot() {
  // Browser-only mock backend for Playwright/dev without Tauri (never included in production builds).
  if (import.meta.env.VITE_MOCK_TAURI === "1") {
    const { installTauriMock } = await import("./mocks/tauriMock");
    installTauriMock();
  }
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false } } });
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <QueryClientProvider client={queryClient}>
        <App />
      </QueryClientProvider>
    </React.StrictMode>,
  );
}
void boot();
