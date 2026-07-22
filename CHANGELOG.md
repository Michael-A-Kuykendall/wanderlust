# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.5] - 2026-07-22

This release hardens Wanderlust with nine new capability modules (each with unit
tests) and aligns the open-source documentation with the `shimmy`/`crabcamera`
project standard.

### Added
- **`yank_guard`** — Grace-period removal of dead PATH entries. A temporarily
  disconnected drive/share is marked "suspicious" and only removed after it has
  been missing for `N` consecutive heal cycles, preventing false-positive
  deletions.
- **`backup_lock`** — Cooperative file mutex
  (`%LOCALAPPDATA%\wanderlust\backup.lock`) that serializes backup/registry
  writes across concurrent heal cycles, with stale-lock takeover so a crashed
  process cannot block future runs.
- **`subsystem`** — Preserves PATH entries owned by / shared with WSL, Cygwin,
  and MSYS2, and parses a live WSL `$PATH` so healing respects POSIX-subsystem
  dependencies.
- **`logging`** — Structured JSON-lines logging to
  `%LOCALAPPDATA%\wanderlust\logs\wanderlust.log`, with rotation that keeps the
  most recent copies.
- **`backup`** — SHA-256 backup integrity checks, backup rotation (keep-N), and
  partial-restore support.
- **`store`** — Cross-session heal history (JSONL) enabling failure escalation,
  baselining, and drift detection across runs.
- **`snapshot`** — Known-good PATH snapshots with drift detection, used for
  verification and rollback.
- **`baseline`** — AUTO-mode baselining (conservative / aggressive sensitivity)
  that learns the "normal" shape of a machine's PATH.
- **`uninstall`** — Detects orphaned PATH entries left behind after a program
  is uninstalled.
- Integration: `run_healing` now records a heal-history entry and a structured
  log line after each successful heal; `apply_path` acquires the backup lock
  around its writes. (Both are gated behind `cfg(not(test))` so unit tests stay
  hermetic.)
- Open-source documentation aligned with the `shimmy`/`crabcamera` standard:
  `CONTRIBUTING.md`, `SPONSORS.md`, `.github/FUNDING.yml`, `AGENTS.md`,
  `.gitattributes`, and `bd` issue tracking.

### Changed
- Added dependencies: `serde`, `serde_json`, `sha2`.
- Sponsor section standardized to the `$5 / $25 / $100 / $500` tier model
  (removed the earlier joke amounts).
- README terminology corrected: "daemon" → "scheduled task".

## [0.2.4] - 2026-02-01

### Added
- Python installation discovery in `scan_common_locations`: detects user
  installs under `%LOCALAPPDATA%\Programs\Python\Python3XX` and system-wide
  Python installs (including `Scripts` subdirectories), plus old-style
  `C:\Python3XX` and `Program Files\Python3XX`.

## [0.2.3] - 2026-02-01

A major healing-engine rewrite focused on safety, correctness, and
observability.

### Added
- System (HKLM) PATH scanning and deduplication via `clean_system_path`.
- Fail-closed transaction model in `apply_path`: refuses empty discovery plans,
  verifies environment health after writing, and rolls back on failure.
- Invariant / property-based testing (`invariant_ppt`) with contract tests.
- Human-readable output for `doctor` and dry-run heal.
- CLI subcommands `doctor`, `install`, `uninstall`, and `-v`/`--verbose`
  logging.

### Changed
- Substantial rewrite of `cleaner.rs` (healing engine) and `system.rs`
  (SystemOps boundary).
- Fixed an elevation / UAC bug in `elevation.rs`.

## [0.2.0] - 2026-01-31

### Added
- HKLM (machine-wide) registry scanning in discovery: `scan_registry_uninstall`
  now inspects both `HKCU` and `HKLM` `Software\Microsoft\Windows\CurrentVersion\Uninstall`.

### Changed
- Dependency cleanup (trimmed `Cargo.lock` and crate dependencies).

## [0.1.0] - 2026-01-31

### Added
- Initial release: a self-healing Windows PATH manager.
- Tool discovery via registry uninstall keys, common locations (`~/.cargo/bin`,
  `~/.local/bin`, Scoop shims), and the existing PATH.
- Deduplication, dead-link removal, and POSIX (Git Bash / MSYS2) cache
  generation.
- Registry backup + rollback and health probes (`cmd`, `powershell`, `whoami`).
- UAC / elevation handling and scheduled-task install/uninstall.
- Project documentation: README, CODE_OF_CONDUCT, DCO, SECURITY, CODEOWNERS,
  LICENSE.

[0.2.5]: https://github.com/Michael-A-Kuykendall/wanderlust/compare/e56a57a...407160e
[0.2.4]: https://github.com/Michael-A-Kuykendall/wanderlust/compare/921098f...e56a57a
[0.2.3]: https://github.com/Michael-A-Kuykendall/wanderlust/compare/baf1ce9...921098f
[0.2.0]: https://github.com/Michael-A-Kuykendall/wanderlust/compare/06d2922...baf1ce9
[0.1.0]: https://github.com/Michael-A-Kuykendall/wanderlust/commit/06d2922
