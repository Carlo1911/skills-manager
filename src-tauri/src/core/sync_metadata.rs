use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;
use walkdir::WalkDir;

use super::central_repo;
use super::repo_lock::RepoLock;
use super::skill_metadata;
use super::skill_store::{ScenarioRecord, SkillRecord, SkillStore};

const SCHEMA_VERSION: u32 = 2;
const APP_MIN_VERSION: &str = "2.0.0";

#[derive(Debug, Serialize, Deserialize)]
pub struct SchemaFile {
    pub schema_version: u32,
    pub app_min_version: String,
    pub created_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceMeta {
    #[serde(rename = "type")]
    pub source_type: String,
    #[serde(rename = "ref")]
    pub ref_: Option<String>,
    pub subpath: Option<String>,
    pub branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillMetaFile {
    pub schema_version: u32,
    pub skill_id: String,
    pub path: String,
    pub path_key: String,
    pub enabled: bool,
    #[serde(default)]
    pub devices: BTreeMap<String, bool>,
    pub tags: Vec<String>,
    pub source: SourceMeta,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScenarioMetaFile {
    pub schema_version: u32,
    pub scenario_id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub sort_order: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScenarioSkillMetaFile {
    pub schema_version: u32,
    pub scenario_id: String,
    pub skill_id: String,
    pub sort_order: i32,
    pub tools: BTreeMap<String, bool>,
}

/// Reindex refused: the metadata lists no skills while the central repo has
/// skill directories (applying it would empty the library). Typed so startup
/// can tell it apart from other failures.
#[derive(Debug)]
pub struct EmptySkillMetadata;

impl std::fmt::Display for EmptySkillMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "sync metadata contains no skills, but the central repository contains skill directories",
        )
    }
}

impl std::error::Error for EmptySkillMetadata {}

pub fn metadata_dir() -> PathBuf {
    central_repo::skills_dir().join(".skills-manager")
}

pub fn metadata_exists() -> bool {
    metadata_dir().join("schema.json").exists()
        || metadata_dir().join("skills").is_dir()
        || metadata_dir().join("scenarios").is_dir()
}

pub fn has_complete_skill_snapshot() -> bool {
    metadata_dir().join("schema.json").is_file() && metadata_dir().join("skills").is_dir()
}

#[allow(dead_code)]
pub fn write_all_from_db(store: &SkillStore) -> Result<()> {
    // Foreground wait: this runs at startup and from CLI preset/enable
    // commands, which must succeed (startup aborts on error), not skip.
    let _lock = RepoLock::acquire_foreground("write sync metadata")?;
    write_all_from_db_unlocked(store)
}

/// Runs `f` while holding the central-repo lock. Callers are user-initiated
/// operations (set tags, delete, preset edits, imports), so we wait out
/// transient contention with background work instead of failing fast.
pub(crate) fn with_repo_lock<T, F>(operation: &str, f: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    let _lock = RepoLock::acquire_foreground(operation)?;
    f()
}

pub(crate) fn write_all_from_db_unlocked(store: &SkillStore) -> Result<()> {
    ensure_metadata_dirs()?;
    write_schema()?;
    write_skill_records_from_db(store)?;
    write_scenario_records_from_db(store)?;
    remove_stale_metadata_files(store)?;
    Ok(())
}

/// Rewrite one preset member's file, which holds its per-agent toggles, for
/// callers that change only that member. `write_all_from_db_unlocked` fsyncs
/// every metadata file, too slow for commands that run once per (skill, agent)
/// pair. Without a complete scenario snapshot the reindex leaves memberships
/// to the database, so there is nothing to keep in step.
pub(crate) fn write_scenario_skill_unlocked(
    store: &SkillStore,
    scenario_id: &str,
    skill_id: &str,
) -> Result<()> {
    if !metadata_has_complete_scenario_snapshot() {
        return Ok(());
    }
    let skill_ids = store.get_skill_ids_for_scenario(scenario_id)?;
    let Some(index) = skill_ids.iter().position(|id| id == skill_id) else {
        return Ok(());
    };
    let tools = store
        .get_scenario_skill_tool_toggles(scenario_id, skill_id)?
        .into_iter()
        .map(|toggle| (toggle.tool, toggle.enabled))
        .collect::<BTreeMap<_, _>>();
    write_membership_file(&ScenarioSkillMetaFile {
        schema_version: SCHEMA_VERSION,
        scenario_id: scenario_id.to_string(),
        skill_id: skill_id.to_string(),
        sort_order: index as i32,
        tools,
    })
}

#[allow(dead_code)]
pub fn reindex_from_metadata(store: &SkillStore) -> Result<()> {
    // Foreground wait: runs at process startup (GUI and every CLI invocation),
    // which aborts on error — better to wait out transient contention than fail.
    let _lock = RepoLock::acquire_foreground("reindex sync metadata")?;
    reindex_from_metadata_unlocked(store)
}

/// Effective enabled flag for this device (opción C).
///
/// `devices[mi-slug]`, when present, is the answer. Otherwise the legacy
/// global `meta.enabled` freezes as the published default: pre-per-device
/// clients deactivated skills by writing `enabled=false`, so a skill with
/// no `devices` entry and `enabled=false` still reads as disabled the
/// first time — and the caller materializes it into `skill_device_state`.
/// New skills (no previous local row, no devices entry) start from the
/// published flag; other devices' entries never decide for us.
pub fn effective_enabled_for(
    meta: &SkillMetaFile,
    my_slug: &str,
    previous_enabled: Option<bool>,
) -> bool {
    if let Some(mine) = meta.devices.get(my_slug) {
        return *mine;
    }
    if meta.devices.is_empty() {
        // Legacy metadata (or an old client): the global flag is the
        // published state. Existing rows keep their local value unless the
        // published flag turned false; new skills take the published flag.
        return match previous_enabled {
            Some(prev) => prev && meta.enabled,
            None => meta.enabled,
        };
    }
    // Other devices have entries but we don't: fresh skill for us.
    false
}

pub(crate) fn reindex_from_metadata_unlocked(store: &SkillStore) -> Result<()> {
    if !metadata_exists() {
        return Ok(());
    }
    if !has_complete_skill_snapshot() {
        bail!("incomplete sync metadata snapshot: missing schema.json or skills directory");
    }

    let skills = read_skill_files()?;
    if skills.is_empty() && central_repo_has_valid_skill_dirs()? {
        return Err(EmptySkillMetadata.into());
    }
    ensure_unique_path_keys(&skills)?;

    let has_complete_scenario_snapshot = metadata_has_complete_scenario_snapshot();
    let scenarios = if has_complete_scenario_snapshot {
        read_scenario_files()?
    } else {
        Vec::new()
    };
    let memberships = if has_complete_scenario_snapshot {
        read_membership_files()?
    } else {
        Vec::new()
    };
    let now = chrono::Utc::now().timestamp_millis();
    let skills_root = central_repo::skills_dir();

    let metadata_ids: HashSet<String> = skills.iter().map(|m| m.skill_id.clone()).collect();
    for existing in store.get_all_skills()? {
        if !metadata_ids.contains(&existing.id) {
            store.delete_skill(&existing.id)?;
        }
    }

    let existing_by_id: HashMap<String, SkillRecord> = store
        .get_all_skills()?
        .into_iter()
        .map(|skill| (skill.id.clone(), skill))
        .collect();

    let my_slug = super::git_backup::device_slug(store);
    // Every surviving row gets its central_path rewritten below; park them
    // on placeholders first so path reassignments between skills (renames or
    // merge collision reshuffles) cannot trip the UNIQUE constraint mid-loop.
    store.park_central_paths_for_reindex()?;

    for meta in skills {
        let skill_dir = skills_root.join(&meta.path);
        if !skill_dir.is_dir() {
            store.delete_skill(&meta.skill_id)?;
            continue;
        }

        let parsed = skill_metadata::parse_skill_md(&skill_dir);
        let inferred_name = skill_dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown-skill".to_string());
        let name = parsed
            .name
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(inferred_name);
        let previous = existing_by_id.get(&meta.skill_id);
        let source_ref = if matches!(meta.source.source_type.as_str(), "import" | "local") {
            previous.and_then(|s| s.source_ref.clone())
        } else {
            meta.source.ref_.clone()
        };
        let central_path = skill_dir.to_string_lossy().to_string();

        let new_hash = super::content_hash::hash_directory(&skill_dir).ok();
        let updated_at = match previous {
            Some(prev) if prev.content_hash == new_hash => prev.updated_at,
            _ => now,
        };

        // Explicit local decision wins over the stored row: a startup repair
        // or an older client may have rewritten `skills.enabled`, but the
        // user's own toggle (explicit=1) is authoritative for this device.
        // The published map still wins when it carries our slug — it is the
        // same decision echoed back after a sync.
        let my_explicit = store
            .get_explicit_device_state(&meta.skill_id, &my_slug)
            .ok()
            .flatten();
        let effective = match (meta.devices.get(&my_slug).copied(), my_explicit) {
            (Some(published), _) => published,
            (None, Some(local)) => local,
            (None, None) => effective_enabled_for(&meta, &my_slug, previous.map(|s| s.enabled)),
        };
        let record = SkillRecord {
            id: meta.skill_id.clone(),
            name,
            description: parsed.description,
            source_type: meta.source.source_type.clone(),
            source_ref,
            source_ref_resolved: previous.and_then(|s| s.source_ref_resolved.clone()),
            source_subpath: meta.source.subpath.clone(),
            source_branch: meta.source.branch.clone(),
            source_revision: previous.and_then(|s| s.source_revision.clone()),
            remote_revision: previous.and_then(|s| s.remote_revision.clone()),
            central_path,
            content_hash: new_hash,
            enabled: effective,
            created_at: previous.map(|s| s.created_at).unwrap_or(now),
            updated_at,
            status: "ok".to_string(),
            update_status: previous
                .map(|s| s.update_status.clone())
                .unwrap_or_else(|| "unknown".to_string()),
            last_checked_at: previous.and_then(|s| s.last_checked_at),
            last_check_error: previous.and_then(|s| s.last_check_error.clone()),
        };
        store.upsert_skill(&record)?;
        store.set_tags_for_skill(&meta.skill_id, &meta.tags)?;
        // Materialize the effective flag so the next metadata write carries
        // it under our own slug. Guarded to our own missing entry only, and
        // only when the merged `devices{}` map AGREES with the computed
        // effective value for us — so a merge another device must still
        // interpret never rewrites our local opt-out, while a fresh legacy
        // row still gets its first explicit entry.
        let merged_mine = meta.devices.get(&my_slug).copied();
        let have_mine = store.get_device_state(&meta.skill_id, &my_slug)?;
        let should_materialize = match (have_mine, merged_mine) {
            (Some(_), _) => false,
            (None, Some(m)) => m == effective,
            (None, None) => true,
        };
        if should_materialize {
            if let Err(e) = store.set_device_state(&meta.skill_id, &my_slug, effective) {
                log::warn!("reindex: failed to materialize device state: {e:#}");
            }
        }
    }

    if has_complete_scenario_snapshot {
        store.replace_scenarios_from_metadata(&scenarios)?;
        store.replace_scenario_memberships_from_metadata(&memberships)?;
    }
    Ok(())
}

#[allow(dead_code)]
pub fn ensure_skill_metadata(store: &SkillStore, skill_id: &str) -> Result<()> {
    // Foreground wait: the CLI `tag` commands flush metadata here after a DB
    // write; they should ride out contention rather than fail fast.
    let _lock = RepoLock::acquire_foreground("write skill metadata")?;
    ensure_skill_metadata_unlocked(store, skill_id)
}

pub(crate) fn ensure_skill_metadata_unlocked(store: &SkillStore, skill_id: &str) -> Result<()> {
    write_schema()?;
    let skill = store
        .get_skill_by_id(skill_id)?
        .ok_or_else(|| anyhow!("skill not found: {skill_id}"))?;
    let tags = store.get_tags_map()?.remove(skill_id).unwrap_or_default();
    let devices = store
        .explicit_device_states_for_skill(skill_id)?
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    write_skill_file(&skill, &tags, &devices)
}

pub fn cleanup_temporary_files() -> Result<()> {
    let root = metadata_dir();
    if !root.exists() {
        return Ok(());
    }
    for entry in WalkDir::new(root).into_iter().flatten() {
        if entry.file_type().is_file() {
            let name = entry.file_name().to_string_lossy();
            if name.contains(".tmp.") {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    Ok(())
}

fn write_schema() -> Result<()> {
    let schema = SchemaFile {
        schema_version: SCHEMA_VERSION,
        app_min_version: APP_MIN_VERSION.to_string(),
        created_by: "skills-manager".to_string(),
    };
    atomic_write_json(&metadata_dir().join("schema.json"), &schema)
}

fn ensure_metadata_dirs() -> Result<()> {
    fs::create_dir_all(metadata_dir().join("skills"))?;
    fs::create_dir_all(metadata_dir().join("scenarios"))?;
    fs::create_dir_all(metadata_dir().join("scenario-skills"))?;
    Ok(())
}

// Scenario files are only ever written together with all of their
// memberships (write_scenario_records_from_db), so scenarios/ alone marks a
// complete snapshot. scenario-skills/ cannot be required: Git drops it once
// the last membership in the library is removed (#553).
fn metadata_has_complete_scenario_snapshot() -> bool {
    metadata_dir().join("schema.json").is_file() && metadata_dir().join("scenarios").is_dir()
}

fn write_skill_records_from_db(store: &SkillStore) -> Result<()> {
    let mut tags = store.get_tags_map()?;
    for skill in store.get_all_skills()? {
        // Only user-decided rows reach the shared blob; derived (reindex
        // materialized) rows stay DB-local so commits don't churn (§10).
        let devices = store
            .explicit_device_states_for_skill(&skill.id)?
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        write_skill_file(&skill, &tags.remove(&skill.id).unwrap_or_default(), &devices)?;
    }
    Ok(())
}

fn write_scenario_records_from_db(store: &SkillStore) -> Result<()> {
    for scenario in store.get_all_scenarios()? {
        write_scenario_file(&scenario)?;
        let skill_ids = store.get_skill_ids_for_scenario(&scenario.id)?;
        for (index, skill_id) in skill_ids.iter().enumerate() {
            let tools = store
                .get_scenario_skill_tool_toggles(&scenario.id, skill_id)?
                .into_iter()
                .map(|toggle| (toggle.tool, toggle.enabled))
                .collect::<BTreeMap<_, _>>();
            let member = ScenarioSkillMetaFile {
                schema_version: SCHEMA_VERSION,
                scenario_id: scenario.id.clone(),
                skill_id: skill_id.clone(),
                sort_order: index as i32,
                tools,
            };
            write_membership_file(&member)?;
        }
    }
    Ok(())
}

fn remove_stale_metadata_files(store: &SkillStore) -> Result<()> {
    let skill_ids: HashSet<String> = store
        .get_all_skills()?
        .into_iter()
        .map(|skill| skill.id)
        .collect();
    remove_stale_json_files(&metadata_dir().join("skills"), &skill_ids)?;

    let scenario_ids: HashSet<String> = store
        .get_all_scenarios()?
        .into_iter()
        .map(|scenario| scenario.id)
        .collect();
    remove_stale_json_files(&metadata_dir().join("scenarios"), &scenario_ids)?;

    let membership_root = metadata_dir().join("scenario-skills");
    if membership_root.exists() {
        let mut expected_memberships = HashSet::new();
        for scenario_id in &scenario_ids {
            for skill_id in store.get_skill_ids_for_scenario(scenario_id)? {
                expected_memberships.insert((scenario_id.clone(), skill_id));
            }
        }

        for entry in fs::read_dir(&membership_root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let scenario_id = entry.file_name().to_string_lossy().to_string();
            if !scenario_ids.contains(&scenario_id) {
                fs::remove_dir_all(entry.path())?;
                continue;
            }

            let mut has_files = false;
            for member in fs::read_dir(entry.path())? {
                let member = member?;
                if !member.file_type()?.is_file() {
                    continue;
                }
                let path = member.path();
                if path.extension().map(|ext| ext == "json").unwrap_or(false) {
                    let skill_id = path
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().to_string())
                        .unwrap_or_default();
                    if !expected_memberships.contains(&(scenario_id.clone(), skill_id)) {
                        fs::remove_file(&path)?;
                    } else {
                        has_files = true;
                    }
                }
            }
            if !has_files && fs::read_dir(entry.path())?.next().is_none() {
                fs::remove_dir(entry.path())?;
            }
        }
    }

    Ok(())
}

fn remove_stale_json_files(dir: &Path, expected_stems: &HashSet<String>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let path = entry.path();
        if path.extension().map(|ext| ext == "json").unwrap_or(false) {
            let stem = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
                .unwrap_or_default();
            if !expected_stems.contains(&stem) {
                fs::remove_file(&path)?;
            }
        }
    }
    Ok(())
}

fn write_skill_file(
    skill: &SkillRecord,
    tags: &[String],
    devices: &BTreeMap<String, bool>,
) -> Result<()> {
    let path = relative_skill_path(&skill.central_path)?;
    let tags = sorted_tags(tags);
    let source_ref = match skill.source_type.as_str() {
        "import" | "local" => None,
        _ => skill.source_ref.clone(),
    };
    let meta = SkillMetaFile {
        schema_version: SCHEMA_VERSION,
        skill_id: skill.id.clone(),
        path_key: path_key(&path),
        path,
        enabled: skill.enabled,
        devices: devices.clone(),
        tags,
        source: SourceMeta {
            source_type: skill.source_type.clone(),
            ref_: source_ref,
            subpath: skill.source_subpath.clone(),
            branch: skill.source_branch.clone(),
        },
    };
    let out_path = metadata_dir()
        .join("skills")
        .join(format!("{}.json", skill.id));
    // Convergence (§10): reindex materializes our own slug into the DB, but
    // the on-disk blob must not churn when nothing device-visible changed.
    // Otherwise every commit rewrites every skill JSON with our own
    // `devices{slug}` entry and no two devices ever produce the same bytes,
    // breaking fast-forwards and clean-deletion propagation.
    if let Ok(existing) = fs::read(&out_path) {
        if let Ok(old) = serde_json::from_slice::<SkillMetaFile>(&existing) {
            if old.path == meta.path
                && old.path_key == meta.path_key
                && old.tags == meta.tags
                && old.source == meta.source
                && old.devices == meta.devices
            {
                return Ok(());
            }
        }
    }
    atomic_write_json(&out_path, &meta)
}

fn write_scenario_file(scenario: &ScenarioRecord) -> Result<()> {
    let meta = ScenarioMetaFile {
        schema_version: SCHEMA_VERSION,
        scenario_id: scenario.id.clone(),
        name: scenario.name.clone(),
        description: scenario.description.clone(),
        icon: scenario.icon.clone(),
        sort_order: scenario.sort_order,
    };
    atomic_write_json(
        &metadata_dir()
            .join("scenarios")
            .join(format!("{}.json", scenario.id)),
        &meta,
    )
}

fn write_membership_file(member: &ScenarioSkillMetaFile) -> Result<()> {
    atomic_write_json(
        &metadata_dir()
            .join("scenario-skills")
            .join(&member.scenario_id)
            .join(format!("{}.json", member.skill_id)),
        member,
    )
}

fn read_skill_files() -> Result<Vec<SkillMetaFile>> {
    read_json_files(metadata_dir().join("skills"))
}

fn central_repo_has_valid_skill_dirs() -> Result<bool> {
    let skills_dir = central_repo::skills_dir();
    if !skills_dir.exists() {
        return Ok(false);
    }

    for entry in WalkDir::new(&skills_dir)
        .min_depth(1)
        .max_depth(6)
        .into_iter()
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            name != ".git" && name != ".skills-manager"
        })
    {
        let entry = entry?;
        if entry.file_type().is_dir() && skill_metadata::is_valid_skill_dir(entry.path()) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn read_scenario_files() -> Result<Vec<ScenarioMetaFile>> {
    read_json_files(metadata_dir().join("scenarios"))
}

fn read_membership_files() -> Result<Vec<ScenarioSkillMetaFile>> {
    let root = metadata_dir().join("scenario-skills");
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut records = Vec::new();
    for entry in WalkDir::new(root)
        .min_depth(2)
        .max_depth(2)
        .into_iter()
        .flatten()
    {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "json")
        {
            let raw = fs::read_to_string(entry.path())?;
            records.push(serde_json::from_str(&raw).with_context(|| {
                format!(
                    "invalid scenario membership metadata {}",
                    entry.path().display()
                )
            })?);
        }
    }
    Ok(records)
}

fn read_json_files<T: for<'de> Deserialize<'de>>(dir: PathBuf) -> Result<Vec<T>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let is_json = entry
            .path()
            .extension()
            .map(|ext| ext == "json")
            .unwrap_or(false);
        if !entry.file_type()?.is_file() || !is_json {
            continue;
        }
        let raw = fs::read_to_string(entry.path())?;
        records.push(
            serde_json::from_str(&raw)
                .with_context(|| format!("invalid metadata file {}", entry.path().display()))?,
        );
    }
    Ok(records)
}

fn ensure_unique_path_keys(skills: &[SkillMetaFile]) -> Result<()> {
    let mut seen = HashMap::new();
    for skill in skills {
        let computed = path_key(&skill.path);
        if computed != skill.path_key {
            bail!("path_key mismatch for skill {}", skill.skill_id);
        }
        if let Some(previous) = seen.insert(skill.path_key.clone(), skill.skill_id.clone()) {
            bail!(
                "case-insensitive path collision between {} and {}",
                previous,
                skill.skill_id
            );
        }
    }
    Ok(())
}

fn relative_skill_path(central_path: &str) -> Result<String> {
    let root = central_repo::skills_dir();
    let path = PathBuf::from(central_path);
    let relative = path
        .strip_prefix(&root)
        .with_context(|| format!("skill path is outside skills root: {}", path.display()))?;
    let value = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");
    if value.is_empty() || value.starts_with("../") {
        bail!("invalid relative skill path: {value}");
    }
    Ok(value)
}

/// Case/Unicode-folded key of a relative skill path. Shared with the merge
/// engine (§2.1): metadata rebuilt during a merge must use the exact same
/// folding as metadata written from the DB.
pub(crate) fn path_key(path: &str) -> String {
    path.split('/')
        .map(|part| caseless::default_case_fold_str(&part.nfc().collect::<String>()))
        .collect::<Vec<_>>()
        .join("/")
}

fn sorted_tags(tags: &[String]) -> Vec<String> {
    tags.iter()
        .map(|tag| tag.trim())
        .filter(|tag| !tag.is_empty())
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

/// Canonical on-disk JSON encoding for metadata files. The merge engine
/// reuses this to rebuild merged metadata blobs so that two devices merging
/// the same pair of commits produce byte-identical blobs → identical tree
/// OIDs (§2.1 / §10).
pub(crate) fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = canonical_json_bytes(value)?;
    let tmp = path.with_extension(format!("json.tmp.{}", uuid::Uuid::now_v7()));
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    sync_parent_dir(path)?;
    Ok(())
}

fn sync_parent_dir(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        // Windows requires FILE_FLAG_BACKUP_SEMANTICS to open a directory
        // handle. std::fs::File::open uses ordinary file semantics and fails
        // with Access is denied (os error 5) for directories.
        // FlushFileBuffers also requires a handle opened with write access.
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x02000000;

        let dir = fs::OpenOptions::new()
            .write(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(parent)?;
        dir.sync_all()?;
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let dir = fs::File::open(parent)?;
        dir.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{central_repo, skill_store::SkillStore};
    use std::sync::MutexGuard;
    use tempfile::{TempDir, tempdir};

    struct TestRepo {
        _lock: MutexGuard<'static, ()>,
        _tmp: TempDir,
        store: SkillStore,
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            central_repo::set_test_base_dir_override(None);
        }
    }

    fn test_repo() -> TestRepo {
        let lock = central_repo::test_base_dir_lock();
        let tmp = tempdir().unwrap();
        let base = tmp.path().join("repo");
        central_repo::set_test_base_dir_override(Some(base.clone()));
        fs::create_dir_all(central_repo::skills_dir()).unwrap();
        let store = SkillStore::new(&base.join("test.db")).unwrap();
        TestRepo {
            _lock: lock,
            _tmp: tmp,
            store,
        }
    }

    fn write_skill_dir(name: &str) -> PathBuf {
        let dir = central_repo::skills_dir().join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), format!("---\nname: {name}\n---\n")).unwrap();
        dir
    }

    fn sample_skill(id: &str, central_path: &Path) -> SkillRecord {
        SkillRecord {
            id: id.to_string(),
            name: id.to_string(),
            description: None,
            source_type: "import".to_string(),
            source_ref: Some(central_path.to_string_lossy().to_string()),
            source_ref_resolved: None,
            source_subpath: None,
            source_branch: None,
            source_revision: None,
            remote_revision: None,
            central_path: central_path.to_string_lossy().to_string(),
            content_hash: None,
            enabled: true,
            created_at: 1,
            updated_at: 1,
            status: "ok".to_string(),
            update_status: "local_only".to_string(),
            last_checked_at: None,
            last_check_error: None,
        }
    }

    /// The single-member write must produce the file a full write would.
    #[test]
    fn member_write_matches_full_write() {
        let repo = test_repo();
        for skill_id in ["skill-1", "skill-2"] {
            let dir = write_skill_dir(skill_id);
            repo.store
                .insert_skill(&sample_skill(skill_id, &dir))
                .unwrap();
        }
        insert_preset_with_member(&repo.store, "p1", "skill-1");
        repo.store.add_skill_to_scenario("p1", "skill-2").unwrap();
        write_all_from_db_unlocked(&repo.store).unwrap();
        // The member at index 1, so a wrong sort_order shows in the file.
        let skill_id = repo.store.get_skill_ids_for_scenario("p1").unwrap()[1].clone();
        repo.store
            .set_scenario_skill_tool_enabled("p1", &skill_id, "claude_code", false)
            .unwrap();
        let path = metadata_dir().join(format!("scenario-skills/p1/{skill_id}.json"));

        write_scenario_skill_unlocked(&repo.store, "p1", &skill_id).unwrap();
        let member_write = fs::read_to_string(&path).unwrap();
        write_all_from_db_unlocked(&repo.store).unwrap();

        assert!(member_write.contains("claude_code"));
        assert_eq!(member_write, fs::read_to_string(&path).unwrap());
    }

    /// Without scenarios/ the reindex leaves presets to the database, so a
    /// member write has nothing to keep in step and writes nothing.
    #[test]
    fn member_write_skips_a_snapshot_without_scenario_metadata() {
        let repo = test_repo();
        let skill_dir = write_skill_dir("skill-1");
        repo.store
            .insert_skill(&sample_skill("skill-1", &skill_dir))
            .unwrap();
        insert_preset_with_member(&repo.store, "p1", "skill-1");
        // Writes schema.json and skills/ only.
        ensure_skill_metadata_unlocked(&repo.store, "skill-1").unwrap();

        write_scenario_skill_unlocked(&repo.store, "p1", "skill-1").unwrap();

        assert!(!metadata_dir().join("scenario-skills").exists());
    }

    #[test]
    fn incomplete_metadata_snapshot_does_not_delete_existing_skills() {
        let repo = test_repo();
        let skill_dir = write_skill_dir("example-skill");
        repo.store
            .insert_skill(&sample_skill("skill-1", &skill_dir))
            .unwrap();

        fs::create_dir_all(metadata_dir()).unwrap();
        fs::write(metadata_dir().join("schema.json"), "{}").unwrap();

        let err = reindex_from_metadata_unlocked(&repo.store).unwrap_err();
        assert!(err.to_string().contains("incomplete sync metadata"));
        assert!(repo.store.get_skill_by_id("skill-1").unwrap().is_some());
    }

    #[test]
    fn empty_skill_metadata_does_not_delete_central_skills() {
        let repo = test_repo();
        let skill_dir = write_skill_dir("example-skill");
        repo.store
            .insert_skill(&sample_skill("skill-1", &skill_dir))
            .unwrap();

        fs::create_dir_all(metadata_dir().join("skills")).unwrap();
        fs::write(metadata_dir().join("schema.json"), "{}").unwrap();

        let err = reindex_from_metadata_unlocked(&repo.store).unwrap_err();
        assert!(err.to_string().contains("contains no skills"));
        assert!(repo.store.get_skill_by_id("skill-1").unwrap().is_some());
    }

    #[test]
    fn metadata_reindex_preserves_skill_id_and_tags() {
        let source = test_repo();
        let skill_dir = write_skill_dir("example-skill");
        source
            .store
            .insert_skill(&sample_skill("skill-1", &skill_dir))
            .unwrap();
        source
            .store
            .set_tags_for_skill("skill-1", &["tag-b".to_string(), "tag-a".to_string()])
            .unwrap();
        write_all_from_db_unlocked(&source.store).unwrap();

        let restored_store =
            SkillStore::new(&central_repo::base_dir().join("restored.db")).unwrap();
        reindex_from_metadata_unlocked(&restored_store).unwrap();

        let restored = restored_store
            .get_skill_by_id("skill-1")
            .unwrap()
            .expect("skill should be restored from metadata");
        assert_eq!(restored.central_path, skill_dir.to_string_lossy());
        assert_eq!(
            restored_store
                .get_tags_map()
                .unwrap()
                .remove("skill-1")
                .unwrap(),
            vec!["tag-a".to_string(), "tag-b".to_string()]
        );
    }

    #[test]
    fn reindex_preserves_updated_at_when_content_unchanged() {
        let repo = test_repo();
        let skill_dir = write_skill_dir("stable-skill");

        let mut record = sample_skill("skill-stable", &skill_dir);
        record.updated_at = 1_000_000;
        record.content_hash =
            Some(super::super::content_hash::hash_directory(&skill_dir).unwrap());
        repo.store.insert_skill(&record).unwrap();
        write_all_from_db_unlocked(&repo.store).unwrap();

        reindex_from_metadata_unlocked(&repo.store).unwrap();

        let after = repo
            .store
            .get_skill_by_id("skill-stable")
            .unwrap()
            .expect("skill must still exist");
        assert_eq!(
            after.updated_at, 1_000_000,
            "updated_at must not change when skill content is unchanged"
        );
    }

    fn insert_preset_with_member(store: &SkillStore, scenario_id: &str, skill_id: &str) {
        store
            .insert_scenario(&ScenarioRecord {
                id: scenario_id.to_string(),
                name: scenario_id.to_string(),
                description: None,
                icon: None,
                sort_order: 0,
                created_at: 1,
                updated_at: 1,
            })
            .unwrap();
        store.add_skill_to_scenario(scenario_id, skill_id).unwrap();
    }

    #[test]
    fn reindex_clears_memberships_when_last_membership_file_is_gone() {
        let repo = test_repo();
        let skill_dir = write_skill_dir("member-skill");
        repo.store
            .insert_skill(&sample_skill("skill-1", &skill_dir))
            .unwrap();
        insert_preset_with_member(&repo.store, "preset-1", "skill-1");
        write_all_from_db_unlocked(&repo.store).unwrap();

        // Another device removed the last membership in the library. Git
        // does not track empty directories, so the pulled tree has no
        // scenario-skills/ at all while scenarios/ is still there (#553).
        fs::remove_dir_all(metadata_dir().join("scenario-skills")).unwrap();

        reindex_from_metadata_unlocked(&repo.store).unwrap();

        let members = repo.store.get_skill_ids_for_scenario("preset-1").unwrap();
        assert!(members.is_empty(), "stale membership survived: {members:?}");
        assert_eq!(repo.store.get_all_scenarios().unwrap().len(), 1);
    }

    #[test]
    fn reindex_keeps_presets_when_snapshot_has_no_scenario_metadata() {
        let repo = test_repo();
        let skill_dir = write_skill_dir("member-skill");
        repo.store
            .insert_skill(&sample_skill("skill-1", &skill_dir))
            .unwrap();
        insert_preset_with_member(&repo.store, "preset-1", "skill-1");
        // Writes schema.json and skills/ only: the snapshot says nothing
        // about presets, so it must not wipe them.
        ensure_skill_metadata_unlocked(&repo.store, "skill-1").unwrap();
        assert!(!metadata_dir().join("scenarios").exists());

        reindex_from_metadata_unlocked(&repo.store).unwrap();

        assert_eq!(repo.store.get_all_scenarios().unwrap().len(), 1);
        assert_eq!(
            repo.store.get_skill_ids_for_scenario("preset-1").unwrap(),
            vec!["skill-1".to_string()]
        );
    }

    #[test]
    fn reindex_updates_updated_at_when_content_changes() {
        let repo = test_repo();
        let skill_dir = write_skill_dir("changing-skill");

        let mut record = sample_skill("skill-changing", &skill_dir);
        record.updated_at = 1_000_000;
        record.content_hash = Some("old-hash-before-change".to_string());
        repo.store.insert_skill(&record).unwrap();
        write_all_from_db_unlocked(&repo.store).unwrap();

        // Modify the skill content so the hash changes
        fs::write(skill_dir.join("extra.md"), "new content").unwrap();

        reindex_from_metadata_unlocked(&repo.store).unwrap();

        let after = repo
            .store
            .get_skill_by_id("skill-changing")
            .unwrap()
            .expect("skill must still exist");
        assert_ne!(
            after.updated_at, 1_000_000,
            "updated_at must be refreshed when skill content changes"
        );
    }

    #[test]
    fn devices_round_trip_through_metadata_and_reindex() {
        let repo = test_repo();
        repo.store
            .set_setting("backup_device_name", "My Laptop")
            .unwrap();
        let skill_dir = write_skill_dir("device-skill");
        repo.store
            .insert_skill(&sample_skill("skill-dev", &skill_dir))
            .unwrap();
        repo.store
            .set_device_state_explicit("skill-dev", "my-laptop", false, true)
            .unwrap();
        repo.store
            .set_device_state_explicit("skill-dev", "office-desktop", true, true)
            .unwrap();

        write_all_from_db_unlocked(&repo.store).unwrap();
        let raw = fs::read_to_string(metadata_dir().join("skills/skill-dev.json")).unwrap();
        let file: SkillMetaFile = serde_json::from_str(&raw).unwrap();
        assert_eq!(file.schema_version, 2);
        assert_eq!(file.devices.get("my-laptop"), Some(&false));
        assert_eq!(file.devices.get("office-desktop"), Some(&true));

        // Reindex on this device (my-laptop): effective=false materializes.
        let restored = SkillStore::new(&central_repo::base_dir().join("restored-dev.db")).unwrap();
        restored
            .set_setting("backup_device_name", "My Laptop")
            .unwrap();
        reindex_from_metadata_unlocked(&restored).unwrap();
        let row = restored.get_skill_by_id("skill-dev").unwrap().unwrap();
        assert!(!row.enabled);
        assert_eq!(
            restored.get_device_state("skill-dev", "my-laptop").unwrap(),
            Some(false)
        );
        // device_states_for_skill is NOT materialized wholesale — only our
        // own row is written by reindex.
        assert_eq!(
            restored.get_device_state("skill-dev", "office-desktop").unwrap(),
            None
        );
    }

    #[test]
    fn legacy_disabled_skill_materializes_on_first_reindex() {
        let repo = test_repo();
        repo.store
            .set_setting("backup_device_name", "My Laptop")
            .unwrap();
        let skill_dir = write_skill_dir("legacy-skill");
        let mut record = sample_skill("skill-legacy", &skill_dir);
        record.enabled = false;
        repo.store.insert_skill(&record).unwrap();
        write_all_from_db_unlocked(&repo.store).unwrap();

        // Simulate an old client: strip devices{} but keep enabled=false.
        let path = metadata_dir().join("skills/skill-legacy.json");
        let raw = fs::read_to_string(&path).unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        value.as_object_mut().unwrap().remove("devices");
        fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();

        let restored = SkillStore::new(&central_repo::base_dir().join("restored-legacy.db")).unwrap();
        restored
            .set_setting("backup_device_name", "My Laptop")
            .unwrap();
        reindex_from_metadata_unlocked(&restored).unwrap();
        let row = restored.get_skill_by_id("skill-legacy").unwrap().unwrap();
        assert!(!row.enabled, "legacy enabled=false must be respected");
        assert_eq!(
            restored.get_device_state("skill-legacy", "my-laptop").unwrap(),
            Some(false)
        );
    }

    #[test]
    fn effective_enabled_for_unit_cases() {
        let mut meta = SkillMetaFile {
            schema_version: 2,
            skill_id: "s".to_string(),
            path: "p".to_string(),
            path_key: "p".to_string(),
            enabled: true,
            devices: BTreeMap::new(),
            tags: vec![],
            source: SourceMeta {
                source_type: "import".to_string(),
                ref_: None,
                subpath: None,
                branch: None,
            },
        };
        // Own entry wins over everything.
        meta.devices.insert("mine".to_string(), false);
        assert!(!effective_enabled_for(&meta, "mine", Some(true)));
        // Empty devices + legacy flag: new skills take it verbatim.
        meta.devices.clear();
        assert!(effective_enabled_for(&meta, "mine", None));
        meta.enabled = false;
        assert!(!effective_enabled_for(&meta, "mine", None));
        // Empty devices + legacy false + existing enabled row: stays off.
        assert!(!effective_enabled_for(&meta, "mine", Some(true)));
        // Other devices' entries never decide for us.
        meta.enabled = true;
        meta.devices.insert("other".to_string(), true);
        assert!(!effective_enabled_for(&meta, "mine", Some(true)));
    }
}
