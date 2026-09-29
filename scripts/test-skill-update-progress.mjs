import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { transpileModule, ModuleKind, ScriptTarget } from "typescript";

// Exercise the pure UI state logic without a browser or a Tauri runtime.
const source = readFileSync(new URL("../src/lib/skillUpdateProgress.ts", import.meta.url), "utf8");
const { outputText } = transpileModule(source, {
  compilerOptions: { module: ModuleKind.ESNext, target: ScriptTarget.ES2022 },
});
const { completedSkillCount, receiveSkillUpdateProgress } = await import(
  `data:text/javascript;base64,${Buffer.from(outputText).toString("base64")}`
);
const item = (id, status) => ({ id, name: `Skill ${id}`, status, error: null });
const run = (run_id, items, finished = false) => ({ run_id, items, finished });

assert.equal(completedSkillCount(run("a", [
  item("1", "pending"), item("2", "checking"), item("3", "updating"),
  item("4", "updated"), item("5", "unchanged"), item("6", "held_back"), item("7", "failed"),
])), 4);

let runs = receiveSkillUpdateProgress([], run("a", [item("1", "pending")]));
runs = receiveSkillUpdateProgress(runs, run("a", [item("1", "updating")]));
assert.equal(runs.length, 1);
assert.equal(completedSkillCount(runs[0]), 0);
runs = receiveSkillUpdateProgress(runs, run("b", [item("2", "updating")]));
runs = receiveSkillUpdateProgress(runs, run("a", [item("1", "failed")], true));
assert.equal(runs.length, 2);
assert.equal(runs[1].run_id, "b");
assert.equal(runs[1].finished, false);
assert.equal(completedSkillCount(runs[0]), 1);

for (let i = 0; i < 5; i++) {
  runs = receiveSkillUpdateProgress(runs, run(`done-${i}`, [item(`${i}`, "updated")], true));
}
assert.equal(runs.filter((value) => value.finished).length, 3);
assert.ok(runs.some((value) => value.run_id === "b"));
assert.deepEqual(receiveSkillUpdateProgress(runs, run("empty", [], true)), runs);

for (const locale of ["en", "es", "zh", "zh-TW"]) {
  const translations = JSON.parse(readFileSync(new URL(`../src/i18n/${locale}.json`, import.meta.url), "utf8"));
  for (const status of ["pending", "checking", "updating", "updated", "unchanged", "held_back", "failed"]) {
    assert.ok(translations.skillUpdateProgress.status[status], `${locale}: missing ${status}`);
  }
}
console.log("Skill update UI state and translation tests passed.");
