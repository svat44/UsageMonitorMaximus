import { Badge } from "@/components/Badge";
import { MeasurementRow } from "@/components/MeasurementRow";
import { CATEGORY_INFO } from "@/utils/labels";
import type { CoverageState, Measurement, MeasurementCategory } from "@/types";

const COVERAGE_BADGE: Record<CoverageState, { tone: "good" | "warn" | "neutral" | "info"; text: string }> = {
  live: { tone: "good", text: "Live" },
  stale: { tone: "warn", text: "Stale" },
  simulated: { tone: "warn", text: "Simulated" },
  not_connected: { tone: "neutral", text: "Not connected" },
};

export function CategorySection({
  category,
  coverage,
  measurements,
  now,
}: {
  category: MeasurementCategory;
  coverage: CoverageState;
  measurements: Measurement[];
  now: Date;
}) {
  const info = CATEGORY_INFO[category];
  const badge = COVERAGE_BADGE[coverage];
  return (
    <section aria-labelledby={`cat-${category}`} className="rounded-xl border border-zinc-800 bg-zinc-900/60 p-4" data-testid={`cat-${category}`}>
      <div className="flex items-center justify-between gap-3">
        <h2 id={`cat-${category}`} className="text-sm font-semibold uppercase tracking-wide text-zinc-300">{info.title}</h2>
        <Badge tone={badge.tone}>{badge.text}</Badge>
      </div>
      <p className="mt-1 text-xs text-zinc-500">{info.blurb}</p>
      {measurements.length === 0 ? (
        <p className="mt-3 text-sm italic text-zinc-500">Nothing reported for this category.</p>
      ) : (
        <ul className="mt-2">{measurements.map((m) => <MeasurementRow key={`${m.id}/${m.scope ?? ""}`} m={m} now={now} />)}</ul>
      )}
    </section>
  );
}
