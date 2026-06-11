# Release v0.4.9

- Harden the desktop shell with a stricter Tauri content security policy and backend request limits for search, path, and affinity jobs.
- Share one Rust job registry across search, path previews, and affinity watch with clearer retry/cancel behavior.
- Canonicalize optimizer objectives, fix exact-upgrade request handling, and move Ash of War compatibility policy into the Rust core for Tauri and Python parity.
- Replace fragile Rust CSV loading with parser-backed validation and add data manifest/version display for the packaged runtime snapshot.
- Add saved build persistence, JSON import/export, and `ta-v1:` share text for build sharing.
- Add analysis caching, typed frontend job hooks, store slices, DTO contract checks, and Scadutree parity coverage.
- Improve desktop accessibility and dense-data behavior with a combobox-backed selector, ARIA grid/table semantics, selected-build summaries, Rankings scaling, and compare option filtering.
- Tighten the Rankings board column alignment so Affinity, AoW, upgrade, scaling, and stat columns stay centered and line up cleanly.
- Replace the single upgrade cap with separate Standard and Somber upgrade caps plus one shared Exact toggle across Rankings, Compare, Paths, and Affinity Watch.
- Refresh Windows e2e packaging so the release helper produces both `dist/TarnishedsArsenal_<version>` and `dist/TarnishedsArsenal_<version>.zip`.
