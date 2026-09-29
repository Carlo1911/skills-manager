import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { transpileModule, ModuleKind, ScriptTarget } from "typescript";

const source = readFileSync(new URL("../src/lib/skillLibraryFilter.ts", import.meta.url), "utf8");
const { outputText } = transpileModule(source, {
  compilerOptions: { module: ModuleKind.ESNext, target: ScriptTarget.ES2022 },
});
const { matchesSkillLibraryFilter, canRefresh } = await import(
  `data:text/javascript;base64,${Buffer.from(outputText).toString("base64")}`
);
const base = { source_type: "git", source_ref: null, update_status: "update_available", preset_ids: ["active"] };

for (const source_type of ["git", "skillssh", "local", "import"]) {
  const skill = { ...base, source_type, source_ref: "/source" };
  assert.equal(matchesSkillLibraryFilter(skill, "updateable", "active"), true);
  assert.equal(matchesSkillLibraryFilter(skill, "updateable", "other"), true);
  assert.equal(matchesSkillLibraryFilter(skill, "updateable", null), true);
}
for (const update_status of ["up_to_date", "unknown", "checking", "updating", "error", "source_missing", "local_only"]) {
  assert.equal(matchesSkillLibraryFilter({ ...base, update_status }, "updateable", null), false);
}
for (const source_type of ["local", "import", "unsupported"]) {
  const skill = { ...base, source_type };
  assert.equal(canRefresh(skill), false);
  assert.equal(matchesSkillLibraryFilter(skill, "updateable", "active"), false);
}
assert.equal(matchesSkillLibraryFilter(base, "all", "active"), true);
assert.equal(matchesSkillLibraryFilter(base, "enabled", "active"), true);
assert.equal(matchesSkillLibraryFilter(base, "enabled", "other"), false);
assert.equal(matchesSkillLibraryFilter(base, "available", "active"), false);
assert.equal(matchesSkillLibraryFilter(base, "available", "other"), true);
assert.equal(matchesSkillLibraryFilter(base, "enabled", null), true);
assert.equal(matchesSkillLibraryFilter(base, "available", null), true);

for (const locale of ["en", "es", "zh", "zh-TW"]) {
  const translations = JSON.parse(readFileSync(new URL(`../src/i18n/${locale}.json`, import.meta.url), "utf8"));
  assert.ok(translations.mySkills.filters.updateable, `${locale}: missing filter label`);
}
console.log("Library filter tests passed (sources, statuses, presets, translations).");
