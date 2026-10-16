import assert from "node:assert/strict";
import { readdirSync, readFileSync } from "node:fs";
import test from "node:test";
import { checkCommandContract, checkDtoContract } from "./check-dto-contract.mjs";

const rust = readFileSync(new URL("../src-tauri/src/dto.rs", import.meta.url), "utf8");
const frontend = readFileSync(new URL("../src/lib/types.ts", import.meta.url), "utf8");

test("all native DTOs and enums agree with the frontend's wire shapes", () => {
  const checked = checkDtoContract(rust, frontend);
  assert.ok(checked.declarations > 50 && checked.fields > 250);
});

for (const [description, before, after, expected] of [
  ["renamed field", "pub job_id: String,", "pub job_key: String,", /Contract drift/],
  ["new field", "pub job_id: String,", "pub job_id: String, pub attempt: u8,", /Contract drift/],
  ["primitive change", "pub weapon_id: u32,", "pub weapon_id: String,", /Contract drift/],
  ["nullability change", "pub aow_id: Option<u16>,", "pub aow_id: u16,", /Contract drift/],
  ["nested type change", "pub stats: CombatStateDto,", "pub stats: ScalingDto,", /Contract drift/],
  ["array change", "pub rows: Vec<SolvedBuildDto>,", "pub rows: SolvedBuildDto,", /Contract drift/],
  ["enum value change", "    NoRespec,", "    Respec,", /Enum drift/],
  ["unsupported serde", "pub job_id: String,", '#[serde(skip_serializing)] pub job_id: String,', /Unsupported serde/],
  ["unsupported Rust type", "pub job_id: String,", "pub job_id: serde_json::Value,", /Unsupported Rust type/],
  ["exception type change", "pub standard_max_upgrade: Option<u8>,", "pub standard_max_upgrade: Option<String>,", /restriction changed/],
]) {
  test(`rejects Rust ${description}`, () => {
    assert.ok(rust.includes(before), "mutation must apply to the current source");
    assert.throws(() => checkDtoContract(rust.replace(before, after), frontend), expected);
  });
}

test("rejects missing frontend interfaces and unsupported TypeScript types", () => {
  assert.throws(() => checkDtoContract(rust, frontend.replace("interface CatalogDto", "interface RenamedCatalogDto")),
    /Unknown or recursive|Missing TypeScript/);
  assert.throws(() => checkDtoContract(rust, frontend.replace("schemaVersion: number;", "schemaVersion: any;")),
    /Unsupported TypeScript type/);
});

test("optional display-field restrictions do not allow unrelated optional fields", () => {
  assert.throws(() => checkDtoContract(rust, frontend.replace("weaponId: number;", "weaponId?: number;")), /Contract drift/);
  assert.throws(() => checkDtoContract(rust, frontend.replace("requirements?: CombatStateDto;", "requirements?: ScalingDto;")),
    /restriction changed/);
});

test("rejects merged TypeScript declarations instead of silently dropping earlier fields", () => {
  assert.throws(() => checkDtoContract(rust, `export interface CombatStateDto { extra: number; }\n${frontend}`),
    /Duplicate TypeScript declaration: CombatStateDto/);
});

test("comments cannot hide unsupported Rust container attributes", () => {
  for (const comment of ["// explanatory comment\n", "/* explanatory comment */\n"]) {
    const changed = rust.replace("#[derive(", `#[serde(deny_unknown_fields)]\n${comment}#[derive(`);
    assert.throws(() => checkDtoContract(changed, frontend), /Unsupported attributes on StableFilterEntryDto/);
  }
});

test("comment handling preserves normal and raw Rust string contents", () => {
  const examples = 'const QUOTED: &str = "/*";\nconst RAW: &str = r##"text \" /* still a string */ //"##;\n';
  assert.deepEqual(checkDtoContract(examples + rust, frontend), checkDtoContract(rust, frontend));
  assert.throws(() => checkDtoContract(`/* nested /* comment */ */\n${rust}`, frontend), /Unsupported nested Rust block comment/);
});

const source = path => readFileSync(new URL(path, import.meta.url), "utf8");
const commandDirectory = new URL("../src-tauri/src/commands/", import.meta.url);
const commandContract = () => ({
  commandSources: readdirSync(commandDirectory).map(name => readFileSync(new URL(name, commandDirectory), "utf8")),
  libSource: source("../src-tauri/src/lib.rs"),
  buildSource: source("../src-tauri/build.rs"),
  capability: JSON.parse(source("../src-tauri/capabilities/default.json")),
  apiSource: source("../src/lib/api.ts"),
});

test("every native command is registered, allowed and called with its argument names", () => {
  assert.ok(checkCommandContract(commandContract()).commands > 15);
});

for (const [description, mutate, expected] of [
  ["unregistered command", c => ({ ...c, libSource: c.libSource.replace("commands::optimize::cancel_search,", "") }), /generate_handler/],
  ["unlisted ACL command", c => ({ ...c, buildSource: c.buildSource.replace('"cancel_search",', "") }), /build.rs/],
  ["capability without the command", c => ({ ...c, capability: { permissions: c.capability.permissions.slice(1) } }), /capability/],
  ["renamed frontend argument", c => ({ ...c, apiSource: c.apiSource.replace('call<boolean>("cancel_search", { jobId })', 'call<boolean>("cancel_search", { id: jobId })') }), /argument drift: cancel_search/],
  ["misspelled frontend command", c => ({ ...c, apiSource: c.apiSource.replace('"cancel_search"', '"cancel_searches"') }), /unregistered command: cancel_searches/],
]) {
  test(`rejects ${description}`, () => {
    const contract = commandContract();
    const changed = mutate(contract);
    assert.notDeepEqual(changed, contract, "mutation must apply to the current source");
    assert.throws(() => checkCommandContract(changed), expected);
  });
}
