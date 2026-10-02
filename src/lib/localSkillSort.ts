import type { DiscoveredGroup } from "./tauri";

export type LocalSkillSort = "name" | "date" | "agent" | "location";
export type SortDirection = "asc" | "desc";

const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });
const compareText = (a: string, b: string) => collator.compare(a, b) || (a < b ? -1 : a > b ? 1 : 0);

function compareLists(a: string[], b: string[]): number {
  for (let i = 0; i < Math.min(a.length, b.length); i++) {
    const result = compareText(a[i], b[i]);
    if (result) return result;
  }
  return a.length - b.length;
}

/** Sort a copy: never change scan order or the source selected for import. */
export function sortLocalSkills(
  groups: readonly DiscoveredGroup[],
  sort: LocalSkillSort,
  direction: SortDirection,
): DiscoveredGroup[] {
  const entries = groups.map((group) => ({
    group,
    agents: [...new Set(group.locations.map((location) => location.tool))].sort(compareText),
    paths: group.locations.map((location) => location.found_path).sort(compareText),
  }));
  return entries.sort((a, b) => {
    const primary = sort === "date" ? a.group.found_at - b.group.found_at
      : sort === "agent" ? compareLists(a.agents, b.agents)
      : sort === "location" ? compareLists(a.paths, b.paths)
      : compareText(a.group.name, b.group.name);
    // Ties always use ascending name/content/paths, independent of scan order.
    return primary * (direction === "asc" ? 1 : -1)
      || compareText(a.group.name, b.group.name)
      || compareText(a.group.fingerprint ?? "", b.group.fingerprint ?? "")
      || compareLists(a.paths, b.paths);
  }).map(({ group }) => group);
}

/** Keep each agent/path pair intact while arranging locations for display. */
export function sortLocalLocations(group: DiscoveredGroup, sort: LocalSkillSort) {
  return [...group.locations].sort((a, b) => sort === "location"
    ? compareText(a.found_path, b.found_path) || compareText(a.tool, b.tool)
    : compareText(a.tool, b.tool) || compareText(a.found_path, b.found_path));
}
