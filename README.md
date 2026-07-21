# Wanderlust: The Self-Healing Windows PATH Manager 🧭✨

![Wanderlust Splash](https://raw.githubusercontent.com/Michael-A-Kuykendall/wanderlust/refs/heads/master/assets/wanderlust-splash.jpg)

[![Crates.io](https://img.shields.io/crates/v/wanderlust.svg)](https://crates.io/crates/wanderlust)
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

Wanderlust is a **self-healing, administration-level daemon** for Windows that autonomously manages your System and User `PATH` environment variables. It eliminates the "Command Not Found" errors caused by installer bloat, broken registries, and stale shell sessions.

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
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  Discovery      │────▶│   Optimization   │────▶│   Application   │
│  (Scanners)     │     │   (Builder)      │     │   (Registry)    │
└─────────────────┘     └──────────────────┘     └─────────────────┘
       │                        │                        │
       ▼                        ▼                        ▼
 1. Registry Scan         4. Normalize Paths       7. Backup .reg
    (HKCU + HKLM)            (Lowercase/Trim)         (%LOCALAPPDATA%)
 2. Common Locations      5. Deduplicate           8. Write HKCU PATH
    (.cargo, .local)         (Preserve Order)      9. Broadcast Change
 3. Existing PATH         6. Prioritize System        (WM_SETTINGCHANGE)
                             (System32 First)
```

### Safety Mechanisms

1.  **Essential Anchoring**: Hardcoded preservation of `C:\Windows\System32`, `PowerShell`, and `OpenSSH` to prevent "bricking" the OS.
2.  **Health Probes**: Post-write execution of `cmd /c ver` and `whoami`.
3.  **Atomic-ish updates**: Backups are written to disk before Registry modification.

### POSIX Integration

Do you use **Git Bash** or **MSYS2**? Wanderlust automatically generates a `.wanderlust_posix` file in your home directory containing a colon-separated, forward-slash version of your PATH (e.g., `/c/Windows/System32`). You can source this in your `.bashrc` for blazing fast shell startups:

```bash
# In .bashrc
if [ -f ~/.wanderlust_posix ]; then
    export PATH=$(cat ~/.wanderlust_posix)
fi
```

## 📦 Source Code & Audit

Wanderlust is designed to be auditable. The full source code is available in this repository, but for a quick audit of the logic, see [WANDERLUST_SOURCE.md](WANDERLUST_SOURCE.md).

### Components

*   `src/cleaner.rs`: The core logic for healing, backup, and rollback.
*   `src/discovery.rs`: The "Search Engine" that finds your installed tools (even if they aren't in PATH).
*   `src/elevation.rs`: UAC token manipulation to ensure we can write to the Registry.
*   `src/main.rs`: The CLI and Scheduler logic.

## 📜 License

MIT License.

**Philosophy**: Your environment should work for you, not against you.

---

**💰 Reminder**: If this tool saved your dev environment, remember the low, low price of **[$401,543,114!!](https://github.com/sponsors/Michael-A-Kuykendall)**.

