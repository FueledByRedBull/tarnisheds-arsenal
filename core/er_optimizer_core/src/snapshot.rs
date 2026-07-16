use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SNAPSHOT_SCHEMA_VERSION: u32 = 3;
pub const RUNTIME_DATA_FILES: [&str; 13] = [
    "aow.csv",
    "aow_attack_data.csv",
    "aow_effect_data.csv",
    "aow_route_assignments.csv",
    "aow_weapon_compat.csv",
    "attack_element_correct.csv",
    "attack_element_correct_ext.csv",
    "calc_correct.csv",
    "native_skill_attack_data.csv",
    "reinforce.csv",
    "weapon_passive_overlays.csv",
    "weapon_passives.csv",
    "weapons.csv",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotSource {
    pub kind: String,
    pub bundled: bool,
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotProfile {
    pub id: String,
    pub display_name: String,
    pub game_version: String,
    pub mod_version: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotCapabilities {
    pub weapon_ar: bool,
    pub status_buildup: bool,
    pub weapon_passives: bool,
    pub aow_compatibility: bool,
    pub aow_damage: bool,
    pub aow_routes: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotRules {
    pub standard_max_upgrade: u8,
    pub somber_max_upgrade: u8,
    pub separate_upgrade_caps: bool,
    pub scadutree_scaling: bool,
    pub zero_attack_element_uses_weapon_scaling: bool,
    pub extended_scaling_grades: bool,
    pub status_buildup_scales: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotManifest {
    pub schema_version: u32,
    pub dataset_version: String,
    pub model_version: String,
    pub id: String,
    pub label: String,
    pub app_version: String,
    pub source: String,
    pub profile: SnapshotProfile,
    pub capabilities: SnapshotCapabilities,
    pub rules: SnapshotRules,
    pub generated_at: String,
    pub extractor_version: String,
    pub provenance: String,
    pub runtime_files: Vec<SnapshotFile>,
    pub diagnostic_files: Vec<SnapshotFile>,
    pub sources: Vec<SnapshotSource>,
}

pub(crate) fn validate_external_snapshot(data_dir: &Path) -> Result<SnapshotManifest, String> {
    let manifest_path = data_dir.join("manifest.json");
    let manifest_bytes = fs::read(&manifest_path).map_err(|err| {
        format!(
            "invalid runtime data snapshot: failed reading {}: {err}; restore the complete data folder",
            manifest_path.display()
        )
    })?;
    let manifest = parse_and_validate_manifest(&manifest_bytes)?;
    let listed_csvs = manifest
        .runtime_files
        .iter()
        .chain(&manifest.diagnostic_files)
        .map(|record| record.path.as_str())
        .collect::<HashSet<_>>();

    for record in manifest
        .runtime_files
        .iter()
        .chain(&manifest.diagnostic_files)
    {
        validate_external_file(data_dir, record)?;
    }
    let actual_csvs = fs::read_dir(data_dir)
        .map_err(|err| {
            format!(
                "invalid runtime data snapshot: failed listing {}: {err}",
                data_dir.display()
            )
        })?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            (path.is_file() && path.extension().and_then(|ext| ext.to_str()) == Some("csv"))
                .then(|| entry.file_name().to_string_lossy().into_owned())
        })
        .collect::<HashSet<_>>();
    let unlisted = actual_csvs
        .iter()
        .filter(|name| !listed_csvs.contains(name.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !unlisted.is_empty() {
        return Err(format!(
            "invalid runtime data snapshot: unlisted CSV files: {}; regenerate the complete snapshot",
            unlisted.join(", ")
        ));
    }

    for source in manifest.sources.iter().filter(|source| source.bundled) {
        validate_external_source(data_dir, source)?;
    }
    Ok(manifest)
}

pub(crate) fn validate_embedded_snapshot(
    manifest_bytes: &[u8],
    content_for_file: impl Fn(&str) -> Option<&'static [u8]>,
) -> Result<SnapshotManifest, String> {
    let manifest = parse_and_validate_manifest(manifest_bytes)?;
    for record in &manifest.runtime_files {
        let content = content_for_file(&record.path).ok_or_else(|| {
            format!(
                "invalid embedded data snapshot: manifest lists unavailable runtime file {}",
                record.path
            )
        })?;
        validate_bytes("embedded runtime file", record, content)?;
    }
    Ok(manifest)
}

fn parse_and_validate_manifest(content: &[u8]) -> Result<SnapshotManifest, String> {
    let manifest: SnapshotManifest = serde_json::from_slice(content)
        .map_err(|err| format!("invalid runtime data manifest JSON: {err}"))?;
    if manifest.schema_version != SNAPSHOT_SCHEMA_VERSION {
        return Err(format!(
            "unsupported data snapshot schema {}; expected {}",
            manifest.schema_version, SNAPSHOT_SCHEMA_VERSION
        ));
    }
    if manifest.rules.standard_max_upgrade > 25 || manifest.rules.somber_max_upgrade > 25 {
        return Err("invalid runtime data manifest: upgrade rules exceed +25".to_string());
    }
    if !manifest.rules.separate_upgrade_caps
        && manifest.rules.standard_max_upgrade != manifest.rules.somber_max_upgrade
    {
        return Err(
            "invalid runtime data manifest: single-path upgrade caps must match".to_string(),
        );
    }
    for (field, value) in [
        ("datasetVersion", manifest.dataset_version.as_str()),
        ("modelVersion", manifest.model_version.as_str()),
        ("id", manifest.id.as_str()),
        ("extractorVersion", manifest.extractor_version.as_str()),
        ("profile.id", manifest.profile.id.as_str()),
        (
            "profile.displayName",
            manifest.profile.display_name.as_str(),
        ),
        (
            "profile.gameVersion",
            manifest.profile.game_version.as_str(),
        ),
    ] {
        if value.trim().is_empty() {
            return Err(format!("invalid runtime data manifest: {field} is empty"));
        }
    }

    let expected = RUNTIME_DATA_FILES.into_iter().collect::<HashSet<_>>();
    let actual = manifest
        .runtime_files
        .iter()
        .map(|record| record.path.as_str())
        .collect::<HashSet<_>>();
    if actual != expected || actual.len() != manifest.runtime_files.len() {
        return Err(format!(
            "invalid runtime data manifest: runtime file set mismatch; expected {} exact files",
            expected.len()
        ));
    }

    let mut all_paths = HashSet::new();
    for record in manifest
        .runtime_files
        .iter()
        .chain(&manifest.diagnostic_files)
    {
        validate_record(record)?;
        if !all_paths.insert(record.path.as_str()) {
            return Err(format!(
                "invalid runtime data manifest: duplicate file entry {}",
                record.path
            ));
        }
    }
    let mut source_kinds = HashSet::new();
    for source in &manifest.sources {
        validate_safe_file_name(&source.path)?;
        validate_sha256(&source.sha256, &source.path)?;
        if source.kind.trim().is_empty() || !source_kinds.insert(source.kind.as_str()) {
            return Err(format!(
                "invalid runtime data manifest: duplicate or empty source kind {}",
                source.kind
            ));
        }
    }
    if !source_kinds.contains("regulation") {
        return Err("invalid runtime data manifest: missing regulation source hash".to_string());
    }
    if (manifest.capabilities.aow_damage || manifest.capabilities.aow_routes)
        && !source_kinds.contains("workbook")
    {
        return Err(
            "invalid runtime data manifest: AoW-capable profile is missing workbook source hash"
                .to_string(),
        );
    }
    Ok(manifest)
}

fn validate_record(record: &SnapshotFile) -> Result<(), String> {
    validate_safe_file_name(&record.path)?;
    validate_sha256(&record.sha256, &record.path)
}

fn validate_safe_file_name(value: &str) -> Result<(), String> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err(format!(
            "invalid runtime data manifest path {value:?}; only plain file names are allowed"
        ));
    }
    Ok(())
}

fn validate_sha256(value: &str, path: &str) -> Result<(), String> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "invalid runtime data manifest SHA-256 for {path}: {value:?}"
        ));
    }
    Ok(())
}

fn validate_external_file(data_dir: &Path, record: &SnapshotFile) -> Result<(), String> {
    let path = data_dir.join(&record.path);
    let content = fs::read(&path).map_err(|err| {
        format!(
            "invalid runtime data snapshot: failed reading {}: {err}",
            path.display()
        )
    })?;
    validate_bytes("runtime file", record, &content)
}

fn validate_external_source(data_dir: &Path, source: &SnapshotSource) -> Result<(), String> {
    let path = data_dir.join(&source.path);
    let content = fs::read(&path).map_err(|err| {
        format!(
            "invalid runtime data snapshot: failed reading workbook source {}: {err}",
            path.display()
        )
    })?;
    let record = SnapshotFile {
        path: source.path.clone(),
        size: source.size,
        sha256: source.sha256.clone(),
    };
    validate_bytes("workbook source", &record, &content)
}

fn validate_bytes(kind: &str, record: &SnapshotFile, content: &[u8]) -> Result<(), String> {
    if content.len() as u64 != record.size {
        return Err(format!(
            "invalid data snapshot {kind} {}: size is {}, manifest requires {}",
            record.path,
            content.len(),
            record.size
        ));
    }
    let actual = format!("{:x}", Sha256::digest(content));
    if actual != record.sha256.to_ascii_lowercase() {
        return Err(format!(
            "invalid data snapshot {kind} {}: SHA-256 mismatch",
            record.path
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use sha2::{Digest, Sha256};

    use super::{
        RUNTIME_DATA_FILES, SNAPSHOT_SCHEMA_VERSION, SnapshotCapabilities, SnapshotFile,
        SnapshotManifest, SnapshotProfile, SnapshotRules, SnapshotSource,
        parse_and_validate_manifest, validate_external_snapshot,
    };

    struct TestSnapshot {
        path: PathBuf,
    }

    impl TestSnapshot {
        fn create() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock after epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "er-optimizer-snapshot-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create test snapshot");

            let runtime_files = RUNTIME_DATA_FILES
                .iter()
                .map(|name| write_record(&path, name, format!("{name}\n").as_bytes()))
                .collect();
            let diagnostic_files = vec![write_record(&path, "coverage.csv", b"status\nresolved\n")];
            let workbook = write_record(&path, "workbook.xlsx", b"workbook-source");
            let manifest = SnapshotManifest {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                dataset_version: "test-dataset".to_string(),
                model_version: "test-model".to_string(),
                id: "test-dataset".to_string(),
                label: "Test dataset".to_string(),
                app_version: "test".to_string(),
                source: workbook.path.clone(),
                profile: SnapshotProfile {
                    id: "test".to_string(),
                    display_name: "Test".to_string(),
                    game_version: "test".to_string(),
                    mod_version: None,
                },
                capabilities: SnapshotCapabilities {
                    weapon_ar: true,
                    status_buildup: true,
                    weapon_passives: true,
                    aow_compatibility: true,
                    aow_damage: true,
                    aow_routes: true,
                },
                rules: SnapshotRules {
                    standard_max_upgrade: 25,
                    somber_max_upgrade: 10,
                    separate_upgrade_caps: true,
                    scadutree_scaling: true,
                    zero_attack_element_uses_weapon_scaling: false,
                    extended_scaling_grades: false,
                    status_buildup_scales: true,
                },
                generated_at: "2026-07-15".to_string(),
                extractor_version: "test-extractor".to_string(),
                provenance: "unit test".to_string(),
                runtime_files,
                diagnostic_files,
                sources: vec![
                    SnapshotSource {
                        kind: "regulation".to_string(),
                        bundled: false,
                        path: "regulation.bin".to_string(),
                        size: 10,
                        sha256: sha256(b"regulation"),
                    },
                    SnapshotSource {
                        kind: "workbook".to_string(),
                        bundled: true,
                        path: workbook.path,
                        size: workbook.size,
                        sha256: workbook.sha256,
                    },
                ],
            };
            fs::write(
                path.join("manifest.json"),
                serde_json::to_vec_pretty(&manifest).expect("serialize manifest"),
            )
            .expect("write manifest");
            Self { path }
        }
    }

    impl Drop for TestSnapshot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn write_record(directory: &Path, name: &str, content: &[u8]) -> SnapshotFile {
        fs::write(directory.join(name), content).expect("write snapshot file");
        SnapshotFile {
            path: name.to_string(),
            size: content.len() as u64,
            sha256: sha256(content),
        }
    }

    fn sha256(content: &[u8]) -> String {
        format!("{:x}", Sha256::digest(content))
    }

    #[test]
    fn external_snapshot_rejects_corruption_missing_and_unlisted_files() {
        let snapshot = TestSnapshot::create();
        let manifest = validate_external_snapshot(&snapshot.path).expect("valid snapshot");
        assert_eq!(manifest.dataset_version, "test-dataset");

        fs::write(snapshot.path.join("aow.csv"), b"mixed-version-data")
            .expect("corrupt runtime file");
        let corruption = validate_external_snapshot(&snapshot.path).unwrap_err();
        assert!(corruption.contains("aow.csv"));
        assert!(corruption.contains("size is") || corruption.contains("SHA-256 mismatch"));
        fs::write(snapshot.path.join("aow.csv"), b"aow.csv\n").expect("restore runtime file");

        fs::write(snapshot.path.join("unlisted.csv"), b"unexpected\n")
            .expect("write unlisted file");
        let unlisted = validate_external_snapshot(&snapshot.path).unwrap_err();
        assert!(unlisted.contains("unlisted CSV files"));
        fs::remove_file(snapshot.path.join("unlisted.csv")).expect("remove unlisted file");

        fs::remove_file(snapshot.path.join("weapons.csv")).expect("remove runtime file");
        let missing = validate_external_snapshot(&snapshot.path).unwrap_err();
        assert!(missing.contains("weapons.csv"));
        assert!(missing.contains("failed reading"));
    }

    #[test]
    fn manifest_parser_rejects_structured_mutation_corpus_without_panicking() {
        type ManifestMutation = (&'static str, Box<dyn Fn(&mut serde_json::Value)>);

        let snapshot = TestSnapshot::create();
        let valid = fs::read(snapshot.path.join("manifest.json")).expect("read manifest");
        parse_and_validate_manifest(&valid).expect("seed manifest is valid");
        let seed: serde_json::Value = serde_json::from_slice(&valid).expect("parse seed JSON");
        let mutations: Vec<ManifestMutation> = vec![
            (
                "wrong schema",
                Box::new(|value| value["schemaVersion"] = 999.into()),
            ),
            ("empty id", Box::new(|value| value["id"] = "".into())),
            (
                "path traversal",
                Box::new(|value| value["runtimeFiles"][0]["path"] = "../escape.csv".into()),
            ),
            (
                "bad hash",
                Box::new(|value| value["runtimeFiles"][0]["sha256"] = "xyz".into()),
            ),
            (
                "duplicate runtime",
                Box::new(|value| {
                    let duplicate = value["runtimeFiles"][0].clone();
                    value["runtimeFiles"]
                        .as_array_mut()
                        .expect("runtime array")
                        .push(duplicate);
                }),
            ),
            (
                "missing sources",
                Box::new(|value| value["sources"] = serde_json::json!([])),
            ),
        ];
        for (label, mutate) in mutations {
            let mut candidate = seed.clone();
            mutate(&mut candidate);
            let bytes = serde_json::to_vec(&candidate).expect("serialize mutation");
            assert!(parse_and_validate_manifest(&bytes).is_err(), "{label}");
        }
        for bytes in [b"".as_slice(), b"null", b"[]", b"{", b"\xff\x00\x01"] {
            assert!(parse_and_validate_manifest(bytes).is_err());
        }
    }
}
