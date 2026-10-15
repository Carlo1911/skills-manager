import type { ManagedSkill } from "./tauri";

export type SkillLibraryFilter = "all" | "enabled" | "available" | "updateable";

type FilterableSkill = Pick<ManagedSkill, "source_type" | "source_ref" | "update_status" | "preset_ids" | "enabled">;

export function canRefresh(skill: Pick<ManagedSkill, "source_type" | "source_ref">): boolean {
  return skill.source_type === "git" ||
    skill.source_type === "skillssh" ||
    ((skill.source_type === "local" || skill.source_type === "import") && !!skill.source_ref);
}

export function matchesSkillLibraryFilter(
  skill: FilterableSkill,
  mode: SkillLibraryFilter,
): boolean {
  // Updateability is independent of whether a preset exists or is enabled.
  if (mode === "updateable") {
    return skill.update_status === "update_available" && canRefresh(skill);
  }
  // Per-device design: filters measure this device's on/off switch
  // (`skill.enabled`), not preset membership.
  if (mode === "enabled") return skill.enabled;
  if (mode === "available") return !skill.enabled;
  return true;
}
