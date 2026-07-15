# Phase 1 Dump Tooling

This folder intentionally excludes bundled WitchyBND binaries to keep the repository lean and publishable.

`phase1_dump.py` is the supported complete-snapshot entry point. It extracts numeric
PARAM relationships, workbook attacks, legal AoW routes, effects, passives, and
coverage diagnostics into a sibling staging directory; validates hashes and file
sets; then promotes files with `manifest.json` last. A runtime loader can never
accept a partially promoted snapshot.

## Required Inputs

- Elden Ring `regulation.bin`
- Local WitchyBND executable path

## Example

```powershell
python tools/phase1/phase1_dump.py `
  --regulation data/raw/regulation.bin `
  --witchybnd C:\path\to\WitchyBND.exe `
  --output data/phase1
```

See `tools/phase1/PHASE1_CONVENTIONS.md` for normalization/indexing conventions.
