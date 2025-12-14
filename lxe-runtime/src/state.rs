//! Installation State Machine
//!
//! Detects the current installation state and determines the wizard mode.
//! This enables the "Maintenance Mode" for already-installed applications.

use lxe_common::metadata::LxeMetadata;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// Installation state detected from the system
#[derive(Debug, Clone)]
pub enum InstallState {
    /// No existing installation found
    Fresh,
    
    /// Same version installed and files are intact
    Installed {
        install_path: PathBuf,
        version: String,
        is_system: bool,
    },
    
    /// Older version installed - upgrade available
    Upgradeable {
        install_path: PathBuf,
        old_version: String,
        new_version: String,
        is_system: bool,
    },
    
    /// Newer version installed - downgrade blocked
    Downgrade {
        install_path: PathBuf,
        installed_version: String,
        package_version: String,
    },
    
    /// Installation exists but files are missing or corrupted
    Corrupted {
        install_path: PathBuf,
        is_system: bool,
    },
}

/// Wizard mode derived from installation state
#[derive(Debug, Clone)]
pub enum WizardMode {
    /// Fresh installation
    Install,
    
    /// Maintenance mode for existing installation
    Maintenance {
        current_version: String,
        install_path: PathBuf,
        can_upgrade: bool,
        can_repair: bool,
        is_system: bool,
    },
}

impl InstallState {
    /// Derive the wizard mode from the installation state
    pub fn to_wizard_mode(&self, new_version: &str) -> WizardMode {
        match self {
            InstallState::Fresh => WizardMode::Install,
            
            InstallState::Installed { install_path, version, is_system } => {
                WizardMode::Maintenance {
                    current_version: version.clone(),
                    install_path: install_path.clone(),
                    can_upgrade: false,
                    can_repair: false,
                    is_system: *is_system,
                }
            }
            
            InstallState::Upgradeable { install_path, old_version, is_system, .. } => {
                WizardMode::Maintenance {
                    current_version: old_version.clone(),
                    install_path: install_path.clone(),
                    can_upgrade: true,
                    can_repair: false,
                    is_system: *is_system,
                }
            }
            
            InstallState::Downgrade { install_path, installed_version, .. } => {
                WizardMode::Maintenance {
                    current_version: installed_version.clone(),
                    install_path: install_path.clone(),
                    can_upgrade: false,
                    can_repair: false,
                    is_system: false,
                }
            }
            
            InstallState::Corrupted { install_path, is_system } => {
                WizardMode::Maintenance {
                    current_version: "unknown".to_string(),
                    install_path: install_path.clone(),
                    can_upgrade: false,
                    can_repair: true,
                    is_system: *is_system,
                }
            }
        }
    }
}

/// Detect the installation state for a package
pub fn detect_install_state(metadata: &LxeMetadata) -> InstallState {
    // Check user-local installation first
    if let Some(local_dir) = dirs::data_local_dir() {
        let desktop_path = local_dir
            .join("applications")
            .join(metadata.desktop_filename());
        
        if let Some(state) = check_installation(&desktop_path, metadata, false) {
            return state;
        }
    }
    
    // Check system-wide installation
    let system_desktop = PathBuf::from("/usr/share/applications")
        .join(metadata.desktop_filename());
    
    if let Some(state) = check_installation(&system_desktop, metadata, true) {
        return state;
    }
    
    // Also check /usr/local
    let local_system_desktop = PathBuf::from("/usr/local/share/applications")
        .join(metadata.desktop_filename());
    
    if let Some(state) = check_installation(&local_system_desktop, metadata, true) {
        return state;
    }
    
    InstallState::Fresh
}

/// Check a specific .desktop file location
fn check_installation(
    desktop_path: &Path,
    metadata: &LxeMetadata,
    is_system: bool,
) -> Option<InstallState> {
    if !desktop_path.exists() {
        return None;
    }
    
    // Parse the .desktop file to find exec path and version
    let desktop_info = match parse_desktop_file(desktop_path) {
        Ok(info) => info,
        Err(_) => {
            // .desktop exists but is invalid - treat as corrupted
            return Some(InstallState::Corrupted {
                install_path: desktop_path.parent()?.parent()?.to_path_buf(),
                is_system,
            });
        }
    };
    
    // Check if the executable exists
    let exec_path = Path::new(&desktop_info.exec);
    if !exec_path.exists() {
        return Some(InstallState::Corrupted {
            install_path: desktop_path.parent()?.parent()?.to_path_buf(),
            is_system,
        });
    }
    
    // Compare versions
    let install_path = exec_path.parent()?.parent()?.to_path_buf();
    
    match compare_versions(&desktop_info.version, &metadata.version) {
        std::cmp::Ordering::Equal => {
            Some(InstallState::Installed {
                install_path,
                version: desktop_info.version,
                is_system,
            })
        }
        std::cmp::Ordering::Less => {
            Some(InstallState::Upgradeable {
                install_path,
                old_version: desktop_info.version,
                new_version: metadata.version.clone(),
                is_system,
            })
        }
        std::cmp::Ordering::Greater => {
            Some(InstallState::Downgrade {
                install_path,
                installed_version: desktop_info.version,
                package_version: metadata.version.clone(),
            })
        }
    }
}

/// Info extracted from a .desktop file
#[derive(Debug)]
struct DesktopInfo {
    exec: String,
    version: String,
}

/// Parse a .desktop file to extract exec path and version
fn parse_desktop_file(path: &Path) -> Result<DesktopInfo> {
    let content = std::fs::read_to_string(path)?;
    
    let mut exec = None;
    let mut version = String::from("0.0.0");
    
    for line in content.lines() {
        let line = line.trim();
        
        if let Some(value) = line.strip_prefix("Exec=") {
            // Extract just the executable path (before any %u, %f args)
            exec = Some(
                value
                    .split_whitespace()
                    .next()
                    .unwrap_or(value)
                    .to_string()
            );
        }
        
        // Look for X-LXE-Version custom field we add
        if let Some(value) = line.strip_prefix("X-LXE-Version=") {
            version = value.to_string();
        }
    }
    
    Ok(DesktopInfo {
        exec: exec.ok_or_else(|| anyhow::anyhow!("No Exec= found in .desktop file"))?,
        version,
    })
}

/// Compare two semantic version strings
fn compare_versions(v1: &str, v2: &str) -> std::cmp::Ordering {
    let parse = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|s| s.parse().ok())
            .collect()
    };
    
    let v1_parts = parse(v1);
    let v2_parts = parse(v2);
    
    for (a, b) in v1_parts.iter().zip(v2_parts.iter()) {
        match a.cmp(b) {
            std::cmp::Ordering::Equal => continue,
            other => return other,
        }
    }
    
    v1_parts.len().cmp(&v2_parts.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compare_versions() {
        assert_eq!(compare_versions("1.0.0", "1.0.0"), std::cmp::Ordering::Equal);
        assert_eq!(compare_versions("1.0.0", "2.0.0"), std::cmp::Ordering::Less);
        assert_eq!(compare_versions("2.0.0", "1.0.0"), std::cmp::Ordering::Greater);
        assert_eq!(compare_versions("1.0", "1.0.0"), std::cmp::Ordering::Less);
        assert_eq!(compare_versions("1.2.3", "1.2.4"), std::cmp::Ordering::Less);
    }

    #[test]
    fn test_wizard_mode_from_fresh() {
        let state = InstallState::Fresh;
        match state.to_wizard_mode("1.0.0") {
            WizardMode::Install => {}
            _ => panic!("Expected Install mode"),
        }
    }

    #[test]
    fn test_wizard_mode_from_installed() {
        let state = InstallState::Installed {
            install_path: PathBuf::from("/home/user/.local"),
            version: "1.0.0".to_string(),
            is_system: false,
        };
        match state.to_wizard_mode("1.0.0") {
            WizardMode::Maintenance { can_upgrade, .. } => {
                assert!(!can_upgrade);
            }
            _ => panic!("Expected Maintenance mode"),
        }
    }
}
