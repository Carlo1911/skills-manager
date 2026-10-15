import { useEffect, useMemo, useState } from "react";
import { CloudDownload, Loader2, Square, SquareCheck, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "../utils";

export interface RestoreSelectSkill {
  id: string;
  name: string;
  enabled: boolean;
}

interface Props {
  open: boolean;
  skills: RestoreSelectSkill[];
  busy: boolean;
  onClose: () => void;
  /** Ids the user left unchecked (to hide on this device). */
  onConfirm: (disabledIds: string[]) => Promise<void>;
}

/**
 * Post-restore picker (per-device enabled, opcion C): after a clone brings in
 * the backup's skills, everything is enabled here by default and the user
 * unchecks what should stay hidden on this device. Already-hidden skills
 * start unchecked so a re-clone doesn't silently re-show them.
 */
export function RestoreSelectDialog({ open, skills, busy, onClose, onConfirm }: Props) {
  const { t } = useTranslation();
  // Fresh mount per prompt (parent passes a new `key`), so lazy init is
  // enough — no reseed effect or render-time setState needed.
  const [disabled, setDisabled] = useState<Set<string>>(
    () => new Set(skills.filter((s) => !s.enabled).map((s) => s.id))
  );

  useEffect(() => {
    if (!open || busy) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, busy, onClose]);

  const rows = useMemo(
    () => [...skills].sort((a, b) => a.name.localeCompare(b.name)),
    [skills]
  );

  if (!open) return null;

  const allChecked = disabled.size === 0;
  const toggleAll = () => {
    if (busy) return;
    setDisabled(allChecked ? new Set(skills.map((s) => s.id)) : new Set());
  };
  const toggle = (id: string) => {
    if (busy) return;
    setDisabled((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };
  const enabledCount = skills.length - disabled.size;

  const close = () => {
    if (!busy) onClose();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/70 backdrop-blur-sm" onClick={close} />
      <div className="relative w-full max-w-[440px] rounded-xl border border-border bg-surface p-5 shadow-2xl">
        <div className="mb-3 flex items-center justify-between">
          <h2 className="flex items-center gap-2 text-[13px] font-semibold text-primary">
            <CloudDownload className="h-4 w-4 text-accent-light" />
            {t("backup.restoreSelect.title")}
          </h2>
          <button
            onClick={close}
            disabled={busy}
            className="rounded-md p-1 text-muted outline-none transition-colors hover:text-secondary disabled:opacity-50"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <p className="mb-3 text-[12px] leading-5 text-muted">
          {t("backup.restoreSelect.message")}
        </p>

        <button
          type="button"
          onClick={toggleAll}
          disabled={busy}
          className="mb-2 inline-flex items-center gap-1.5 rounded-md px-1 py-0.5 text-[12px] font-medium text-muted outline-none transition-colors hover:text-secondary disabled:opacity-50"
        >
          {allChecked
            ? <SquareCheck className="h-3.5 w-3.5 text-accent" />
            : <Square className="h-3.5 w-3.5 text-faint" />}
          {allChecked ? t("mySkills.deselectAll") : t("mySkills.selectAll")}
        </button>

        <div className="grid max-h-[320px] gap-1.5 overflow-y-auto pr-0.5">
          {rows.map((skill) => {
            const checked = !disabled.has(skill.id);
            return (
              <button
                key={skill.id}
                type="button"
                onClick={() => toggle(skill.id)}
                disabled={busy}
                className={cn(
                  "flex w-full items-center gap-2 rounded-md border px-2 py-1.5 text-left text-[12px] transition-colors",
                  checked ? "border-border bg-surface" : "border-border-subtle bg-bg-secondary",
                  busy ? "opacity-55" : "hover:bg-surface-hover"
                )}
              >
                <span className="shrink-0">
                  {checked
                    ? <SquareCheck className="h-3.5 w-3.5 text-accent" />
                    : <Square className="h-3.5 w-3.5 text-faint" />}
                </span>
                <span className="min-w-0 flex-1 truncate font-medium text-secondary">
                  {skill.name}
                </span>
              </button>
            );
          })}
        </div>

        <p className="mt-3 text-[12px] text-faint">
          {t("backup.restoreSelect.selection", { enabled: enabledCount, hidden: disabled.size })}
        </p>

        <div className="flex justify-end gap-2 pt-4">
          <button
            onClick={close}
            disabled={busy}
            className="rounded-lg px-3 py-1.5 text-[13px] font-medium text-tertiary outline-none transition-colors hover:bg-surface-hover hover:text-secondary disabled:opacity-50"
          >
            {t("common.cancel")}
          </button>
          <button
            onClick={() => onConfirm(Array.from(disabled))}
            disabled={busy}
            className="inline-flex items-center gap-1.5 rounded-lg border border-accent-border bg-accent-dark px-3 py-1.5 text-[13px] font-medium text-white outline-none transition-colors hover:bg-accent disabled:cursor-not-allowed disabled:opacity-50"
          >
            {busy && <Loader2 className="h-3.5 w-3.5 animate-spin" />}
            {busy ? t("common.loading") : t("backup.restoreSelect.confirm")}
          </button>
        </div>
      </div>
    </div>
  );
}

/* Hook lives in src/hooks/useRestoreSelect.tsx (fast-refresh: one component per file). */
