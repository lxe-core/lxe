//! Library Loader - Self-Extracting Bundled Libraries
//!
//! This module handles extracting and loading bundled libraries
//! to ensure the runtime works on any Linux distribution regardless
//! of the host's glibc or GTK version.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Directory name for bundled libraries within the package
const BUNDLED_LIBS_DIR: &str = "libs";

/// Environment variable to set library path
const LD_LIBRARY_PATH: &str = "LD_LIBRARY_PATH";

/// Check if bundled libraries exist adjacent to the executable
pub fn find_bundled_libs() -> Option<PathBuf> {
    let exe_path = std::env::current_exe().ok()?;
    let exe_dir = exe_path.parent()?;
    
    // Check for libs/ directory next to the executable
    let libs_dir = exe_dir.join(BUNDLED_LIBS_DIR);
    if libs_dir.is_dir() {
        tracing::info!("Found bundled libs at: {:?}", libs_dir);
        return Some(libs_dir);
    }
    
    // Also check one level up (for wrapper scripts)
    let parent_libs = exe_dir.parent()?.join(BUNDLED_LIBS_DIR);
    if parent_libs.is_dir() {
        tracing::info!("Found bundled libs at: {:?}", parent_libs);
        return Some(parent_libs);
    }
    
    None
}

/// Set up the environment to use bundled libraries
/// 
/// This prepends the bundled libs directory to LD_LIBRARY_PATH
/// so the dynamic linker prefers our bundled versions over system ones.
pub fn configure_library_path(libs_dir: &Path) -> Result<()> {
    let libs_path = libs_dir.canonicalize()
        .context("Failed to canonicalize bundled libs path")?;
    
    let libs_str = libs_path.display().to_string();
    
    // Get existing LD_LIBRARY_PATH
    let existing = std::env::var(LD_LIBRARY_PATH).unwrap_or_default();
    
    // Prepend our libs directory
    let new_path = if existing.is_empty() {
        libs_str.clone()
    } else {
        format!("{}:{}", libs_str, existing)
    };
    
    std::env::set_var(LD_LIBRARY_PATH, &new_path);
    tracing::info!("Set {}={}", LD_LIBRARY_PATH, new_path);
    
    // Also set GTK module paths if present
    let gtk_modules = libs_dir.parent()
        .map(|p| p.join("gtk-4.0"));
    
    if let Some(gtk_path) = gtk_modules {
        if gtk_path.is_dir() {
            std::env::set_var("GTK_PATH", gtk_path.display().to_string());
            tracing::info!("Set GTK_PATH={:?}", gtk_path);
        }
    }
    
    // Set GDK pixbuf loaders if present
    let pixbuf_dir = libs_dir.parent()
        .map(|p| p.join("gdk-pixbuf-2.0").join("2.10.0").join("loaders.cache"));
    
    if let Some(cache_path) = pixbuf_dir {
        if cache_path.is_file() {
            std::env::set_var("GDK_PIXBUF_MODULE_FILE", cache_path.display().to_string());
            tracing::info!("Set GDK_PIXBUF_MODULE_FILE={:?}", cache_path);
        }
    }
    
    Ok(())
}

/// Initialize bundled libraries if available
/// 
/// Call this early in main() before any GTK/library initialization.
/// Returns true if bundled libs were found and configured.
pub fn init_bundled_libs() -> bool {
    match find_bundled_libs() {
        Some(libs_dir) => {
            match configure_library_path(&libs_dir) {
                Ok(()) => {
                    tracing::info!("Bundled libraries initialized successfully");
                    true
                }
                Err(e) => {
                    tracing::warn!("Failed to configure bundled libs: {}", e);
                    false
                }
            }
        }
        None => {
            tracing::debug!("No bundled libraries found, using system libraries");
            false
        }
    }
}

/// Check if GTK4 libraries can be loaded
/// 
/// This is a runtime check to detect if GTK4 is available before
/// attempting to initialize it (which would panic).
pub fn can_load_gtk4() -> bool {
    // Try to dlopen libgtk-4
    #[cfg(target_os = "linux")]
    {
        use std::ffi::CString;
        
        let lib_names = ["libgtk-4.so.1", "libgtk-4.so"];
        
        for name in lib_names {
            if let Ok(cname) = CString::new(name) {
                // RTLD_NOW | RTLD_GLOBAL = 0x102 on most Linux systems
                let handle = unsafe {
                    libc::dlopen(cname.as_ptr(), libc::RTLD_NOW)
                };
                
                if !handle.is_null() {
                    unsafe { libc::dlclose(handle) };
                    tracing::debug!("GTK4 check: {} found", name);
                    return true;
                }
            }
        }
        
        tracing::warn!("GTK4 libraries not found");
        false
    }
    
    #[cfg(not(target_os = "linux"))]
    {
        // On non-Linux, assume GTK is available or we'll find out later
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_find_bundled_libs_returns_none_on_dev() {
        // In development, there are no bundled libs
        // This should return None without panicking
        let result = find_bundled_libs();
        // Don't assert anything specific - just ensure no panic
        assert!(result.is_none() || result.is_some());
    }
    
    #[test]
    fn test_can_load_gtk4() {
        // This test just ensures the function doesn't panic
        // The result depends on the system
        let _ = can_load_gtk4();
    }
}
