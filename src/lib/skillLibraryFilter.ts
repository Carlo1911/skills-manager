import type { ManagedSkill } from "./tauri";

export type SkillLibraryFilter = "all" | "enabled" | "available" | "updateable";

type FilterableSkill = Pick<ManagedSkill, "source_type" | "source_ref" | "update_status" | "preset_ids">;

export function canRefresh(skill: Pick<ManagedSkill, "source_type" | "source_ref">): boolean {
  return skill.source_type === "git" ||
    skill.source_type === "skillssh" ||
    ((skill.source_type === "local" || skill.source_type === "import") && !!skill.source_ref);
}

export function matchesSkillLibraryFilter(
  skill: FilterableSkill,
  mode: SkillLibraryFilter,
  presetId: string | null,
): boolean {
  // Updateability is independent of whether a preset exists or is enabled.
  if (mode === "updateable") {
    return skill.update_status === "update_available" && canRefresh(skill);
  }
  if (!presetId) return true;
  const enabled = skill.preset_ids.includes(presetId);
  if (mode === "enabled") return enabled;
  if (mode === "available") return !enabled;
  return true;
}
