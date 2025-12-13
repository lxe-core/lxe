//! LXE Runtime - Self-extracting Linux package installer
//!
//! This is the main entry point for the LXE runtime. It handles:
//! 1. CLI argument parsing (--silent, --install-dir, etc.)
//! 2. Reading the embedded payload from its own binary
//! 3. Detecting installation state (fresh vs maintenance mode)
//! 4. Launching the GTK4/Libadwaita wizard or silent installer

mod extractor;
mod installer;
mod metadata;
mod paths;
mod payload;
mod polkit;
mod signing;
mod state;
mod ui;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

/// LXE Runtime - Linux Executable Environment Installer
#[derive(Parser, Debug)]
#[command(name = "lxe-runtime")]
#[command(about = "Self-extracting Linux application installer")]
#[command(version)]
struct Args {
    /// Run installation silently without GUI
    #[arg(long)]
    silent: bool,

    /// Custom installation directory
    #[arg(long, value_name = "DIR")]
    install_dir: Option<PathBuf>,

    /// Install system-wide (requires root/polkit)
    #[arg(long)]
    system: bool,

    /// Measure and display startup time (for benchmarking)
    #[arg(long, hide = true)]
    measure_startup: bool,

    /// Force reinstall even if already installed
    #[arg(long)]
    force: bool,
    
    /// Install the polkit policy file (requires root)
    /// Run this once before using --system flag
    #[arg(long)]
    install_policy: bool,
}

fn main() -> Result<()> {
    let startup_time = std::time::Instant::now();

    // Parse CLI arguments
    let args = Args::parse();

    // Initialize logging - only if not in silent mode
    if !args.silent {
        let _ = FmtSubscriber::builder()
            .with_max_level(Level::INFO)
            .with_target(false)
            .without_time()
            .try_init();
    }

    // Benchmark mode
    if args.measure_startup {
        let elapsed = startup_time.elapsed();
        println!("Startup time: {:?}", elapsed);
        if elapsed.as_millis() > 200 {
            eprintln!("WARNING: Startup exceeded 200ms target!");
        }
        return Ok(());
    }
    
    // V6 FIX: Handle --install-policy flag
    if args.install_policy {
        return install_polkit_policy();
    }

    info!("LXE Runtime v{}", env!("CARGO_PKG_VERSION"));

    // Read our own binary to extract payload metadata
    let exe_path = std::env::current_exe()?;
    info!("Executable: {:?}", exe_path);

    // Parse the embedded payload
    let payload_info = match payload::read_payload_info(&exe_path) {
        Ok(info) => {
            info!("Package: {} v{}", info.metadata.name, info.metadata.version);
            Some(info)
        }
        Err(e) => {
            // No payload embedded - this is the development/packer binary
            info!("No embedded payload found: {}", e);
            None
        }
    };

    if args.silent {
        // Silent installation mode
        run_silent_install(args, payload_info)
    } else {
        // Launch GTK4 GUI
        run_gui(args, payload_info, startup_time)
    }
}

/// Install the polkit policy file for system-wide installations
fn install_polkit_policy() -> Result<()> {
    println!("LXE Polkit Policy Installer");
    println!();
    
    if !polkit::is_root() {
        eprintln!("Error: Installing the polkit policy requires root privileges.");
        eprintln!();
        eprintln!("Run with sudo:");
        eprintln!("  sudo {} --install-policy", std::env::current_exe()?.display());
        std::process::exit(1);
    }
    
    match polkit::install_policy_file() {
        Ok(true) => {
            println!("✅ Polkit policy installed successfully to:");
            println!("   {}", polkit::POLICY_FILE_PATH);
            println!();
            println!("You can now use --system flag for system-wide installations.");
            Ok(())
        }
        Ok(false) => {
            println!("ℹ️  Polkit policy already exists at:");
            println!("   {}", polkit::POLICY_FILE_PATH);
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to install polkit policy: {}", e);
            Err(e)
        }
    }
}

fn run_silent_install(
    args: Args,
    payload_info: Option<payload::PayloadInfo>,
) -> Result<()> {
    let payload = payload_info.ok_or_else(|| {
        anyhow::anyhow!("No payload embedded. Cannot run silent install on packer binary.")
    })?;

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let install_path = args.install_dir.unwrap_or_else(|| {
            if args.system {
                paths::system::base_dir()
            } else {
                paths::user::base_dir().unwrap_or_else(|| PathBuf::from("~/.local"))
            }
        });

        installer::install_silent(&payload, &install_path, args.system).await
    })
}

fn run_gui(
    args: Args,
    payload_info: Option<payload::PayloadInfo>,
    startup_time: std::time::Instant,
) -> Result<()> {
    // V9 FIX: Check for display availability before initializing GTK
    // This provides a helpful message instead of a panic
    if std::env::var("DISPLAY").is_err() && std::env::var("WAYLAND_DISPLAY").is_err() {
        eprintln!("Error: No display server detected (X11 or Wayland).");
        eprintln!();
        eprintln!("If you're running via SSH, please use:");
        eprintln!("  {} --silent", std::env::current_exe()?.display());
        eprintln!();
        eprintln!("Or enable X11 forwarding with:");
        eprintln!("  ssh -X user@host");
        std::process::exit(1);
    }
    
    // Initialize GTK
    if let Err(e) = gtk::init() {
        eprintln!("Failed to initialize GTK4: {}", e);
        eprintln!("Please ensure GTK4 is installed on your system.");
        std::process::exit(1);
    }
    
    if let Err(e) = adw::init() {
        eprintln!("Failed to initialize Libadwaita: {}", e);
        eprintln!("Please ensure Libadwaita is installed on your system.");
        std::process::exit(1);
    }

    info!("GTK4/Libadwaita initialized in {:?}", startup_time.elapsed());

    // Create and run the application
    let app = ui::app::LxeApplication::new(payload_info, args.force);
    
    // Run the GTK main loop
    let exit_code = app.run();
    
    std::process::exit(exit_code.into());
}
