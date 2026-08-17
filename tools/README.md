# Tools

Tool directories follow the project phase names used during development.

| Path | Purpose |
| --- | --- |
| `tools/phase1` | Extraction and data refresh tooling for the committed runtime CSV snapshot. |
| `tools/phase4` | Validation, benchmarking, release metadata checks, and release packaging. |

There are no `tools/phase2` or `tools/phase3` directories because those phases
map to the Rust core and Tauri frontend source trees instead of standalone
Python tooling.

## Retained diagnostics

```powershell
python tools/phase4/benchmark_optimizer_phases.py --warmups 1 --repeats 5
python tools/phase4/validate_external_calculator.py
```

The benchmark is advisory; the external-calculator check compares supported
Vanilla cases with the public T. Clark calculator.
