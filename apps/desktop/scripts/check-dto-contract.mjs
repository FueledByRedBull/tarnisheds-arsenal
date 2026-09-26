import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import ts from "typescript";

// A source-shape check, not a Rust parser or runtime validator. New serde features
// must be handled explicitly here; numeric ranges and String value validation
// remain the native validators' responsibility.
const tsName = name => name === "PathMode" ? "PathModeId" : name;
const camelCase = name => name.replace(/_([a-z])/g, (_, letter) => letter.toUpperCase());
const snakeCase = name => name.replace(/[A-Z]/g, (letter, index) => `${index ? "_" : ""}${letter.toLowerCase()}`);
const union = values => [...new Set(values.flatMap(value => value.split(" | ")))].sort().join(" | ");

function rustType(type) {
  type = type.replace(/\s/g, "");
  if (/^(?:[ui](?:8|16|32|64|128|size)|f(?:32|64))$/.test(type)) return "number";
  if (type === "String") return "string";
  if (type === "bool") return "boolean";
  const generic = /^(Option|Vec)<(.+)>$/.exec(type);
  if (generic) {
    const inner = rustType(generic[2]);
    return generic[1] === "Option" ? union([inner, "null"]) : `Array<${inner}>`;
  }
  assert.match(type, /^(?:\w+Dto|PathMode)$/, `Unsupported Rust type: ${type}`);
  return tsName(type);
}

function rustDeclarations(source) {
  // Comments may separate attributes. Preserve quoted/raw strings so their text
  // cannot be mistaken for a comment that hides the next declaration.
  source = source.replace(/r(#{0,})"[\s\S]*?"\1|"(?:\\[\s\S]|[^"\\])*"|'(?:\\.|[^'\\])'|\/\/[^\r\n]*|\/\*[\s\S]*?\*\//g, token => {
    if (!token.startsWith("//") && !token.startsWith("/*")) return token;
    assert.ok(!token.startsWith("/*") || !token.slice(2).includes("/*"), "Unsupported nested Rust block comment");
    return token.replace(/[^\r\n]/g, " ");
  });
  const declarations = new Map();
  const pattern = /((?:#\[[^\]]+\]\s*)+)pub (struct|enum) (\w+)\s*\{([^}]+)\}/g;
  for (const [, attributes, kind, name, body] of source.matchAll(pattern)) {
    const rename = kind === "struct" ? "camelCase" : "snake_case";
    assert.equal(attributes.replace(/#\[derive\([^\]]+\)\]\s*/g, "")
      .replace(`#[serde(rename_all = "${rename}")]`, "").trim(), "", `Unsupported attributes on ${name}`);
    assert.ok(attributes.includes(`#[serde(rename_all = "${rename}")]`), `Missing serde rename on ${name}`);
    if (kind === "enum") {
      const variants = body.replace(/#\[default\]/g, "").split(",").map(value => value.trim()).filter(Boolean);
      for (const variant of variants) assert.match(variant, /^\w+$/, `Unsupported enum variant: ${name}.${variant}`);
      declarations.set(tsName(name), { variants: variants.map(snakeCase).sort() });
      continue;
    }
    const fields = new Map();
    const remainder = body.replace(/((?:#\[[^\]]+\]\s*)*)pub (\w+):\s*([^,]+),/g,
      (_, fieldAttributes, field, type) => {
        assert.equal(fieldAttributes.replace(/#\[serde\(default(?: = "\w+")?\)\]\s*/g, "").trim(), "",
          `Unsupported serde field attributes: ${name}.${field}`);
        assert.ok(!fields.has(camelCase(field)), `Duplicate field: ${name}.${field}`);
        fields.set(camelCase(field), rustType(type));
        return "";
      });
    assert.equal(remainder.trim(), "", `Unsupported Rust declaration: ${name}`);
    declarations.set(name, { fields });
  }
  assert.equal(declarations.size, [...source.matchAll(/\bpub (?:struct|enum)\s/g)].length,
    "Unrecognized public Rust declaration (including unsupported attributes or generics)");
  return declarations;
}

function typescriptDeclarations(source) {
  const file = ts.createSourceFile("types.ts", source, ts.ScriptTarget.Latest, true);
  assert.equal(file.parseDiagnostics.length, 0, "TypeScript source must parse");
  const declarations = new Map();
  for (const node of file.statements) {
    if (!ts.isInterfaceDeclaration(node) && !ts.isTypeAliasDeclaration(node)) continue;
    assert.ok(!declarations.has(node.name.text), `Duplicate TypeScript declaration: ${node.name.text}`);
    declarations.set(node.name.text, node);
  }
  function typeShape(node, seen = new Set()) {
    if (node.kind === ts.SyntaxKind.StringKeyword) return "string";
    if (node.kind === ts.SyntaxKind.NumberKeyword) return "number";
    if (node.kind === ts.SyntaxKind.BooleanKeyword) return "boolean";
    if (ts.isLiteralTypeNode(node)) {
      if (ts.isStringLiteral(node.literal)) return "string";
      if (ts.isNumericLiteral(node.literal)) return "number";
      if (node.literal.kind === ts.SyntaxKind.NullKeyword) return "null";
    }
    if (ts.isUnionTypeNode(node)) return union(node.types.map(type => typeShape(type, seen)));
    if (ts.isArrayTypeNode(node)) return `Array<${typeShape(node.elementType, seen)}>`;
    if (ts.isTypeReferenceNode(node) && ts.isIdentifier(node.typeName) && !node.typeArguments) {
      const name = node.typeName.text;
      const declaration = declarations.get(name);
      assert.ok(declaration && !seen.has(name), `Unknown or recursive TypeScript alias: ${name}`);
      if (ts.isInterfaceDeclaration(declaration) || name === "PathModeId" || name === "AnalysisJobKindDto") return name;
      return typeShape(declaration.type, new Set([...seen, name]));
    }
    assert.fail(`Unsupported TypeScript type: ${node.getText(file)}`);
  }
  return { declarations, typeShape };
}

// These are deliberate frontend restrictions, not a second copy of the schema.
// Exact before/after shapes ensure an exception cannot conceal later drift.
const differences = {
  "OptimizeRequestDto.maxUpgrade": ["null | number", undefined],
  "OptimizeRequestDto.fixedUpgrade": ["null | number", undefined],
  "OptimizeRequestDto.standardMaxUpgrade": ["null | number", "number"],
  "OptimizeRequestDto.somberMaxUpgrade": ["null | number", "number"],
  "OptimizeRequestDto.exactUpgrade": ["boolean | null", "boolean"],
  // Older saved builds can lack these display details; native responses include them.
  "SolvedBuildDto.weaponTypeName": ["string", "optional string"],
  "SolvedBuildDto.requirements": ["CombatStateDto", "optional CombatStateDto"],
  "SolvedBuildDto.effectiveScaling": ["ScalingDto", "optional ScalingDto"],
};

export function checkDtoContract(rustSource, tsSource) {
  const rust = rustDeclarations(rustSource);
  const { declarations, typeShape } = typescriptDeclarations(tsSource);
  const usedDifferences = new Set();
  let fieldCount = 0;
  for (const [name, contract] of rust) {
    const declaration = declarations.get(name);
    assert.ok(declaration, `Missing TypeScript contract: ${name}`);
    if (contract.variants) {
      assert.ok(ts.isTypeAliasDeclaration(declaration) && ts.isUnionTypeNode(declaration.type), `Expected enum union: ${name}`);
      const values = declaration.type.types.map(node => {
        assert.ok(ts.isLiteralTypeNode(node) && ts.isStringLiteral(node.literal), `Expected string enum: ${name}`);
        return node.literal.text;
      });
      assert.deepEqual(values.sort(), contract.variants, `Enum drift: ${name}`);
      continue;
    }
    assert.ok(ts.isInterfaceDeclaration(declaration) && !declaration.heritageClauses && !declaration.typeParameters,
      `Expected plain TypeScript interface: ${name}`);
    const fields = new Map(declaration.members.map(member => {
      assert.ok(ts.isPropertySignature(member) && ts.isIdentifier(member.name) && member.type && !member.modifiers,
        `Unsupported TypeScript field: ${name}`);
      return [member.name.text, `${member.questionToken ? "optional " : ""}${typeShape(member.type)}`];
    }));
    assert.equal(fields.size, declaration.members.length, `Duplicate TypeScript field: ${name}`);
    for (const field of new Set([...contract.fields.keys(), ...fields.keys()])) {
      const key = `${name}.${field}`;
      const shapes = [contract.fields.get(field), fields.get(field)];
      if (key in differences) {
        assert.deepEqual(shapes, differences[key], `Intentional contract restriction changed: ${key}`);
        usedDifferences.add(key);
      } else {
        assert.equal(shapes[1], shapes[0], `Contract drift: ${key}`);
      }
      fieldCount++;
    }
  }
  for (const name of declarations.keys()) {
    if (name.endsWith("Dto")) assert.ok(rust.has(name), `TypeScript DTO has no Rust declaration: ${name}`);
  }
  assert.deepEqual([...usedDifferences].sort(), Object.keys(differences).sort(), "Stale contract restrictions");
  return { declarations: rust.size, fields: fieldCount };
}

export function checkRepositoryContract() {
  return checkDtoContract(
    readFileSync(new URL("../src-tauri/src/dto.rs", import.meta.url), "utf8"),
    readFileSync(new URL("../src/lib/types.ts", import.meta.url), "utf8"),
  );
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const checked = checkRepositoryContract();
  console.log(`DTO contract verified: ${checked.declarations} declarations, ${checked.fields} fields.`);
}
