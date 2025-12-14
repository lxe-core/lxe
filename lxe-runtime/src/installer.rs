//! Installer - XDG-compliant installation logic
//!
//! Handles file installation, .desktop file creation, and icon installation.
//! V5 FIX: Now includes polkit integration for system installs.

use crate::extractor;
use crate::polkit;
use lxe_common::metadata::LxeMetadata;
use lxe_common::payload::PayloadInfo;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Installation target configuration
#[derive(Debug, Clone)]
pub struct InstallConfig {
    /// Base installation directory (e.g., ~/.local or /usr)
    pub base_dir: PathBuf,
    
    /// Whether this is a system-wide installation
    pub is_system: bool,
    
    /// Whether to create a .desktop file
    pub create_desktop_entry: bool,
    
    /// Whether to update the icon cache
    pub update_icon_cache: bool,
}

impl InstallConfig {
    /// Create config for user-local installation
    pub fn user_local() -> Self {
        let base = dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("~/.local/share"));
        
        Self {
            base_dir: base.parent().unwrap_or(&base).to_path_buf(),
            is_system: false,
            create_desktop_entry: true,
            update_icon_cache: true,
        }
    }
    
    /// Create config for system-wide installation
    pub fn system() -> Self {
        Self {
            base_dir: lxe_common::paths::system::base_dir(),
            is_system: true,
            create_desktop_entry: true,
            update_icon_cache: true,
        }
    }
    
    /// Get the applications directory path
    pub fn applications_dir(&self) -> PathBuf {
        self.base_dir.join("share").join("applications")
    }
    
    /// Get the bin directory path
    pub fn bin_dir(&self) -> PathBuf {
        self.base_dir.join("bin")
    }
    
    /// Get the icons directory path
    pub fn icons_dir(&self) -> PathBuf {
        self.base_dir.join("share").join("icons").join("hicolor")
    }
    
    /// Get where the app files are installed
    pub fn app_dir(&self, app_id: &str) -> PathBuf {
        self.base_dir.join("share").join(app_id)
    }
}

/// Perform silent installation (no GUI)
/// V5 FIX: Now checks polkit authorization before system installs
pub async fn install_silent(
    payload: &PayloadInfo,
    install_path: &Path,
    is_system: bool,
) -> Result<()> {
    let config = if is_system {
        // V5 FIX: Check/request polkit authorization for system installs
        if !polkit::is_root() {
            tracing::info!("System install requested, checking polkit authorization...");
            
            match polkit::request_authorization(polkit::ACTION_INSTALL_SYSTEM).await {
                Ok(true) => {
                    tracing::info!("Polkit authorization granted");
                }
                Ok(false) => {
                    anyhow::bail!(
                        "Authorization denied. System-wide installation requires administrator privileges.\n\
                         Try running with: pkexec {} --silent --system",
                        std::env::current_exe()?.display()
                    );
                }
                Err(e) => {
                    // Polkit not available or other error - give helpful message
                    anyhow::bail!(
                        "Could not request authorization: {}\n\n\
                         To install system-wide, either:\n\
                         1. Run as root: sudo {} --silent --system\n\
                         2. Ensure polkit is installed and the org.lxe.policy file is in /usr/share/polkit-1/actions/",
                        e,
                        std::env::current_exe()?.display()
                    );
                }
            }
        }
        InstallConfig::system()
    } else {
        InstallConfig {
            base_dir: install_path.to_path_buf(),
            ..InstallConfig::user_local()
        }
    };
    
    // Ensure target directory exists
    let target_dir = config.base_dir.join("share");
    fs::create_dir_all(&target_dir).await
        .context("Failed to create installation directory")?;
    
    // Extract files
    let (_rx, handle) = extractor::extract_async(payload.clone(), target_dir);
    
    // Wait for extraction to complete
    handle.await
        .context("Extraction task failed")??;
    
    // Install lxe-runtime to bin directory for uninstall support
    let runtime_path = install_runtime_binary(&config).await?;
    
    // Ensure ~/.local/bin is in user's PATH (first install only)
    if let Err(e) = ensure_path_configured(&config).await {
        tracing::warn!("Could not configure PATH: {}", e);
        // Non-fatal - continue with installation
    }
    
    // Create .desktop file (needs runtime_path for uninstall action)
    let desktop_path = create_desktop_entry(&payload.metadata, &config, &runtime_path).await?;
    
    // Create symlink in bin directory
    let symlink_path = create_bin_symlink(&payload.metadata, &config).await?;
    
    // Install icon
    let icon_path = if payload.metadata.icon.is_some() {
        install_icon(&payload.metadata, &config).await?
    } else {
        None
    };
    
    // Save manifest for tracking (enables clean uninstall)
    let mut manifest = crate::manifest::InstallManifest::new(
        payload.metadata.app_id.clone(),
        Some(payload.metadata.name.clone()),
        payload.metadata.version.clone(),
        is_system,
    );
    manifest.add_file(&config.app_dir(&payload.metadata.app_id));
    manifest.add_file(&desktop_path);
    manifest.add_file(&symlink_path);
    manifest.add_file(&runtime_path);
    if let Some(ref icon) = icon_path {
        manifest.add_file(icon);
    }
    manifest.save().await
        .context("Failed to save installation manifest")?;
    
    tracing::info!(
        "Successfully installed {} v{} to {:?}",
        payload.metadata.name,
        payload.metadata.version,
        config.base_dir
    );
    
    Ok(())
}

/// Install the runtime binary to the bin directory for persistent uninstall support
/// Public alias: install_runtime_to_bin
pub async fn install_runtime_binary(config: &InstallConfig) -> Result<PathBuf> {
    let bin_dir = config.bin_dir();
    fs::create_dir_all(&bin_dir).await?;
    
    let runtime_dest = bin_dir.join("lxe-runtime");
    
    // Only copy if not already present or if source is newer
    let current_exe = std::env::current_exe()
        .context("Failed to get current executable path")?;
    
    // Copy the runtime binary
    // ALWAYS overwrite to ensure we have the latest version of the runtime
    // This fixes issues where an old runtime doesn't support new flags (like --uninstall-gui)
    fs::copy(&current_exe, &runtime_dest).await
        .context("Failed to copy runtime binary to bin directory")?;
    
    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&runtime_dest, perms)?;
    }
    
    tracing::info!("Installed runtime to {:?}", runtime_dest);
    
    Ok(runtime_dest)
}

/// Alias for install_runtime_binary (used by GUI)
pub async fn install_runtime_to_bin(config: &InstallConfig) -> Result<PathBuf> {
    install_runtime_binary(config).await
}

/// Ensure ~/.local/bin is in the user's PATH
/// 
/// This automatically adds the PATH export to the user's shell config
/// on first install, so they don't have to do it manually.
/// 
/// Returns true if shell config was modified (user needs to restart terminal)
pub async fn ensure_path_configured(config: &InstallConfig) -> Result<bool> {
    // Skip for system installs (system bins are already in PATH)
    if config.is_system {
        return Ok(false);
    }
    
    let bin_dir = config.bin_dir();
    let bin_str = bin_dir.display().to_string();
    
    // Check if already in PATH
    if let Ok(path) = std::env::var("PATH") {
        if path.split(':').any(|p| p == bin_str || p == "$HOME/.local/bin" || p.ends_with("/.local/bin")) {
            tracing::debug!("~/.local/bin already in PATH");
            return Ok(false);
        }
    }
    
    // Find shell config file
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Cannot find home directory"))?;
    
    let shell_configs = [
        (".zshrc", "zsh"),
        (".bashrc", "bash"),
        (".profile", "sh"),
    ];
    
    let path_export = r#"
# Added by LXE installer - enables running LXE-installed apps from terminal
export PATH="$HOME/.local/bin:$PATH"
"#;
    
    for (config_file, shell_name) in shell_configs {
        let config_path = home.join(config_file);
        
        if config_path.exists() {
            // Check if we already added it
            let contents = fs::read_to_string(&config_path).await
                .unwrap_or_default();
            
            if contents.contains("/.local/bin") || contents.contains("Added by LXE") {
                tracing::debug!("{} already configured", config_file);
                return Ok(false);
            }
            
            // Append to the config file
            let mut file = fs::OpenOptions::new()
                .append(true)
                .open(&config_path)
                .await
                .context("Failed to open shell config")?;
            
            use tokio::io::AsyncWriteExt;
            file.write_all(path_export.as_bytes()).await
                .context("Failed to write to shell config")?;
            
            tracing::info!("Added ~/.local/bin to PATH in {} ({})", config_file, shell_name);
            
            return Ok(true);  // Modified shell config
        }
    }
    
    // No shell config found - create .profile as fallback
    let profile_path = home.join(".profile");
    fs::write(&profile_path, path_export).await
        .context("Failed to create .profile")?;
    
    tracing::info!("Created ~/.profile with PATH configuration");
    
    Ok(true)  // Modified shell config
}

/// Create a .desktop file for the application
pub async fn create_desktop_entry(
    metadata: &LxeMetadata,
    config: &InstallConfig,
    runtime_path: &PathBuf,
) -> Result<PathBuf> {
    let desktop_dir = config.applications_dir();
    fs::create_dir_all(&desktop_dir).await?;
    
    let desktop_path = desktop_dir.join(metadata.desktop_filename());
    
    let exec_path = config.app_dir(&metadata.app_id).join(&metadata.exec);
    
    // FIX: Use absolute path to icon instead of relying on icon theme lookup
    // This is more reliable and doesn't require gtk-update-icon-cache
    let icon_value = if let Some(ref icon_filename) = metadata.icon {
        let icon_path = config.app_dir(&metadata.app_id).join(icon_filename);
        if icon_path.exists() {
            // Use absolute path - most reliable method
            icon_path.display().to_string()
        } else {
            // Fallback to app_id for theme lookup
            metadata.app_id.clone()
        }
    } else {
        metadata.app_id.clone()
    };
    
    let terminal = if metadata.terminal { "true" } else { "false" };
    
    let content = format!(
        r#"[Desktop Entry]
Type=Application
Name={name}
Comment={comment}
Exec={exec}
Icon={icon}
Terminal={terminal}
Categories={categories}
StartupWMClass={wm_class}
X-LXE-Version={version}
X-LXE-AppId={app_id}
Actions=Uninstall;

[Desktop Action Uninstall]
Name=Uninstall {name}
Exec={runtime_path} --uninstall-gui {app_id}
"#,
        name = metadata.name,
        comment = metadata.description.as_deref().unwrap_or(&metadata.name),
        exec = exec_path.display(),
        icon = icon_value,
        terminal = terminal,
        categories = metadata.categories_string(),
        // Use custom wm_class if provided, otherwise derive from app_id
        wm_class = metadata.wm_class.as_deref()
            .unwrap_or_else(|| metadata.app_id.split('.').last().unwrap_or(&metadata.name)),
        version = metadata.version,
        app_id = metadata.app_id,
        // Use the installed runtime path for uninstall action
        runtime_path = runtime_path.display(),
    );
    
    fs::write(&desktop_path, content).await
        .context("Failed to write .desktop file")?;
    
    // Make it executable (some systems require this)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        std::fs::set_permissions(&desktop_path, perms)?;
    }
    
    Ok(desktop_path)
}

/// Create a symlink in the bin directory
pub async fn create_bin_symlink(
    metadata: &LxeMetadata,
    config: &InstallConfig,
) -> Result<PathBuf> {
    let bin_dir = config.bin_dir();
    fs::create_dir_all(&bin_dir).await?;
    
    let exec_name = Path::new(&metadata.exec)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| metadata.exec.clone());
    
    let link_path = bin_dir.join(&exec_name);
    let target_path = config.app_dir(&metadata.app_id).join(&metadata.exec);
    
    // Remove existing symlink if present
    if link_path.exists() || link_path.is_symlink() {
        fs::remove_file(&link_path).await.ok();
    }
    
    #[cfg(unix)]
    {
        tokio::fs::symlink(&target_path, &link_path).await
            .context("Failed to create symlink in bin directory")?;
    }
    
    Ok(link_path)
}

/// Install the application icon to the hicolor theme
pub async fn install_icon(
    metadata: &LxeMetadata,
    config: &InstallConfig,
) -> Result<Option<PathBuf>> {
    let icon_relative = match &metadata.icon {
        Some(icon) => icon,
        None => return Ok(None),
    };
    
    let source_icon = config.app_dir(&metadata.app_id).join(icon_relative);
    
    if !source_icon.exists() {
        tracing::warn!("Icon file not found: {:?}", source_icon);
        return Ok(None);
    }
    
    // Determine icon size from filename or default to scalable
    let (size_dir, extension) = if icon_relative.ends_with(".svg") {
        ("scalable", "svg")
    } else if icon_relative.ends_with(".png") {
        // Try to detect size from filename like "icon-48x48.png"
        ("48x48", "png")
    } else {
        ("48x48", "png")
    };
    
    let icon_dir = config.icons_dir()
        .join(size_dir)
        .join("apps");
    
    fs::create_dir_all(&icon_dir).await?;
    
    let target_icon = icon_dir.join(format!("{}.{}", metadata.app_id, extension));
    
    fs::copy(&source_icon, &target_icon).await
        .context("Failed to install icon")?;
    
    // Update icon cache if possible
    if config.update_icon_cache {
        update_icon_cache(&config.icons_dir()).await.ok();
    }
    
    Ok(Some(target_icon))
}

/// Update the GTK icon cache
async fn update_icon_cache(icons_dir: &Path) -> Result<()> {
    let output = tokio::process::Command::new("gtk-update-icon-cache")
        .arg("-f")
        .arg("-t")
        .arg(icons_dir)
        .output()
        .await;
    
    match output {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => {
            tracing::warn!(
                "gtk-update-icon-cache failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            Ok(())
        }
        Err(e) => {
            tracing::warn!("Could not run gtk-update-icon-cache: {}", e);
            Ok(())
        }
    }
}

/// Uninstall an application
/// 
/// SAFETY: This function validates paths before deletion to prevent
/// accidental deletion of system directories.
/// 
/// V5 FIX: Checks polkit for system uninstalls
pub async fn uninstall(
    app_id: &str,
    config: &InstallConfig,
) -> Result<()> {
    use lxe_common::paths::safety;
    
    // Check polkit for system uninstalls
    if config.is_system && !polkit::is_root() {
        match polkit::request_authorization(polkit::ACTION_UNINSTALL_SYSTEM).await {
            Ok(true) => {
                tracing::info!("Polkit authorization granted for uninstall");
            }
            Ok(false) => {
                anyhow::bail!("Authorization denied for system uninstall");
            }
            Err(e) => {
                anyhow::bail!("Could not request uninstall authorization: {}", e);
            }
        }
    }
    
    // Remove app directory with SAFETY CHECK
    let app_dir = config.app_dir(app_id);
    if app_dir.exists() {
        // SAFETY: Validate path before deletion
        if !safety::is_safe_to_delete(&app_dir, app_id) {
            anyhow::bail!(
                "SAFETY: Refusing to delete {:?} - path does not match expected pattern for app {}",
                app_dir, app_id
            );
        }
        
        tracing::info!("Removing app directory: {:?}", app_dir);
        fs::remove_dir_all(&app_dir).await
            .context("Failed to remove application directory")?;
    }
    
    // Remove .desktop file
    let desktop_file = config.applications_dir().join(format!("{}.desktop", app_id));
    if desktop_file.exists() {
        tracing::info!("Removing desktop entry: {:?}", desktop_file);
        fs::remove_file(&desktop_file).await
            .context("Failed to remove .desktop file")?;
    }
    
    // Remove bin symlinks - find any symlinks pointing to this app's directory
    let bin_dir = config.bin_dir();
    if bin_dir.exists() {
        let app_dir = config.app_dir(app_id);
        if let Ok(mut entries) = tokio::fs::read_dir(&bin_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.is_symlink() {
                    // Check if this symlink points to our app directory
                    if let Ok(target) = tokio::fs::read_link(&path).await {
                        if target.starts_with(&app_dir) || target.to_string_lossy().contains(app_id) {
                            tracing::info!("Removing bin symlink: {:?}", path);
                            fs::remove_file(&path).await.ok();
                        }
                    }
                }
            }
        }
    }
    
    // Remove icon (all sizes) - using paths module
    for size in lxe_common::paths::icons::SIZES {
        let icon_dir = config.icons_dir().join(size).join("apps");
        for ext in ["svg", "png"] {
            let icon_path = icon_dir.join(format!("{}.{}", app_id, ext));
            if icon_path.exists() {
                tracing::info!("Removing icon: {:?}", icon_path);
                fs::remove_file(&icon_path).await.ok();
            }
        }
    }
    
    // Also check scalable
    let scalable_dir = config.icons_dir().join("scalable").join("apps");
    for ext in ["svg", "png"] {
        let icon_path = scalable_dir.join(format!("{}.{}", app_id, ext));
        if icon_path.exists() {
            fs::remove_file(&icon_path).await.ok();
        }
    }
    
    tracing::info!("Uninstallation complete for {}", app_id);
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_install_config_paths() {
        let config = InstallConfig {
            base_dir: PathBuf::from("/home/user/.local"),
            is_system: false,
            create_desktop_entry: true,
            update_icon_cache: true,
        };
        
        assert_eq!(
            config.applications_dir(),
            PathBuf::from("/home/user/.local/share/applications")
        );
        assert_eq!(
            config.bin_dir(),
            PathBuf::from("/home/user/.local/bin")
        );
        assert_eq!(
            config.app_dir("com.example.App"),
            PathBuf::from("/home/user/.local/share/com.example.App")
        );
    }
}
