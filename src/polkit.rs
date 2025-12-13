//! Polkit Integration - Privilege Elevation
//!
//! Uses zbus_polkit to request elevated privileges for system-wide installation.

use anyhow::{Context, Result};
use std::collections::HashMap;
use zbus::Connection;
use zbus_polkit::policykit1::{AuthorityProxy, CheckAuthorizationFlags, Subject};

/// Polkit action ID for LXE system installation
pub const ACTION_INSTALL_SYSTEM: &str = "org.lxe.install.system";
pub const ACTION_UNINSTALL_SYSTEM: &str = "org.lxe.uninstall.system";

/// Check if the current process has authorization for an action
pub async fn check_authorization(action_id: &str) -> Result<bool> {
    let connection = Connection::system().await
        .context("Failed to connect to system D-Bus")?;
    
    let proxy = AuthorityProxy::new(&connection).await
        .context("Failed to create polkit authority proxy")?;
    
    let subject = Subject::new_for_owner(std::process::id(), None, None)?;
    
    let result = proxy
        .check_authorization(
            &subject,
            action_id,
            &HashMap::new(),
            CheckAuthorizationFlags::AllowUserInteraction.into(),
            "",
        )
        .await
        .context("Failed to check authorization")?;
    
    Ok(result.is_authorized)
}

/// Request authorization with user interaction
pub async fn request_authorization(action_id: &str) -> Result<bool> {
    let connection = Connection::system().await
        .context("Failed to connect to system D-Bus")?;
    
    let proxy = AuthorityProxy::new(&connection).await
        .context("Failed to create polkit authority proxy")?;
    
    let subject = Subject::new_for_owner(std::process::id(), None, None)?;
    
    // This will show the polkit authentication dialog
    let result = proxy
        .check_authorization(
            &subject,
            action_id,
            &HashMap::new(),
            CheckAuthorizationFlags::AllowUserInteraction.into(),
            "",
        )
        .await
        .context("Authorization failed")?;
    
    Ok(result.is_authorized)
}

/// Run a command with elevated privileges using pkexec
pub async fn run_elevated<I, S>(command: &str, args: I) -> Result<std::process::Output>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let output = tokio::process::Command::new("pkexec")
        .arg(command)
        .args(args)
        .output()
        .await
        .context("Failed to run pkexec")?;
    
    Ok(output)
}

/// Check if we're running with root privileges
pub fn is_root() -> bool {
    #[cfg(unix)]
    {
        // Check effective UID by reading /proc/self/status
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|content| {
                content.lines()
                    .find(|line| line.starts_with("Uid:"))
                    .and_then(|line| {
                        // Format: Uid: real effective saved fs
                        line.split_whitespace().nth(2) // effective uid
                    })
                    .map(|uid| uid == "0")
            })
            .unwrap_or(false)
    }
    #[cfg(not(unix))]
    {
        false
    }
}

/// Polkit policy file content for LXE
/// 
/// V6 FIX: This policy file MUST be installed to /usr/share/polkit-1/actions/
/// before system-wide installation will work. Without it, polkit will reject
/// the authorization request with "org.lxe.install.system is not registered"
///
/// To install: sudo lxe-runtime --install-policy
pub fn policy_file_content() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC
 "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/PolicyKit/1.0/policyconfig.dtd">
<policyconfig>
  <vendor>LXE</vendor>
  <vendor_url>https://github.com/arsh/lxe</vendor_url>
  
  <action id="org.lxe.install.system">
    <description>Install application system-wide</description>
    <message>Authentication is required to install this application for all users</message>
    <defaults>
      <allow_any>auth_admin</allow_any>
      <allow_inactive>auth_admin</allow_inactive>
      <allow_active>auth_admin_keep</allow_active>
    </defaults>
  </action>
  
  <action id="org.lxe.uninstall.system">
    <description>Uninstall system-wide application</description>
    <message>Authentication is required to uninstall this application</message>
    <defaults>
      <allow_any>auth_admin</allow_any>
      <allow_inactive>auth_admin</allow_inactive>
      <allow_active>auth_admin_keep</allow_active>
    </defaults>
  </action>
</policyconfig>
"#
}

/// Path where the polkit policy file should be installed
pub const POLICY_FILE_PATH: &str = "/usr/share/polkit-1/actions/org.lxe.policy";

/// Install the polkit policy file (requires root)
/// 
/// Returns Ok(true) if installed, Ok(false) if already exists, Err on failure
pub fn install_policy_file() -> Result<bool> {
    use std::fs;
    use std::path::Path;
    
    let policy_path = Path::new(POLICY_FILE_PATH);
    
    // Check if already installed
    if policy_path.exists() {
        tracing::info!("Polkit policy file already exists at {}", POLICY_FILE_PATH);
        return Ok(false);
    }
    
    // Ensure parent directory exists
    if let Some(parent) = policy_path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .context("Failed to create polkit actions directory")?;
        }
    }
    
    // Write the policy file
    fs::write(policy_path, policy_file_content())
        .context("Failed to write polkit policy file. Are you running as root?")?;
    
    tracing::info!("Installed polkit policy file to {}", POLICY_FILE_PATH);
    Ok(true)
}

/// Check if the polkit policy file is installed
pub fn is_policy_installed() -> bool {
    std::path::Path::new(POLICY_FILE_PATH).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_root() {
        // This test just ensures the function runs without panicking
        let _ = is_root();
    }

    #[test]
    fn test_policy_file_content() {
        let content = policy_file_content();
        assert!(content.contains("org.lxe.install.system"));
        assert!(content.contains("org.lxe.uninstall.system"));
    }
    
    #[test]
    fn test_policy_path_constant() {
        assert!(POLICY_FILE_PATH.ends_with(".policy"));
        assert!(POLICY_FILE_PATH.contains("polkit-1"));
    }
}

