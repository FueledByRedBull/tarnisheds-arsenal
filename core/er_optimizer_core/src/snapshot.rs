use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde::Deserialize;
use sha2::{Digest, Sha256};

pub const SNAPSHOT_SCHEMA_VERSION: u32 = 6;
pub const RUNTIME_DATA_FILES: [&str; 12] = [
    "aow.csv",
    "aow_attack_data.csv",
    "aow_effect_data.csv",
    "aow_route_assignments.csv",
    "attack_element_correct.csv",
    "attack_element_correct_ext.csv",
    "calc_correct.csv",
    "native_skill_attack_data.csv",
    "reinforce.csv",
    "weapon_passive_overlays.csv",
    "weapon_passives.csv",
    "weapons.csv",
];

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotSource {
    pub kind: String,
    pub bundled: bool,
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotProfile {
    pub id: String,
    pub display_name: String,
    pub game_version: String,
    pub mod_version: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotCapabilities {
    pub weapon_ar: bool,
    pub weapon_ar_for_ammunition: bool,
    pub class_budget: bool,
    pub status_buildup: bool,
    pub weapon_passives: bool,
    pub aow_compatibility: bool,
    pub aow_damage: bool,
    pub aow_routes: bool,
}

#[derive(Clone, Debug, Deserialize)]
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

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotManifest {
    pub schema_version: u32,
    pub dataset_version: String,
    pub model_version: String,
    pub id: String,
    pub label: String,
    /// Game version the snapshot was extracted from (equals `profile.gameVersion`), not the
    /// application version.
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

#[derive(Debug)]
pub(crate) struct ValidatedExternalSnapshot {
    pub manifest: SnapshotManifest,
    pub runtime_files: HashMap<String, Vec<u8>>,
}

pub(crate) fn validate_external_snapshot(
    data_dir: &Path,
) -> Result<ValidatedExternalSnapshot, String> {
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
        .map(|record| record.path.to_ascii_lowercase())
        .collect::<HashSet<_>>();

    let mut runtime_files = HashMap::with_capacity(manifest.runtime_files.len());
    for record in &manifest.runtime_files {
        runtime_files.insert(
            record.path.clone(),
            read_validated_external_file(data_dir, record)?,
        );
    }
    for record in &manifest.diagnostic_files {
        read_validated_external_file(data_dir, record)?;
    }
    let entries = fs::read_dir(data_dir).map_err(|err| {
        format!(
            "invalid runtime data snapshot: failed listing {}: {err}",
            data_dir.display()
        )
    })?;
    let actual_csvs = snapshot_csv_names(entries).map_err(|error| {
        format!(
            "invalid runtime data snapshot: failed listing {}: {error}",
            data_dir.display()
        )
    })?;
    let unlisted = actual_csvs
        .iter()
        .filter(|name| !listed_csvs.contains(&name.to_ascii_lowercase()))
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
    Ok(ValidatedExternalSnapshot {
        manifest,
        runtime_files,
    })
}

fn snapshot_csv_names(
    entries: impl IntoIterator<Item = std::io::Result<fs::DirEntry>>,
) -> Result<HashSet<String>, String> {
    let mut names = HashSet::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("failed reading directory entry: {error}"))?;
        let path = entry.path();
        let metadata = fs::metadata(&path)
            .map_err(|error| format!("failed reading metadata for {}: {error}", path.display()))?;
        if metadata.is_file()
            && path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
        {
            names.insert(entry.file_name().to_string_lossy().into_owned());
        }
    }
    Ok(names)
}

pub(crate) fn validate_embedded_snapshot<'a>(
    manifest_bytes: &[u8],
    content_for_file: impl Fn(&str) -> Option<&'a [u8]>,
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

    if manifest.id != manifest.dataset_version
        || manifest.app_version != manifest.profile.game_version
    {
        return Err(
            "invalid runtime data manifest: id, datasetVersion, appVersion and profile.gameVersion disagree"
                .to_string(),
        );
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
        if !all_paths.insert(record.path.to_ascii_lowercase()) {
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
    // Same rule on every OS: no separators, drive or stream colons, Windows-trimmed
    // trailing dots/spaces, dot names or reserved device names.
    let stem = value
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit());
    if value.is_empty()
        || value.contains(['/', '\\', ':'])
        || value.ends_with(['.', ' '])
        || value.starts_with('.')
        || value.chars().any(char::is_control)
        || reserved
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

fn read_validated_external_file(data_dir: &Path, record: &SnapshotFile) -> Result<Vec<u8>, String> {
    let content = read_sized(data_dir, "runtime file", record)?;
    validate_bytes("runtime file", record, &content)?;
    Ok(content)
}

fn validate_external_source(data_dir: &Path, source: &SnapshotSource) -> Result<(), String> {
    let record = SnapshotFile {
        path: source.path.clone(),
        size: source.size,
        sha256: source.sha256.clone(),
    };
    let content = read_sized(data_dir, "workbook source", &record)?;
    validate_bytes("workbook source", &record, &content)
}

/// Checks the on-disk length before reading so an oversized file is never loaded.
fn read_sized(data_dir: &Path, kind: &str, record: &SnapshotFile) -> Result<Vec<u8>, String> {
    let path = data_dir.join(&record.path);
    let failed = |err: std::io::Error| {
        format!(
            "invalid runtime data snapshot: failed reading {kind} {}: {err}",
            path.display()
        )
    };
    let size = fs::metadata(&path).map_err(failed)?.len();
    if size != record.size {
        return Err(format!(
            "invalid data snapshot {kind} {}: size is {size}, manifest requires {}",
            record.path, record.size
        ));
    }
    fs::read(&path).map_err(failed)
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

    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};

    use super::{
        RUNTIME_DATA_FILES, SNAPSHOT_SCHEMA_VERSION, parse_and_validate_manifest,
        validate_embedded_snapshot, validate_external_snapshot,
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
                .collect::<Vec<_>>();
            let coverage = write_record(&path, "coverage.csv", b"status\nresolved\n");
            let mut workbook = write_record(&path, "workbook.xlsx", b"workbook-source");
            workbook["kind"] = "workbook".into();
            workbook["bundled"] = true.into();
            let manifest = json!({
                "schemaVersion": SNAPSHOT_SCHEMA_VERSION,
                "datasetVersion": "test-dataset",
                "modelVersion": "test-model",
                "id": "test-dataset",
                "label": "Test dataset",
                "appVersion": "test",
                "source": "workbook.xlsx",
                "profile": {
                    "id": "test",
                    "displayName": "Test",
                    "gameVersion": "test",
                    "modVersion": null,
                },
                "capabilities": {
                    "weaponAr": true,
                    "weaponArForAmmunition": true,
                    "classBudget": true,
                    "statusBuildup": true,
                    "weaponPassives": true,
                    "aowCompatibility": true,
                    "aowDamage": true,
                    "aowRoutes": true,
                },
                "rules": {
                    "standardMaxUpgrade": 25,
                    "somberMaxUpgrade": 10,
                    "separateUpgradeCaps": true,
                    "scadutreeScaling": true,
                    "zeroAttackElementUsesWeaponScaling": false,
                    "extendedScalingGrades": false,
                    "statusBuildupScales": true,
                },
                "generatedAt": "2026-07-15",
                "extractorVersion": "test-extractor",
                "provenance": "unit test",
                "runtimeFiles": runtime_files,
                "diagnosticFiles": [coverage],
                "sources": [
                    {
                        "kind": "regulation",
                        "bundled": false,
                        "path": "regulation.bin",
                        "size": 10,
                        "sha256": sha256(b"regulation"),
                    },
                    workbook,
                ],
            });
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

    fn write_record(directory: &Path, name: &str, content: &[u8]) -> Value {
        fs::write(directory.join(name), content).expect("write snapshot file");
        json!({"path": name, "size": content.len(), "sha256": sha256(content)})
    }

    fn sha256(content: &[u8]) -> String {
        format!("{:x}", Sha256::digest(content))
    }

    #[test]
    fn snapshot_directory_enumeration_errors_fail_closed() {
        let entries = [Err(std::io::Error::other("injected directory entry error"))];
        let error =
            super::snapshot_csv_names(entries).expect_err("entry errors must not be dropped");
        assert!(error.contains("injected directory entry error"));
    }

    #[test]
    fn snapshot_directory_metadata_errors_fail_closed() {
        let snapshot = TestSnapshot::create();
        let entry = fs::read_dir(&snapshot.path)
            .unwrap()
            .map(Result::unwrap)
            .find(|entry| entry.file_name() == "weapons.csv")
            .unwrap();
        fs::remove_file(entry.path()).unwrap();
        let error = super::snapshot_csv_names([Ok(entry)])
            .expect_err("metadata errors must not be dropped");
        assert!(error.contains("weapons.csv"));
    }

    #[test]
    fn external_snapshot_rejects_corruption_missing_and_unlisted_files() {
        let snapshot = TestSnapshot::create();
        let manifest = validate_external_snapshot(&snapshot.path).expect("valid snapshot");
        assert_eq!(manifest.manifest.dataset_version, "test-dataset");

        fs::write(snapshot.path.join("aow.csv"), b"mixed-version-data")
            .expect("corrupt runtime file");
        let corruption = validate_external_snapshot(&snapshot.path).unwrap_err();
        assert!(corruption.contains("aow.csv"));
        assert!(corruption.contains("size is") || corruption.contains("SHA-256 mismatch"));
        fs::write(snapshot.path.join("aow.csv"), b"aow.csv\n").expect("restore runtime file");

        for name in ["unlisted.csv", "UNLISTED.CSV"] {
            fs::write(snapshot.path.join(name), b"unexpected\n").expect("write unlisted file");
            let unlisted = validate_external_snapshot(&snapshot.path).unwrap_err();
            assert!(unlisted.contains("unlisted CSV files"), "{name}");
            fs::remove_file(snapshot.path.join(name)).expect("remove unlisted file");
        }

        fs::remove_file(snapshot.path.join("weapons.csv")).expect("remove runtime file");
        let missing = validate_external_snapshot(&snapshot.path).unwrap_err();
        assert!(missing.contains("weapons.csv"));
        assert!(missing.contains("failed reading"));
    }

    #[test]
    fn old_csv_schema_is_rejected_at_manifest_boundary() {
        let snapshot = TestSnapshot::create();
        let mut manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(snapshot.path.join("manifest.json")).unwrap())
                .unwrap();
        manifest["schemaVersion"] = 5.into();
        let error =
            parse_and_validate_manifest(&serde_json::to_vec(&manifest).unwrap()).unwrap_err();
        assert_eq!(error, "unsupported data snapshot schema 5; expected 6");
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
                Box::new(|value| value["schemaVersion"] = 3.into()),
            ),
            (
                "unknown future schema",
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
            (
                "upgrade cap above +25",
                Box::new(|value| value["rules"]["standardMaxUpgrade"] = 26.into()),
            ),
            (
                "single-path caps differ",
                Box::new(|value| value["rules"]["separateUpgradeCaps"] = false.into()),
            ),
            (
                "duplicate source kind",
                Box::new(|value| {
                    let duplicate = value["sources"][0].clone();
                    value["sources"].as_array_mut().unwrap().push(duplicate);
                }),
            ),
            (
                "AoW-capable without workbook",
                Box::new(|value| value["sources"].as_array_mut().unwrap().truncate(1)),
            ),
            (
                "id differs from datasetVersion",
                Box::new(|value| value["id"] = "other-dataset".into()),
            ),
            (
                "appVersion differs from gameVersion",
                Box::new(|value| value["appVersion"] = "other".into()),
            ),
            (
                "diagnostic duplicates a runtime file by case",
                Box::new(|value| {
                    let mut duplicate = value["runtimeFiles"][0].clone();
                    duplicate["path"] = "AOW.CSV".into();
                    value["diagnosticFiles"]
                        .as_array_mut()
                        .unwrap()
                        .push(duplicate);
                }),
            ),
        ];
        let unsafe_names = [
            "/abs.csv",
            "C:\\abs.csv",
            "C:abs.csv",
            "dir\\coverage.csv",
            "coverage.csv/",
            "coverage.csv/.",
            "coverage.csv:stream",
            "coverage.csv.",
            "coverage.csv ",
            ".",
            "..",
            "NUL.csv",
            "com1.csv",
        ];
        for name in unsafe_names {
            let mut candidate = seed.clone();
            candidate["diagnosticFiles"][0]["path"] = name.into();
            let bytes = serde_json::to_vec(&candidate).expect("serialize mutation");
            assert!(parse_and_validate_manifest(&bytes).is_err(), "{name:?}");
        }
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

    #[test]
    fn embedded_snapshot_rejects_size_and_hash_mismatches() {
        let snapshot = TestSnapshot::create();
        let manifest = fs::read(snapshot.path.join("manifest.json")).expect("read manifest");
        let files = RUNTIME_DATA_FILES.map(|name| (name, format!("{name}\n")));
        let content = |replacement: &'static [u8]| {
            let files = &files;
            move |name: &str| {
                if name == "aow.csv" {
                    Some(replacement)
                } else {
                    files
                        .iter()
                        .find(|(file, _)| *file == name)
                        .map(|(_, bytes)| bytes.as_bytes())
                }
            }
        };
        validate_embedded_snapshot(&manifest, content(b"aow.csv\n")).expect("valid embedded");
        let size = validate_embedded_snapshot(&manifest, content(b"aow.csv\n\n")).unwrap_err();
        assert!(
            size.contains("aow.csv") && size.contains("size is"),
            "{size}"
        );
        let hash = validate_embedded_snapshot(&manifest, content(b"AOW.CSV\n")).unwrap_err();
        assert!(
            hash.contains("aow.csv") && hash.contains("SHA-256"),
            "{hash}"
        );
        let missing = validate_embedded_snapshot(&manifest, |_| None).unwrap_err();
        assert!(missing.contains("unavailable runtime file"), "{missing}");
    }
}
