//! LXE Runtime - Self-extracting Linux package installer
//!
//! This is the main entry point for the LXE runtime. It handles:
//! 1. CLI argument parsing (--silent, --install-dir, etc.)
//! 2. Reading the embedded payload from its own binary
//! 3. Detecting installation state (fresh vs maintenance mode)
//! 4. Launching the GTK4/Libadwaita wizard or silent installer

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

// Import from the runtime library crate
use lxe_runtime::{installer, libloader, manifest, polkit, state, ui};
use lxe_common::{paths, payload};

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
    
    /// Uninstall an application by its app ID
    #[arg(long, value_name = "APP_ID")]
    uninstall: Option<String>,
    
    /// Uninstall with GUI (for desktop shortcut)
    #[arg(long, value_name = "APP_ID")]
    uninstall_gui: Option<String>,
    
    /// List all installed LXE applications
    #[arg(long)]
    list: bool,
}

fn main() -> Result<()> {
    // FIRST: Initialize bundled libraries if present
    // This must happen before ANY library initialization (GTK, etc.)
    let using_bundled = libloader::init_bundled_libs();
    
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
    
    // Handle --uninstall flag (CLI mode)
    if let Some(app_id) = &args.uninstall {
        return run_uninstall(app_id, args.system);
    }
    
    // Handle --uninstall-gui flag (GUI mode)
    if let Some(app_id) = &args.uninstall_gui {
        return run_uninstall_gui(app_id, args.system);
    }
    
    // Handle --list flag
    if args.list {
        return list_installed_apps();
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

    // Print installation banner
    println!("📦 LXE Silent Installer");
    println!();
    println!("   Package: {} v{}", payload.metadata.name, payload.metadata.version);
    println!("   App ID:  {}", payload.metadata.app_id);
    println!();

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let install_path = args.install_dir.unwrap_or_else(|| {
            if args.system {
                paths::system::base_dir()
            } else {
                paths::user::base_dir().unwrap_or_else(|| PathBuf::from("~/.local"))
            }
        });

        println!("📁 Installing to: {:?}", install_path);
        println!();
        
        // Check if PATH is already configured in shell config (before install modifies it)
        let path_already_configured = {
            let home = dirs::home_dir();
            home.map(|h| {
                // Check if any shell config already has .local/bin
                let configs = [".zshrc", ".bashrc", ".profile"];
                configs.iter().any(|f| {
                    std::fs::read_to_string(h.join(f))
                        .map(|c| c.contains(".local/bin"))
                        .unwrap_or(false)
                })
            }).unwrap_or(false)
        };
        
        let result = installer::install_silent(&payload, &install_path, args.system).await;
        
        match &result {
            Ok(()) => {
                println!();
                println!("✅ Installation complete!");
                println!();
                println!("   Find '{}' in your application menu.", payload.metadata.name);
                
                // Only show terminal restart note if we configured PATH this session
                if !path_already_configured && !args.system {
                    println!();
                    println!("   💡 To run '{}' from terminal:", payload.metadata.exec);
                    println!("      Restart your terminal (or run: source ~/.zshrc)");
                }
            }
            Err(e) => {
                eprintln!();
                eprintln!("❌ Installation failed: {}", e);
            }
        }
        
        result
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

/// Uninstall an application by its app ID
fn run_uninstall(app_id: &str, is_system: bool) -> Result<()> {
    println!("🗑️  LXE Uninstaller");
    println!();
    
    // Check if app is installed via manifest
    let rt = tokio::runtime::Runtime::new()?;
    
    let manifest = rt.block_on(manifest::InstallManifest::load(app_id))?;
    
    let config = if is_system {
        installer::InstallConfig::system()
    } else {
        installer::InstallConfig::user_local()
    };
    
    match manifest {
        Some(m) => {
            println!("Found: {} v{}", app_id, m.version);
            println!("Installed at: {}", m.installed_at);
            println!();
        }
        None => {
            println!("No manifest found for '{}', but will attempt cleanup anyway.", app_id);
            println!();
        }
    }
    
    // Run uninstall
    println!("Removing files...");
    rt.block_on(installer::uninstall(app_id, &config))?;
    
    // Remove manifest
    rt.block_on(manifest::InstallManifest::delete(app_id))?;
    
    println!();
    println!("✅ {} has been uninstalled.", app_id);
    
    Ok(())
}

/// Uninstall with GUI - shows confirmation dialog then uninstalls
/// Uninstall with GUI - shows confirmation dialog then uninstalls
fn run_uninstall_gui(app_id: &str, is_system: bool) -> Result<()> {
    use gtk::prelude::*;
    
    info!("LXE Uninstaller GUI for {}", app_id);
    
    // Check if app is installed via manifest
    let rt = tokio::runtime::Runtime::new()?;
    let manifest = rt.block_on(manifest::InstallManifest::load(app_id))?;
    
    // Use stored name from manifest, or fallback to app_id
    let app_name = manifest
        .as_ref()
        .and_then(|m| m.name.clone())
        .or_else(|| {
            // Fallback: guess from app_id
            let name_part = app_id.split('.').last().unwrap_or(app_id);
            // Handle common cases
            match app_id {
                "com.microsoft.vscode" => Some("Visual Studio Code".to_string()),
                _ => Some(name_part.to_string()),
            }
        })
        .unwrap_or_else(|| app_id.to_string());
    
    // Format branding text
    let app_display_name = app_name;
    
    let version = manifest
        .as_ref()
        .map(|m| m.version.clone())
        .unwrap_or_else(|| "unknown".to_string());
        
    let content_text = if version == "unknown" {
        "This application will be removed from your system.".to_string()
    } else {
        format!("Version {} will be removed from your system.", version)
    };
    
    // Initialize GTK
    let gtk_startup = std::time::Instant::now();
    gtk::init().expect("Failed to initialize GTK");
    info!("GTK initialized in {:?}", gtk_startup.elapsed());
    
    // Create confirmation dialog
    let dialog = gtk::MessageDialog::builder()
        .message_type(gtk::MessageType::Question)
        .buttons(gtk::ButtonsType::None)
        .title("Uninstall")  // Concise title
        .text(&format!("Uninstall {}?", app_display_name))
        .secondary_text(&format!(
            "{}\nThis action cannot be undone.",
            content_text
        ))
        .modal(true)
        .build();
    
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    dialog.add_button("Uninstall", gtk::ResponseType::Accept);
    
    // Style the Uninstall button as destructive
    if let Some(button) = dialog.widget_for_response(gtk::ResponseType::Accept) {
        button.add_css_class("destructive-action");
    }
    
    let app_id_owned = app_id.to_string();
    let is_system_owned = is_system;
    
    // Use a shared flag to track when we're done
    let done = std::rc::Rc::new(std::cell::RefCell::new(false));
    let done_clone = done.clone();
    
    dialog.connect_response(move |dialog, response| {
        dialog.close();
        
        if response == gtk::ResponseType::Accept {
            println!("🗑️  Uninstalling {}...", app_id_owned);
            
            let config = if is_system_owned {
                installer::InstallConfig::system()
            } else {
                installer::InstallConfig::user_local()
            };
            
            let rt = tokio::runtime::Runtime::new().unwrap();
            
            // Run uninstall
            if let Err(e) = rt.block_on(installer::uninstall(&app_id_owned, &config)) {
                eprintln!("Error uninstalling: {}", e);
            }
            // Remove manifest
            let _ = rt.block_on(manifest::InstallManifest::delete(&app_id_owned));
            
            println!("✅ {} has been uninstalled.", app_id_owned);
        }
        
        *done_clone.borrow_mut() = true;
    });
    
    dialog.show();
    
    // Run GTK main loop
    let main_context = glib::MainContext::default();
    while !*done.borrow() {
        main_context.iteration(true);
    }
    
    Ok(())
}

/// List all installed LXE applications
fn list_installed_apps() -> Result<()> {
    println!("📦 Installed LXE Applications");
    println!();
    
    let rt = tokio::runtime::Runtime::new()?;
    let apps = rt.block_on(manifest::InstallManifest::list_installed())?;
    
    if apps.is_empty() {
        println!("  (no applications installed via LXE)");
        return Ok(());
    }
    
    for app_id in apps {
        if let Some(manifest) = rt.block_on(manifest::InstallManifest::load(&app_id))? {
            let location = if manifest.is_system { "system" } else { "user" };
            println!("  • {} v{} ({})", app_id, manifest.version, location);
        } else {
            println!("  • {} (manifest corrupted)", app_id);
        }
    }
    
    Ok(())
}
