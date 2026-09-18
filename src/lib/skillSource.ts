import type { ManagedSkill } from "./tauri";

export type RepoGroupKind = "git" | "local" | "unknown";

export interface RepoGroup {
  key: string;
  label: string;
  host: string | null;
  kind: RepoGroupKind;
  count: number;
}

const GITHUB_HOST = "github.com";
const UNKNOWN_KEY = "unknown";

/**
 * Canonical repo key for a git URL in any form the library stores:
 * clone URLs (`https://github.com/owner/repo.git`), scp shorthand
 * (`git@github.com:owner/repo.git`), bare shorthand (`owner/repo`), and
 * GitHub tree URLs (`…/tree/<branch>/<subpath>`) from single-skill installs.
 * Returns null when the ref carries no usable owner/repo.
 */
export function parseGitRepo(ref: string): { host: string; path: string } | null {
  const s = ref.trim();
  if (!s) return null;

  // scp-like `[user@]host:path` — no `://`, so a Windows drive letter can
  // never reach here (local paths never route to this parser).
  const scp = s.match(/^(?:[^@/:\s]+@)?([^/:\s]+):(.+)$/);
  if (scp && !s.includes("://")) {
    const parts = scp[2].replace(/\.git$/, "").split("/").filter(Boolean);
    if (parts.length < 2) return null;
    return { host: scp[1].toLowerCase(), path: `${parts[0]}/${parts[1]}` };
  }

  if (!/^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(s)) {
    // Bare `owner/repo[/…]` shorthand — the backend resolves it to GitHub.
    const parts = s.split("/").filter(Boolean);
    if (parts.length < 2) return null;
    return { host: GITHUB_HOST, path: `${parts[0]}/${parts[1]}` };
  }

  let url: URL;
  try {
    url = new URL(s);
  } catch {
    return null;
  }
  const segs = url.pathname.split("/").filter(Boolean);
  if (segs.length < 2) return null;
  const repo = segs[1].replace(/\.git$/, "");
  if (!segs[0] || !repo) return null;
  // `…/tree/<branch>/<subpath>` names one skill but belongs to the repo.
  return { host: url.hostname.toLowerCase(), path: `${segs[0]}/${repo}` };
}

/**
 * Repo key for the Library filter. Git accepts clone/scp/shorthand/tree
 * URLs via `parseGitRepo`; skills.sh accepts `owner/repo/skill[-@skill]`
 * and falls back to the stored clone URL.
 */
export function skillRepoKey(skill: ManagedSkill): string {
  if (skill.source_type === "git") {
    for (const ref of [skill.source_ref, skill.source_ref_resolved]) {
      if (!ref) continue;
      const parsed = parseGitRepo(ref);
      if (parsed) return `${parsed.host}/${parsed.path}`.toLowerCase();
    }
    return UNKNOWN_KEY;
  }
  if (skill.source_type === "skillssh") {
    const ref = (skill.source_ref ?? "").trim();
    if (ref) {
      const parts = ref.split("@")[0].split("/").filter(Boolean);
      if (parts.length >= 2) return `${GITHUB_HOST}/${parts[0]}/${parts[1]}`.toLowerCase();
    }
    if (skill.source_ref_resolved) {
      const parsed = parseGitRepo(skill.source_ref_resolved);
      if (parsed) return `${parsed.host}/${parsed.path}`.toLowerCase();
    }
    return UNKNOWN_KEY;
  }
  if (skill.source_type === "local" || skill.source_type === "import") {
    const segs = (skill.source_ref ?? "").split(/[\\/]/).filter(Boolean);
    const parent = segs.length >= 2 ? segs[segs.length - 2] : null;
    return `local/${(parent || "local").toLowerCase()}`;
  }
  return UNKNOWN_KEY;
}

/**
 * Count skills per repo key. Labels are the short repo/folder name, upgraded
 * to the full key only when two groups would otherwise share a label.
 */
export function buildRepoGroups(skills: ManagedSkill[], unknownLabel: string): RepoGroup[] {
  const counts = new Map<string, { count: number; kind: RepoGroupKind; host: string | null }>();
  for (const skill of skills) {
    const key = skillRepoKey(skill);
    const kind: RepoGroupKind =
      skill.source_type === "git" || skill.source_type === "skillssh"
        ? "git"
        : skill.source_type === "local" || skill.source_type === "import"
          ? "local"
          : "unknown";
    const host = kind === "git" ? key.split("/")[0] : null;
    const entry = counts.get(key);
    if (entry) entry.count += 1;
    else counts.set(key, { count: 1, kind, host });
  }

  const shortOf = (key: string) =>
    key === UNKNOWN_KEY ? unknownLabel : key.split("/").pop() || key;
  const seen = new Map<string, number>();
  for (const key of counts.keys()) {
    const short = shortOf(key).toLowerCase();
    seen.set(short, (seen.get(short) ?? 0) + 1);
  }

  return [...counts.entries()]
    .map(([key, { count, kind, host }]) => ({
      key,
      label: seen.get(shortOf(key).toLowerCase())! > 1 ? key : shortOf(key),
      host,
      kind,
      count,
    }))
    .sort((a, b) => b.count - a.count || a.label.localeCompare(b.label));
}
