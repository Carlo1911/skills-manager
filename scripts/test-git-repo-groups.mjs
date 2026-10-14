import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { transpileModule, ModuleKind, ScriptTarget } from "typescript";

const source = readFileSync(new URL("../src/lib/skillSource.ts", import.meta.url), "utf8");
const { outputText } = transpileModule(source, {
  compilerOptions: { module: ModuleKind.ESNext, target: ScriptTarget.ES2022 },
});
const { buildRepoGroups, skillRepoKey } = await import(
  `data:text/javascript;base64,${Buffer.from(outputText).toString("base64")}`
);
const refs = [
  "https://github.com/acme/skills.git",
  "git@github.com:acme/skills.git",
  "https://github.com/acme/skills/tree/main/skills/one",
  "ssh://git@github.com/acme/skills.git",
  "acme/skills",
];
const skills = refs.map((source_ref, i) => ({ id: String(i), source_type: "git", source_ref }));
skills.push({ id: "market", source_type: "skillssh", source_ref: "acme/skills@two" });
skills.push({ id: "other", source_type: "git", source_ref: "https://github.com/other/skills.git" });
skills.push({ id: "local", source_type: "local", source_ref: "/Projects/acme/skills/one" });
for (const skill of skills.slice(0, 6)) assert.equal(skillRepoKey(skill), "github.com/acme/skills");
const groups = buildRepoGroups(skills, "Unknown");
const acme = groups.find((group) => group.key === "github.com/acme/skills");
assert.equal(acme.count, 6);
assert.equal(acme.kind, "git");
const removalIds = skills.filter((skill) => ["git", "skillssh"].includes(skill.source_type) && skillRepoKey(skill) === acme.key).map((skill) => skill.id);
assert.deepEqual(removalIds, ["0", "1", "2", "3", "4", "market"]);
assert.equal(skillRepoKey({ source_type: "git", source_ref: null, source_ref_resolved: refs[0] }), acme.key);
assert.equal(skillRepoKey({ source_type: "git", source_ref: null }), "unknown");
for (const lang of ["en", "es"]) {
  const locale = JSON.parse(readFileSync(new URL(`../src/i18n/${lang}.json`, import.meta.url), "utf8"));
  for (const key of ["title", "viewSkills", "confirmDescription", "confirmRemove", "removeCache", "safeScope", "removePartial", "cacheFailed"]) {
    assert.ok(locale.install.gitRepos[key], `${lang}: missing ${key}`);
  }
}
console.log("Git repository grouping and removal-scope tests passed.");
