//! # Wanderlust: The Main Entry Point
//!
//! This module handles Command Line Interface (CLI) parsing, logging initialization,
//! and dispatching commands to the appropriate sub-modules. It is the orchestrator
//! of the Wanderlust application.
//!
//! The application is designed to be run as an Administrator (for `heal`, `install`, `uninstall`).

use clap::{Parser, Subcommand};
use log::{info, error, warn, LevelFilter};
use simplelog::{Config, SimpleLogger};

mod cleaner;
mod discovery;
mod elevation;

/// The primary Command Line Interface (CLI) configuration.
///
/// Uses `clap` for sub-command parsing and help generation.
#[derive(Parser)]
#[command(name = "wanderlust")]
#[command(about = "A self-healing PATH manager for Windows", long_about = None)]
struct Cli {
    /// The sub-command to execute (heal, doctor, install, etc.).
    #[command(subcommand)]
    command: Option<Commands>,

    /// Turn on verbose logging.
    ///
    /// - `-v`: Debug
    /// - `-vv`: Trace
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
}

/// Available sub-commands for the Wanderlust utility.
#[derive(Subcommand)]
enum Commands {
    /// Analyze and fix the PATH once.
    ///
    /// This command will:
    /// 1. Discover all tools on the system.
    /// 2. Construct a minimal, deduplicated PATH.
    /// 3. Update the Registry.
    /// 4. Broadcast the change to the system.
    Heal {
        /// Dry run: don't actually change the registry, just print what would happen.
        ///
        /// Useful for auditing what Wanderlust *would* do without risk.
        #[arg(long)]
        dry_run: bool,
    },
    /// Inspect the PATH and report issues.
    ///
    /// Checks for:
    /// - Duplicate entries.
    /// - Broken paths (directories that don't exist).
    /// - Shadowed commands.
    Doctor,
    /// Install as a scheduled task (runs every 30 minutes).
    ///
    /// This creates a Windows Scheduled Task running with highest privileges.
    Install,
    /// Uninstall the scheduled task.
    ///
    /// Removes the `WanderlustHeal` task from the scheduler.
    Uninstall,
}

fn main() {
    let cli = Cli::parse();

    // Determine log level based on verbosity flag
    let log_level = match cli.verbose {
        0 => LevelFilter::Info,
        1 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    };

    // Initialize logger
    // We ignore the result here as logging failure shouldn't crash the startup
    let _ = SimpleLogger::init(log_level, Config::default());

    match &cli.command {
        Some(Commands::Heal { dry_run }) => {
            // Check for elevation if we are going to write to the Registry (non-dry-run)
            if !*dry_run && !elevation::is_elevated() {
                warn!("Access might be denied. Attempting to elevate privileges...");
                if elevation::relaunch_as_admin() {
                    // If relaunch was successful, the new process handles it. We exit.
                    return;
                } else {
                    error!("Failed to elevate. Continuing with current privileges (this might fail)...");
                }
            }

            info!("Starting self-healing process...");
            if let Err(e) = cleaner::heal_path(*dry_run) {
                error!("Failed to heal PATH: {}", e);
                std::process::exit(1);
            }
        }
        Some(Commands::Doctor) => {
            if let Err(e) = cleaner::doctor() {
                error!("Doctor check failed: {}", e);
            }
        }
        Some(Commands::Install) => {
            // Installation strictly requires Admin rights to modify Scheduled Tasks.
            if !elevation::is_elevated() {
                 warn!("Installation requires admin rights. Attempting to elevate...");
                 if elevation::relaunch_as_admin() {
                     return;
                 }
                 error!("Elevation failed. Installation will likely fail.");
            }

            // Reliable way to get the absolute path of the currently running binary.
            let exe_path = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("wanderlust.exe"));
            let exe_str = exe_path.to_string_lossy();

            info!("Installing scheduled task 'WanderlustHeal'...");

            // Create a scheduled task that runs "wanderlust heal" every 30 minutes.
            //
            // TRICK: We wrap the call in PowerShell with `-WindowStyle Hidden`.
            // By default, scheduled tasks might flash a console window. This wrapper prevents that annoyance.
            //
            // Arguments breakdown:
            // /SC MINUTE /MO 30 -> Schedule every 30 minutes
            // /RL HIGHEST       -> Run with highest privileges (Admin)
            // /NP               -> No Password required (can run non-interactively)
            // /F                -> Force create (overwrite existing)

            let arg_command = format!("powershell -WindowStyle Hidden -Command '& \"{}\" heal'", exe_str);

            let status = std::process::Command::new("schtasks")
                .arg("/Create")
                .arg("/SC")
                .arg("MINUTE")
                .arg("/MO")
                .arg("30")
                .arg("/TN")
                .arg("WanderlustHeal")
                .arg("/TR")
                .arg(arg_command)
                .arg("/F") 
                .arg("/RL")
                .arg("HIGHEST") 
                .arg("/NP")    
                .status();

            match status {
                Ok(s) if s.success() => info!("Successfully installed scheduled task. Wanderlust will run every 30 minutes (hidden)."),     
                Ok(s) => error!("Failed to install task. Exit code: {:?}", s.code()),
                Err(e) => error!("Failed to execute schtasks: {}", e),
            }
        }
        Some(Commands::Uninstall) => {
            info!("Uninstalling scheduled task 'WanderlustHeal'...");

            let status = std::process::Command::new("schtasks")
                .arg("/Delete")
                .arg("/TN")
                .arg("WanderlustHeal")
                .arg("/F")
                .status();

             match status {
                Ok(s) if s.success() => info!("Successfully uninstalled scheduled task."),
                Ok(s) => error!("Failed to uninstall task (maybe it doesn't exist?). Exit code: {:?}", s.code()),
                Err(e) => error!("Failed to execute schtasks: {}", e),
            }
        }
        None => {
            // Default behavior if no command: print the help message
            use clap::CommandFactory;
            let _ = Cli::command().print_help();
        }
    }
}
