# Tools

Tool directories follow the project phase names used during development.

| Path | Purpose |
| --- | --- |
| `tools/phase1` | Extraction and data refresh tooling for the committed runtime CSV snapshot. |
| `tools/phase4` | Validation, benchmarking, release metadata checks, and release packaging. |

There are no `tools/phase2` or `tools/phase3` directories because those phases
map to the Rust core and Tauri frontend source trees instead of standalone
Python tooling.
