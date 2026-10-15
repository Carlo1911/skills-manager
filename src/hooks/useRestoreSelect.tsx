import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useApp } from "../context/AppContext";
import * as api from "../lib/tauri";
import type { ManagedSkill } from "../lib/tauri";
import { RestoreSelectDialog, type RestoreSelectSkill } from "../components/RestoreSelectDialog";

/**
 * Shared prompt state for the two restore entries (Backup page + first-run
 * wizard), mirroring the `useSetAsideOffer` shape: a trigger plus a dialog
 * element the host renders. The skill list is fetched fresh at prompt time so
 * it reflects the just-restored library. Each prompt gets a fresh `key` so
 * the dialog remounts with current visibility, no reseed effect needed.
 */
export function useRestoreSelect() {
  const { t } = useTranslation();
  const { refreshManagedSkills, refreshPresets } = useApp();
  const [skills, setSkills] = useState<RestoreSelectSkill[] | null>(null);
  const [promptId, setPromptId] = useState(0);
  const [busy, setBusy] = useState(false);

  const promptRestoreSelect = useCallback(async () => {
    const list = await api.getManagedSkills().catch(() => [] as ManagedSkill[]);
    if (list.length === 0) return;
    setSkills(list.map((s) => ({ id: s.id, name: s.name, enabled: s.enabled })));
    setPromptId((n) => n + 1);
  }, []);

  const closeRestoreSelect = useCallback(() => {
    setSkills(null);
  }, []);

  const confirmRestoreSelect = useCallback(async (disabledIds: string[]) => {
    setBusy(true);
    try {
      for (const id of disabledIds) {
        try {
          await api.setSkillDeviceEnabled(id, false);
        } catch {
          // One failure must not strand the rest; the toggle stays
          // available on the card for a retry.
        }
      }
      await Promise.all([refreshManagedSkills(), refreshPresets()]);
      if (disabledIds.length > 0) {
        toast.success(t("backup.restoreSelect.applied", { count: disabledIds.length }));
      }
    } finally {
      setBusy(false);
      setSkills(null);
    }
  }, [refreshManagedSkills, refreshPresets, t]);

  const restoreSelectDialog = (
    <RestoreSelectDialog
      key={promptId}
      open={skills !== null}
      skills={skills ?? []}
      busy={busy}
      onClose={closeRestoreSelect}
      onConfirm={confirmRestoreSelect}
    />
  );

  return { promptRestoreSelect, restoreSelectDialog };
}
