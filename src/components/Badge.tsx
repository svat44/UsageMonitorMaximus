import type { ReactNode } from "react";

const TONES = {
  neutral: "bg-zinc-800 text-zinc-300 ring-zinc-700",
  good: "bg-emerald-950 text-emerald-300 ring-emerald-800",
  warn: "bg-amber-950 text-amber-300 ring-amber-800",
  bad: "bg-red-950 text-red-300 ring-red-800",
  info: "bg-sky-950 text-sky-300 ring-sky-800",
} as const;

export function Badge({ tone = "neutral", children, title }: { tone?: keyof typeof TONES; children: ReactNode; title?: string }) {
  return (
    <span title={title} className={`inline-flex items-center rounded px-1.5 py-0.5 text-[11px] font-medium ring-1 ring-inset ${TONES[tone]}`}>
      {children}
    </span>
  );
}
