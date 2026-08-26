# Wanderlust: The Self-Healing Windows PATH Manager 🧭✨

![Wanderlust Splash](https://raw.githubusercontent.com/Michael-A-Kuykendall/wanderlust/refs/heads/master/assets/wanderlust-splash.jpg)

[![Crates.io](https://img.shields.io/crates/v/wanderlust.svg)](https://crates.io/crates/wanderlust) [![Trans rights](https://pride-badges.pony.workers.dev/static/v1?label=trans%20rights&stripeWidth=6&stripeColors=5BCEFA,F5A9B8,FFFFFF,F5A9B8,5BCEFA)](https://translifeline.org/) [![LGBTQ+ friendly](https://pride-badges.pony.workers.dev/static/v1?label=lgbtq%2B%20friendly&stripeWidth=6&stripeColors=E40303,FF8C00,FFED00,008026,24408E,732982)](https://www.thetrevorproject.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-stable-brightgreen.svg)](https://rustup.rs/)
[![Platform](https://img.shields.io/badge/platform-windows-blue.svg)](https://microsoft.com/windows)

[![Sponsor](https://img.shields.io/badge/❤️-Sponsor-ea4aaa?logo=github)](https://github.com/sponsors/Michael-A-Kuykendall)

### 💝 Support Wanderlust

🚀 **If Wanderlust helps you, consider [sponsoring](https://github.com/sponsors/Michael-A-Kuykendall) — 100% of support goes to keeping it free forever.**

- **$5/month**: Coffee Hero ☕ — Eternal gratitude + name in [SPONSORS.md](SPONSORS.md)
- **$25/month**: Developer Supporter 🐛 — Priority bug response + roadmap influence
- **$100/month**: Corporate Backer 🏢 — Logo in README + release-note recognition
- **$500/month**: Enterprise Partner 🚀 — Prominent logo + monthly office hours + roadmap input

[**🎯 Become a Sponsor**](https://github.com/sponsors/Michael-A-Kuykendall) | See our amazing [sponsors](SPONSORS.md) 🙏

**Thank you to our sponsors:** [ZephyrCloudIO](https://github.com/ZephyrCloudIO) (Corporate Backer) · alistairheath (Coffee Hero)

---

**🧭 Wanderlust is the "Set and Forget" solution for Windows environment variables.**  
It runs silently in the background, keeping your `PATH` clean, deduplicated, and synchronized between Windows and POSIX shells.

## 🧭 What is Wanderlust?

Wanderlust is a **self-healing Windows scheduled task** that autonomously manages your System and User `PATH` environment variables. It eliminates the "Command Not Found" errors caused by installer bloat, broken registries, and stale shell sessions.

| Feature | Wanderlust 🧭 | Manual Editing | Other Tools |
|---------|---------------|----------------|-------------|
| **Autonomous Healing** | Runs every 30 mins (Silent) 🏆 | Never | Manual trigger only |
| **Path Deduplication** | Intelligent & Safe 🏆 | Error-prone | Basic |
| **Dead Link Removal** | Validates existence 🏆 | Manual check | Basic |
| **POSIX Integration** | Generates `/c/Users...` paths 🏆 | N/A | N/A |
| **Registry Health** | Scans `Uninstall` keys 🏆 | N/A | N/A |
| **Safety** | **Backup & Rollback** logic 🏆 | YOLO | Rare |
| **Elevation** | Semantic UAC handling 🏆 | "Run as Admin" | Hit or miss |

## 🎯 Strategic Focus: The Immaculate PATH

After years of dealing with broken dev environments, we built Wanderlust to enforce **Environment Hygiene** through what we call **The Immaculate PATH Philosophy**.

*   **Entropy Reduction**: Windows environments naturally degrade over time as installers add duplicate or conflicting entries. Wanderlust actively fights this entropy.
*   **Verification**: Before applying any changes, Wanderlust probes the new PATH with critical system binaries (`cmd`, `powershell`, `whoami`). If a probe fails, it **automatically rolls back**.
*   **Backup First**: Every change is preceded by a full `.reg` backup in `%LOCALAPPDATA%\wanderlust`.

## 🚀 Quick Start (10 seconds)

### Installation

Wanderlust is a single binary. It handles its own installation into the Windows Task Scheduler.

```powershell
# 1. Build or Download
cargo build --release

# 2. Install (Requires Admin)
.\target\release\wanderlust.exe install
```

That's it. Wanderlust now runs every 30 minutes in the background (hidden window), ensuring your PATH remains perfect.

### Manual Commands

You can run Wanderlust manually if you need immediate healing or diagnostics.

```powershell
# Dry Run (See what would happen)
wanderlust heal --dry-run

# Force Heal (Admin required)
wanderlust heal

# Doctor (Diagnostics)
wanderlust doctor

# Uninstall Service
wanderlust uninstall
```

## 🏗️ Technical Architecture

### The Healing Cycle

```
Discovery ──► Optimization ──► Application ──► Verification
   │                │                │                │
   ├─ Registry      ├─ Baseline      ├─ Backup Lock   ├─ cmd probes
   ├─ Common paths  ├─ Yank Guard    ├─ SHA-256 .reg  ├─ rollback on fail
   ├─ Existing PATH ├─ Subsystem     ├─ Write HKCU    └─ persist outcome
   └─ Uninstall     ├─ Drift check   └─ Broadcast
      orphans       └─ Build minimal
                        PATH
```

### Safety Mechanisms

1.  **Backup Lock**: File-based mutex prevents overlapping heal cycles from corrupting backups.
2.  **SHA-256 Checksums**: Every backup includes a sidecar checksum; corruption is detected before restore.
3.  **Grace Periods**: Missing entries (removable drives, network shares) get `YankGuard` grace cycles before removal.
4.  **Subsystem Protection**: WSL, Cygwin, and MSYS2 PATH entries are never silently removed.
5.  **Known-Good Snapshots**: After each successful heal, a snapshot is captured for drift detection.
6.  **Health Probes**: Post-write execution of `cmd`, `powershell`, and `whoami` to verify the system is usable.
7.  **Automatic Rollback**: If probes fail, the previous PATH is restored and the failure is logged.
8.  **Cross-Session Memory**: `HistoryStore` persists outcomes across runs for failure-streak escalation.

### POSIX Integration

Do you use **Git Bash** or **MSYS2**? Wanderlust automatically generates a `.wanderlust_posix` file in your home directory containing a colon-separated, forward-slash version of your PATH (e.g., `/c/Windows/System32`). You can source this in your `.bashrc` for blazing fast shell startups:

```bash
# In .bashrc
if [ -f ~/.wanderlust_posix ]; then
    export PATH=$(cat ~/.wanderlust_posix)
fi
```

## 📦 Source Code

Wanderlust is designed to be auditable. The full source is in this repository.

### Modules

| Module | Purpose |
|--------|---------|
| `src/cleaner.rs` | Core healing orchestration: discovery → optimization → application |
| `src/discovery.rs` | Crawls registry, common locations, and existing PATH for tools |
| `src/system.rs` | `SystemOps` trait + mock for isolated testing |
| `src/elevation.rs` | UAC privilege check and admin relaunch |
| `src/main.rs` | CLI entry point and scheduled-task install/uninstall |
| `src/backup.rs` | SHA-256 checksummed backup files with rotation and partial restore |
| `src/backup_lock.rs` | File-based mutex guarding backup writes |
| `src/baseline.rs` | Learns normal PATH shape across samples for anomaly detection |
| `src/invariant_ppt.rs` | Runtime invariant assertions for fail-closed safety |
| `src/logging.rs` | JSON-lines structured logging with rotation |
| `src/snapshot.rs` | Known-good PATH snapshots for drift detection |
| `src/store.rs` | Cross-session heal history in JSON-lines format |
| `src/subsystem.rs` | Protects WSL/Cygwin/MSYS2 PATH entries from removal |
| `src/uninstall.rs` | Detects orphaned PATH entries after program removal |
| `src/yank_guard.rs` | Grace-period tracking before removing missing entries |

## 📜 License

MIT License.

**Philosophy**: Your environment should work for you, not against you.

---

**Want to support Wanderlust?** [Become a sponsor](https://github.com/sponsors/Michael-A-Kuykendall) — every dollar keeps it free forever.

## Support

This project is a safe space. Trans rights are human rights.

If you or someone you love needs support:

- [The Trevor Project](https://www.thetrevorproject.org/) — 24/7 for LGBTQ+ young people. Call 1-866-488-7386 or text START to 678-678
- [Trans Lifeline](https://translifeline.org/) — peer support run by and for trans people. US: 877-565-8860
- [988 Suicide & Crisis Lifeline](https://988lifeline.org/) — call or text 988

