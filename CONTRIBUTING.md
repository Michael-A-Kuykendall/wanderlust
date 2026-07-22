# Contributing to Wanderlust

## Open Source, Not Open Contribution

Wanderlust is **open source** but **not open contribution**.

- The code is freely available under the MIT license
- You can fork, modify, use, and learn from it without restriction
- **Pull requests are not accepted by default**
- All architectural, roadmap, and merge decisions are made by the project maintainer

This model keeps the project coherent, maintains clear ownership, and ensures consistent quality. It's the same approach used by SQLite and many infrastructure projects.

## How to Contribute

If you believe you can contribute meaningfully to Wanderlust:

1. **Email the maintainer first**: [michaelallenkuykendall@gmail.com](mailto:michaelallenkuykendall@gmail.com)
2. Describe your background and proposed contribution
3. If there is alignment, a scoped collaboration may be discussed privately
4. Only after discussion will PRs be considered

**Unsolicited PRs will be closed without merge.** This isn't personal, it's how this project operates.

## What We Welcome (via email first)

- Bug reports with detailed reproduction steps (Issues are fine)
- Security vulnerability reports (please email directly)
- Documentation improvements (discuss first)
- Platform-specific fixes (discuss first)

## What We Handle Internally

- New features and architectural changes
- API design decisions
- Dependency updates
- Performance optimizations
- Cross-platform compatibility work

## Bug Reports

Bug reports via GitHub Issues are welcome! Please include:
- Windows version and edition
- Rust version and wanderlust version
- Minimal reproduction case
- Expected vs actual behavior
- Output of `wanderlust doctor`

## Code Style (for reference)

If a contribution is discussed and approved:
- Rust 2021 edition with `cargo fmt` and `cargo clippy`
- Comprehensive error handling using `Result<T, Error>`
- All public APIs must have documentation with examples
- Safety-critical code with verification and rollback support

## Wanderlust Philosophy

Any accepted work must align with:
- **Safety First**: All changes are backed up and probed before finalizing
- **Zero-Config**: Install and forget — no configuration files
- **Windows Native**: Deep Windows integration via Registry API and Task Scheduler
- **Free Forever**: No features that could lead to paid tiers

## Why This Model?

Building reliable Windows environment management requires tight architectural control. This ensures:
- Consistent safety guarantees across all operations
- No ownership disputes or governance overhead
- Quality control without committee delays
- Clear direction for the project's future

The code is open. The governance is centralized. This is intentional.

## Recognition

Helpful bug reports and community feedback are acknowledged in release notes.
If email collaboration leads to merged work, attribution will be given appropriately.

---

**Maintainer**: Michael A. Kuykendall
