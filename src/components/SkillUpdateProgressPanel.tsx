import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { AlertTriangle, CheckCircle2, Circle, Loader2, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "../utils";
import {
  completedSkillCount,
  receiveSkillUpdateProgress,
  type SkillUpdateProgress,
} from "../lib/skillUpdateProgress";

/** Lives in Layout: progress stays visible while navigating or filtering skills. */
export function SkillUpdateProgressPanel() {
  const { t } = useTranslation();
  const [runs, setRuns] = useState<SkillUpdateProgress[]>([]);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    listen<SkillUpdateProgress>("skill-update-progress", ({ payload }) => {
      if (!disposed) setRuns((current) => receiveSkillUpdateProgress(current, payload));
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unsubscribe = unlisten;
    }).catch((error) => console.error("Could not listen for skill update progress:", error));
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, []);

  if (runs.length === 0) return null;

  return (
    <aside
      aria-label={t("skillUpdateProgress.title")}
      className="fixed bottom-5 right-5 z-40 flex max-h-[60vh] w-[380px] max-w-[calc(100vw-40px)] flex-col gap-3 overflow-y-auto rounded-xl border border-border-subtle bg-bg-secondary p-3 shadow-xl"
    >
      {runs.map((run) => {
        const completed = completedSkillCount(run);
        const percent = Math.round((completed / run.items.length) * 100);
        return (
          <section key={run.run_id} className="min-w-0">
            <div className="flex items-start justify-between gap-2">
              <div className="min-w-0">
                <p className="flex items-center gap-2 text-[13px] font-semibold text-primary" role="status">
                  {!run.finished && <Loader2 aria-hidden="true" className="h-4 w-4 shrink-0 animate-spin" />}
                  {t(run.finished ? "skillUpdateProgress.finished" : "skillUpdateProgress.title")}
                </p>
                <p className="mt-1 text-xs text-muted">
                  {t("skillUpdateProgress.count", { completed, total: run.items.length })}
                </p>
              </div>
              {run.finished && (
                <button
                  type="button"
                  aria-label={t("skillUpdateProgress.dismiss")}
                  onClick={() => setRuns((current) => current.filter((item) => item.run_id !== run.run_id))}
                  className="rounded p-1 text-muted hover:bg-surface-hover hover:text-primary"
                >
                  <X aria-hidden="true" className="h-4 w-4" />
                </button>
              )}
            </div>
            <div
              role="progressbar"
              aria-label={t("skillUpdateProgress.title")}
              aria-valuemin={0}
              aria-valuemax={run.items.length}
              aria-valuenow={completed}
              aria-valuetext={t("skillUpdateProgress.count", { completed, total: run.items.length })}
              className="my-3 h-1.5 overflow-hidden rounded-full bg-surface-hover"
            >
              <div className="h-full rounded-full bg-blue-500 transition-[width] duration-300" style={{ width: `${percent}%` }} />
            </div>
            <ul className="max-h-48 space-y-2 overflow-y-auto">
              {run.items.map((item) => {
                const warning = item.status === "held_back" || item.status === "failed";
                const Icon = item.status === "updating" || item.status === "checking" ? Loader2
                  : item.status === "pending" ? Circle
                  : warning ? AlertTriangle : CheckCircle2;
                return (
                  <li key={item.id} className="flex items-start gap-2 text-xs">
                    <Icon aria-hidden="true" className={cn(
                      "mt-0.5 h-3.5 w-3.5 shrink-0",
                      (item.status === "updating" || item.status === "checking") && "animate-spin text-blue-400",
                      warning ? "text-amber-400" : "text-muted",
                    )} />
                    <div className="min-w-0 flex-1">
                      <p className="break-words font-medium text-primary">{item.name}</p>
                      <p className={cn("mt-0.5 text-muted", warning && "text-amber-400")}>
                        {t(`skillUpdateProgress.status.${item.status}`)}
                      </p>
                      {item.error && <p className="mt-1 break-words text-muted">{item.error}</p>}
                    </div>
                  </li>
                );
              })}
            </ul>
          </section>
        );
      })}
    </aside>
  );
}
