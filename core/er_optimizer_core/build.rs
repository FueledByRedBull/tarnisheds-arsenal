// Reads data/profile-facts.json, the one place that names the shipped profiles and the snapshot
// contract (versions, runtime tables, class rules). The Python tools read the same file.
//
// Embeds the runtime CSVs brotli-compressed: as text they were ~5 MB of a ~17 MB desktop
// executable. Each CSV must match its manifest size and SHA-256 before it is embedded; the
// loader still decompresses and checks the exact original bytes at runtime.
use sha2::{Digest, Sha256};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const FACTS_PATH: &str = "../../data/profile-facts.json";

fn main() {
    let out_dir = env::var("OUT_DIR").expect("OUT_DIR");
    let crate_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let facts = read_json(Path::new(FACTS_PATH));
    println!("cargo:rerun-if-changed={FACTS_PATH}");

    let text = |key: &str| {
        facts[key]
            .as_str()
            .unwrap_or_else(|| panic!("{FACTS_PATH}: {key} must be a string"))
    };
    let number = |key: &str| {
        facts[key]
            .as_u64()
            .unwrap_or_else(|| panic!("{FACTS_PATH}: {key} must be an unsigned integer"))
    };
    let runtime_files = facts["runtimeFiles"]
        .as_array()
        .unwrap_or_else(|| panic!("{FACTS_PATH}: runtimeFiles must be an array"))
        .iter()
        .map(|name| {
            name.as_str()
                .unwrap_or_else(|| panic!("{FACTS_PATH}: runtimeFiles entries must be strings"))
        })
        .collect::<Vec<_>>();
    let class_catalog_file = text("classCatalogFile");
    let built_in_class_budget_profile = text("builtInClassBudgetProfile");
    let profiles = facts["profiles"]
        .as_array()
        .unwrap_or_else(|| panic!("{FACTS_PATH}: profiles must be an array"));

    let mut generated = String::new();
    writeln!(
        generated,
        "pub const SNAPSHOT_SCHEMA_VERSION: u32 = {};",
        number("schemaVersion")
    )
    .expect("format facts");
    for (name, key) in [
        ("SNAPSHOT_MODEL_VERSION", "modelVersion"),
        ("SNAPSHOT_EXTRACTOR_VERSION", "extractorVersion"),
        ("CLASS_CATALOG_FILE", "classCatalogFile"),
        (
            "BUILT_IN_CLASS_BUDGET_PROFILE_ID",
            "builtInClassBudgetProfile",
        ),
    ] {
        writeln!(generated, "pub const {name}: &str = {:?};", text(key)).expect("format facts");
    }
    writeln!(
        generated,
        "/// Rune Level is the eight-attribute total minus this offset for every class.\n\
         pub const CLASS_LEVEL_OFFSET: u16 = {};",
        number("classLevelOffset")
    )
    .expect("format facts");
    writeln!(
        generated,
        "pub const RUNTIME_DATA_FILES: [&str; {}] = {runtime_files:?};",
        runtime_files.len()
    )
    .expect("format facts");

    let params = brotli::enc::BrotliEncoderParams {
        quality: 9,
        lgwin: 22,
        ..Default::default()
    };
    let mut profile_ids = Vec::new();
    let mut manifests = String::new();
    let mut tables = String::new();
    for profile in profiles {
        let profile_id = profile["id"]
            .as_str()
            .unwrap_or_else(|| panic!("{FACTS_PATH}: profile id must be a string"));
        let directory = profile["directory"]
            .as_str()
            .unwrap_or_else(|| panic!("{FACTS_PATH}: profile directory must be a string"));
        profile_ids.push((profile_id, directory));
        let directory_path = Path::new("../..").join(directory);
        let manifest_path = directory_path.join("manifest.json");
        println!("cargo:rerun-if-changed={}", manifest_path.display());
        let manifest = read_json(&manifest_path);
        // The class catalog rule the loader applies to a manifest (snapshot::requires_class_catalog).
        let ships_class_catalog = manifest["capabilities"]["classBudget"] == true
            && profile_id != built_in_class_budget_profile;
        writeln!(
            manifests,
            "    ({profile_id:?}, include_bytes!({:?}).as_slice()),",
            Path::new(&crate_dir)
                .join(&manifest_path)
                .display()
                .to_string()
        )
        .expect("format manifests");
        let extra_tables = ships_class_catalog.then_some(class_catalog_file);
        for &table in runtime_files.iter().chain(&extra_tables) {
            let source = directory_path.join(table);
            println!("cargo:rerun-if-changed={}", source.display());
            let bytes = fs::read(&source)
                .unwrap_or_else(|error| panic!("read {}: {error}", source.display()));
            verify_against_manifest(&manifest, table, &bytes, &source);
            let mut compressed = Vec::new();
            brotli::BrotliCompress(&mut bytes.as_slice(), &mut compressed, &params)
                .unwrap_or_else(|error| panic!("compress {}: {error}", source.display()));
            let target = Path::new(&out_dir).join(format!("{profile_id}-{table}.br"));
            fs::write(&target, compressed).expect("write compressed table");
            writeln!(
                tables,
                "    ({profile_id:?}, {table:?}, include_bytes!({:?}).as_slice()),",
                target.display().to_string()
            )
            .expect("format tables");
        }
    }
    writeln!(
        generated,
        "/// (profile, directory relative to the repository root) for every shipped profile.\n\
         #[cfg(test)]\n\
         pub const PROFILE_DIRECTORIES: [(&str, &str); {}] = {profile_ids:?};",
        profile_ids.len()
    )
    .expect("format facts");
    writeln!(
        generated,
        "/// (profile, manifest.json bytes) for every shipped profile.\n\
         pub const EMBEDDED_MANIFESTS: &[(&str, &[u8])] = &[\n{manifests}];"
    )
    .expect("format facts");
    writeln!(
        generated,
        "/// (profile, table, brotli-compressed CSV) for every embedded runtime table.\n\
         pub const EMBEDDED_TABLES: &[(&str, &str, &[u8])] = &[\n{tables}];"
    )
    .expect("format facts");
    fs::write(Path::new(&out_dir).join("profile_facts.rs"), generated)
        .expect("write profile facts");
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_slice(
        &fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn verify_against_manifest(manifest: &serde_json::Value, table: &str, bytes: &[u8], source: &Path) {
    let record = manifest["runtimeFiles"]
        .as_array()
        .and_then(|files| files.iter().find(|file| file["path"] == table))
        .unwrap_or_else(|| panic!("{} is not listed in its manifest", source.display()));
    let size = bytes.len() as u64;
    if record["size"].as_u64() != Some(size) {
        panic!(
            "{} is {size} bytes but its manifest says {}; regenerate the snapshot",
            source.display(),
            record["size"]
        );
    }
    let digest = lower_hex(&Sha256::digest(bytes));
    if record["sha256"].as_str() != Some(digest.as_str()) {
        panic!(
            "{} has SHA-256 {digest} but its manifest says {}; regenerate the snapshot",
            source.display(),
            record["sha256"]
        );
    }
}

/// Lowercase hexadecimal, as the manifests record SHA-256 digests.
fn lower_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut hex, byte| {
            write!(hex, "{byte:02x}").expect("writing to a String cannot fail");
            hex
        })
}
