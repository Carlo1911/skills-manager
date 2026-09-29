export type SkillUpdateStatus =
  | "pending"
  | "checking"
  | "updating"
  | "updated"
  | "unchanged"
  | "held_back"
  | "failed";

export interface SkillUpdateItem {
  id: string;
  name: string;
  status: SkillUpdateStatus;
  error: string | null;
}

export interface SkillUpdateProgress {
  run_id: string;
  items: SkillUpdateItem[];
  finished: boolean;
}

export function completedSkillCount(run: SkillUpdateProgress): number {
  return run.items.filter((item) => item.status !== "pending" && item.status !== "checking" && item.status !== "updating").length;
}

/** Keep concurrent operations separate and bound retained completed history. */
export function receiveSkillUpdateProgress(
  runs: SkillUpdateProgress[],
  incoming: SkillUpdateProgress,
): SkillUpdateProgress[] {
  if (incoming.items.length === 0) return runs;
  const index = runs.findIndex((run) => run.run_id === incoming.run_id);
  const next = index < 0
    ? [...runs, incoming]
    : runs.map((run) => run.run_id === incoming.run_id ? incoming : run);
  const retained = new Set(next.filter((run) => run.finished).slice(-3).map((run) => run.run_id));
  return next.filter((run) => !run.finished || retained.has(run.run_id));
}
