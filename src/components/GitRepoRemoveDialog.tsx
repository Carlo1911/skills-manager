import { useEffect, useRef } from "react";
import { AlertTriangle, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";

interface Props {
  repo: { key: string; skills: { id: string; name: string }[] } | null;
  removeCache: boolean;
  onRemoveCacheChange: (value: boolean) => void;
  busy: boolean;
  onClose: () => void;
  onConfirm: () => Promise<void>;
}

/** Native modal provides focus trapping and restores focus to its trigger. */
export function GitRepoRemoveDialog({ repo, removeCache, onRemoveCacheChange, busy, onClose, onConfirm }: Props) {
  const { t } = useTranslation();
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const dialog = ref.current;
    if (repo && dialog && !dialog.open) dialog.showModal();
    if (!repo && dialog?.open) dialog.close();
  }, [repo]);

  return (
    <dialog ref={ref} aria-labelledby="remove-git-repo-title"
      onCancel={(event) => { event.preventDefault(); if (!busy) onClose(); }}
      className="m-auto w-full max-w-lg rounded-xl border border-border bg-surface p-5 text-secondary shadow-2xl backdrop:bg-black/70 max-h-[calc(85vh/var(--app-scale))] overflow-y-auto">
      {repo && <>
        <h2 id="remove-git-repo-title" className="flex items-center gap-2 text-[14px] font-semibold">
          <AlertTriangle className="h-4 w-4 text-amber-600 dark:text-amber-400" />{t("install.gitRepos.confirmTitle")}
        </h2>
        <p className="mt-3 break-all font-mono text-[13px]">{repo.key}</p>
        <p className="mt-3 text-[13px] text-muted">{t("install.gitRepos.confirmDescription", { count: repo.skills.length })}</p>
        <ul className="my-4 max-h-40 overflow-y-auto text-[13px]">
          {repo.skills.map((skill) => <li key={skill.id}>{skill.name}</li>)}
        </ul>
        <label className="flex items-start gap-2 text-[13px]">
          <input type="checkbox" checked={removeCache} disabled={busy}
            onChange={(event) => onRemoveCacheChange(event.target.checked)} className="mt-1 accent-accent" />
          {t("install.gitRepos.removeCache")}
        </label>
        <p className="mt-2 text-[12px] text-muted">{t("install.gitRepos.safeScope")}</p>
        <div className="mt-5 flex justify-end gap-2">
          <button autoFocus onClick={onClose} disabled={busy} className="app-button-secondary">{t("common.cancel")}</button>
          <button onClick={() => void onConfirm()} disabled={busy}
            className="inline-flex items-center gap-2 rounded-lg bg-red-600 px-3 py-2 text-[13px] font-medium text-white hover:bg-red-500 disabled:opacity-50">
            {busy && <Loader2 className="h-4 w-4 animate-spin" />}
            {busy ? t("common.loading") : t("install.gitRepos.confirmRemove", { count: repo.skills.length })}
          </button>
        </div>
      </>}
    </dialog>
  );
}
