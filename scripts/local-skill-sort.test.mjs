import test from "node:test";
import assert from "node:assert/strict";
import { sortLocalSkills, sortLocalLocations } from "../src/lib/localSkillSort.ts";

const location = (tool, found_path) => ({ id: found_path, tool, found_path });
const group = (name, locations, found_at = 0, imported = false) => ({
  name, fingerprint: name, locations, found_at, imported,
});
const multi = group("competitor-news-monitor", [
  location("hermes_profiles:money-admin-assistant", "/Users/carlo/.hermes/profiles/money-admin-assistant/skills/research/competitor-news-monitor"),
  location("hermes", "/Users/carlo/.hermes/skills/.archive/competitor-news-monitor"),
], 100, true);
const single = group("audiocraft", [location("cursor", "/Users/carlo/.cursor/skills/audiocraft")], 200);

test("name and date support both directions, including imported skills", () => {
  assert.deepEqual(sortLocalSkills([multi, single], "name", "asc"), [single, multi]);
  assert.deepEqual(sortLocalSkills([multi, single], "name", "desc"), [multi, single]);
  assert.deepEqual(sortLocalSkills([multi, single], "date", "asc"), [multi, single]);
  assert.deepEqual(sortLocalSkills([multi, single], "date", "desc"), [single, multi]);
});

test("multi-location skills remain single groups and location input order has no effect", () => {
  const reversed = { ...multi, locations: [...multi.locations].reverse() };
  for (const key of ["agent", "location"]) {
    assert.deepEqual(sortLocalSkills([multi, single], key, "asc"), [single, multi]);
    assert.deepEqual(sortLocalSkills([reversed, single], key, "asc"), [single, reversed]);
    assert.deepEqual(sortLocalSkills([multi, single], key, "desc"), [multi, single]);
  }
});

test("full agent list breaks ties, with deterministic name/date ties", () => {
  const a = group("z", [location("hermes", "/z"), location("cursor", "/a")]);
  const b = group("a", [location("github_copilot", "/b"), location("cursor", "/c")]);
  assert.deepEqual(sortLocalSkills([a, b], "agent", "asc"), [b, a]);
  assert.deepEqual(sortLocalSkills([a, b], "date", "desc"), [b, a]);
});

test("display sorting preserves agent/path pairs without changing the import source", () => {
  const original = structuredClone(multi);
  assert.equal(sortLocalLocations(multi, "agent")[0].tool, "hermes");
  assert.equal(sortLocalLocations(multi, "location")[0].tool, "hermes_profiles:money-admin-assistant");
  sortLocalSkills([multi, single], "agent", "asc");
  assert.deepEqual(multi, original);
});

test("empty lists and missing locations are supported", () => {
  assert.deepEqual(sortLocalSkills([], "name", "asc"), []);
  const empty = group("empty", []);
  assert.deepEqual(sortLocalSkills([multi, empty], "agent", "asc"), [empty, multi]);
});
