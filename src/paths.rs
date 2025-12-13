//! Centralized Path Definitions
//!
//! This module contains all system and user paths used throughout the LXE runtime.
//! Centralizing these prevents hardcoded strings scattered across the codebase.

/// System-wide installation paths (requires root/polkit)
pub mod system {
    use std::path::PathBuf;
    
    /// Base directory for system-wide installations
    pub const BASE_DIR: &str = "/usr";
    
    /// System-wide binary directory
    pub const BIN_DIR: &str = "/usr/bin";
    
    /// System-wide share directory
    pub const SHARE_DIR: &str = "/usr/share";
    
    /// System-wide applications directory (for .desktop files)
    pub const APPLICATIONS_DIR: &str = "/usr/share/applications";
    
    /// Alternative system applications directory
    pub const LOCAL_APPLICATIONS_DIR: &str = "/usr/local/share/applications";
    
    /// Polkit policy files directory
    pub const POLKIT_ACTIONS_DIR: &str = "/usr/share/polkit-1/actions";
    
    /// Get the base directory as PathBuf
    pub fn base_dir() -> PathBuf {
        PathBuf::from(BASE_DIR)
    }
    
    /// Get the applications directory as PathBuf
    pub fn applications_dir() -> PathBuf {
        PathBuf::from(APPLICATIONS_DIR)
    }
    
    /// Get the bin directory as PathBuf
    pub fn bin_dir() -> PathBuf {
        PathBuf::from(BIN_DIR)
    }
}

/// User-local installation paths (no privileges required)
pub mod user {
    use std::path::PathBuf;
    
    /// Get user's local data directory (~/.local/share)
    pub fn data_dir() -> Option<PathBuf> {
        dirs::data_local_dir()
    }
    
    /// Get user's local bin directory (~/.local/bin)
    pub fn bin_dir() -> Option<PathBuf> {
        dirs::data_local_dir().map(|p| p.parent().map(|pp| pp.join("bin"))).flatten()
    }
    
    /// Get user's applications directory (~/.local/share/applications)
    pub fn applications_dir() -> Option<PathBuf> {
        dirs::data_local_dir().map(|p| p.join("applications"))
    }
    
    /// Get the base directory for user installations (~/.local)
    pub fn base_dir() -> Option<PathBuf> {
        dirs::data_local_dir().and_then(|p| p.parent().map(|pp| pp.to_path_buf()))
    }
}

/// Icon theme paths (hicolor)
pub mod icons {
    use std::path::PathBuf;
    
    /// Get the hicolor icon theme base directory
    pub fn hicolor_base(is_system: bool) -> Option<PathBuf> {
        if is_system {
            Some(PathBuf::from("/usr/share/icons/hicolor"))
        } else {
            dirs::data_local_dir().map(|p| p.join("icons/hicolor"))
        }
    }
    
    /// Get icon path for a specific size
    pub fn icon_path(is_system: bool, size: &str, icon_name: &str) -> Option<PathBuf> {
        hicolor_base(is_system).map(|base| {
            base.join(size).join("apps").join(format!("{}.png", icon_name))
        })
    }
    
    /// Standard icon sizes in hicolor theme
    pub const SIZES: &[&str] = &["16x16", "24x24", "32x32", "48x48", "64x64", "128x128", "256x256", "512x512"];
}

/// LXE-specific paths and naming conventions
pub mod lxe {
    use std::path::PathBuf;
    
    /// The polkit policy file path
    pub const POLKIT_POLICY_PATH: &str = "/usr/share/polkit-1/actions/org.lxe.policy";
    
    /// Get the installation directory for an app
    pub fn app_install_dir(base_dir: &PathBuf, app_id: &str) -> PathBuf {
        base_dir.join("share").join(app_id)
    }
    
    /// Get the .desktop file path for an app
    pub fn desktop_file_path(is_system: bool, app_id: &str) -> Option<PathBuf> {
        let apps_dir = if is_system {
            Some(super::system::applications_dir())
        } else {
            super::user::applications_dir()
        };
        
        apps_dir.map(|dir| dir.join(format!("{}.desktop", app_id)))
    }
}

/// Safety validation for paths before deletion
pub mod safety {
    use std::path::Path;
    
    /// Validate that a path is safe to delete
    /// 
    /// Returns true if the path:
    /// - Is not a root directory
    /// - Contains the app_id or app name
    /// - Is within an expected installation directory
    pub fn is_safe_to_delete(path: &Path, app_id: &str) -> bool {
        let path_str = path.to_string_lossy();
        
        // Never delete root or home directly
        if path_str == "/" || path_str == "/home" || path_str == "/usr" {
            return false;
        }
        
        // Path must contain the app_id
        if !path_str.contains(app_id) {
            return false;
        }
        
        // Path must be within an expected location
        let in_local = path_str.contains(".local/share");
        let in_usr_share = path_str.contains("/usr/share/") || path_str.contains("/usr/local/share/");
        let in_opt = path_str.contains("/opt/");
        
        in_local || in_usr_share || in_opt
    }
    
    /// Validate a file path is safe to delete
    pub fn is_file_safe_to_delete(path: &Path, app_id: &str) -> bool {
        let path_str = path.to_string_lossy();
        
        // Must contain app_id
        if !path_str.contains(app_id) {
            return false;
        }
        
        // Must be a file, not a directory
        if path.is_dir() {
            return false;
        }
        
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_safety_blocks_root() {
        assert!(!safety::is_safe_to_delete(std::path::Path::new("/"), "com.test.App"));
        assert!(!safety::is_safe_to_delete(std::path::Path::new("/home"), "com.test.App"));
        assert!(!safety::is_safe_to_delete(std::path::Path::new("/usr"), "com.test.App"));
    }
    
    #[test]
    fn test_safety_requires_app_id() {
        let path = std::path::Path::new("/home/user/.local/share/some-app");
        assert!(!safety::is_safe_to_delete(path, "com.test.App"));
    }
    
    #[test]
    fn test_safety_allows_valid_paths() {
        let path = std::path::Path::new("/home/user/.local/share/com.test.App");
        assert!(safety::is_safe_to_delete(path, "com.test.App"));
        
        let system_path = std::path::Path::new("/usr/share/com.test.App");
        assert!(safety::is_safe_to_delete(system_path, "com.test.App"));
    }
}
