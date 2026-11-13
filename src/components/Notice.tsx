import type { ReactNode } from "react";
import { AlertTriangle, Info, ShieldAlert } from "lucide-react";

const STYLE = {
  info: { box: "border-sky-800 bg-sky-950/50 text-sky-200", Icon: Info },
  warn: { box: "border-amber-800 bg-amber-950/50 text-amber-200", Icon: AlertTriangle },
  bad: { box: "border-red-800 bg-red-950/50 text-red-200", Icon: ShieldAlert },
} as const;

export function Notice({ tone, title, children }: { tone: keyof typeof STYLE; title: string; children?: ReactNode }) {
  const { box, Icon } = STYLE[tone];
  return (
    <div role="status" className={`flex gap-3 rounded-lg border p-3 text-sm ${box}`}>
      <Icon className="mt-0.5 h-4 w-4 shrink-0" aria-hidden />
      <div>
        <div className="font-medium">{title}</div>
        {children && <div className="mt-0.5 text-[13px] opacity-90">{children}</div>}
      </div>
    </div>
  );
}
