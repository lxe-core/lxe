//! Project Detection Module
//!
//! Auto-detects project metadata from common configuration files.
//! Supports: Rust, Node.js, Tauri, Electron, Python, monorepos

use std::path::Path;
use std::fs;

/// Detected project information
#[derive(Debug, Default, Clone)]
pub struct DetectedProject {
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub executable: Option<String>,
    pub icon: Option<String>,
    pub build_script: Option<String>,
    pub build_input: Option<String>,
}

impl DetectedProject {
    /// Detect project info from the current directory
    pub fn detect(dir: &Path) -> Self {
        let mut detected = Self::default();
        
        // Detection priority:
        // 1. FIRST: Source of truth configs (Cargo.toml, etc.)
        // 2. THEN: Generated/example configs (tauri.conf.json, electron-builder)
        // 3. FINALLY: Fallbacks
        
        // 1. For Tauri projects, ALWAYS prefer src-tauri/Cargo.toml (source of truth)
        if let Some(info) = detect_from_tauri_cargo(dir) {
            detected.merge(info);
        }
        
        // 2. Root Cargo.toml (Rust projects)
        if let Some(info) = detect_from_cargo_toml(dir) {
            detected.merge(info);
        }
        
        // 3. Monorepo detection (finds real app packages)
        if let Some(info) = detect_from_monorepo(dir) {
            detected.merge(info);
        }
        
        // 4. Root package.json (skip if private/root monorepo)
        if let Some(info) = detect_from_package_json(dir) {
            detected.merge(info);
        }
        
        // 5. Tauri conf - only if not placeholder/example
        if let Some(info) = detect_from_tauri_conf(dir) {
            if !is_placeholder_name(info.name.as_deref()) {
                detected.merge(info);
            }
        }
        
        // 6. Electron builder config
        if let Some(info) = detect_from_electron_builder(dir) {
            if !is_placeholder_name(info.name.as_deref()) {
                detected.merge(info);
            }
        }
        
        // 7. Python configs
        if let Some(info) = detect_from_pyproject(dir) {
            detected.merge(info);
        }
        if let Some(info) = detect_from_setup_py(dir) {
            detected.merge(info);
        }
        if let Some(info) = detect_from_setup_cfg(dir) {
            detected.merge(info);
        }
        
        // Fallback to folder name
        if detected.name.is_none() {
            detected.name = dir.file_name()
                .and_then(|s| s.to_str())
                .map(|s| to_title_case(s));
        }
        
        // Detect executable
        if detected.executable.is_none() {
            detected.executable = detect_executable(dir);
        }
        
        // Detect icon
        if detected.icon.is_none() {
            detected.icon = detect_icon(dir);
        }
        
        detected
    }
    
    /// Merge another detection result (doesn't overwrite existing values)
    fn merge(&mut self, other: DetectedProject) {
        if self.name.is_none() { self.name = other.name; }
        if self.version.is_none() { self.version = other.version; }
        if self.description.is_none() { self.description = other.description; }
        if self.executable.is_none() { self.executable = other.executable; }
        if self.icon.is_none() { self.icon = other.icon; }
        if self.build_script.is_none() { self.build_script = other.build_script; }
        if self.build_input.is_none() { self.build_input = other.build_input; }
    }
    
    /// Check if detection found anything meaningful
    pub fn is_useful(&self) -> bool {
        self.name.is_some() && 
        self.name.as_ref().map(|n| n != "Root" && !n.is_empty()).unwrap_or(false)
    }
}

/// Generate app ID from username and project name
pub fn generate_app_id(name: &str) -> String {
    let username = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "developer".to_string());
    
    let kebab_name = to_kebab_case(name);
    format!("com.{}.{}", username.to_lowercase(), kebab_name)
}

/// Check if a name looks like a placeholder/example
fn is_placeholder_name(name: Option<&str>) -> bool {
    let name = match name {
        Some(n) => n.to_lowercase(),
        None => return false,
    };
    
    // Common placeholder patterns
    let placeholders = [
        "my app", "my-app", "myapp", "my_app",
        "example", "sample", "demo", "test",
        "hello world", "hello-world", "helloworld",
        "your app", "your-app", "yourapp",
        "app name", "app-name", "appname",
        "project", "untitled", "newapp",
        "weekly", "daily", // Common in templates
    ];
    
    for p in placeholders {
        if name == p || name.starts_with(&format!("{}.", p)) {
            return true;
        }
    }
    
    // Skip very short names (likely examples)
    if name.len() <= 2 {
        return true;
    }
    
    false
}

/// Detect from src-tauri/Cargo.toml (source of truth for Tauri projects)
fn detect_from_tauri_cargo(dir: &Path) -> Option<DetectedProject> {
    let path = dir.join("src-tauri/Cargo.toml");
    let content = fs::read_to_string(&path).ok()?;
    let toml: toml::Value = content.parse().ok()?;
    
    let package = toml.get("package")?;
    
    // Skip if it looks like a placeholder
    let name = package.get("name").and_then(|v| v.as_str())?;
    if is_placeholder_name(Some(name)) {
        return None;
    }
    
    Some(DetectedProject {
        name: Some(to_title_case(name)),
        version: package.get("version").and_then(|v| v.as_str()).map(String::from),
        description: package.get("description").and_then(|v| v.as_str()).map(String::from),
        executable: Some(name.to_string()),
        icon: detect_tauri_icon(dir),
        build_script: Some(format!(r#"rm -rf dist && mkdir -p dist && \
cp src-tauri/target/release/{} dist/ && \
if [ -f src-tauri/binaries/server-x86_64-unknown-linux-gnu ]; then \
    cp src-tauri/binaries/server-x86_64-unknown-linux-gnu dist/server && \
    chmod +x dist/server; \
fi && \
cp src-tauri/icons/128x128.png dist/icon.png 2>/dev/null || \
cp src-tauri/icons/icon.png dist/icon.png 2>/dev/null || echo "No icon""#, name)),
        build_input: Some("./dist".to_string()),
    })
}

// === Detection Functions ===

fn detect_from_cargo_toml(dir: &Path) -> Option<DetectedProject> {
    let path = dir.join("Cargo.toml");
    let content = fs::read_to_string(&path).ok()?;
    let toml: toml::Value = content.parse().ok()?;
    
    let package = toml.get("package")?;
    
    Some(DetectedProject {
        name: package.get("name").and_then(|v| v.as_str()).map(|s| to_title_case(s)),
        version: package.get("version").and_then(|v| v.as_str()).map(String::from),
        description: package.get("description").and_then(|v| v.as_str()).map(String::from),
        executable: package.get("name").and_then(|v| v.as_str()).map(String::from),
        icon: None,
        build_script: None,
        build_input: None,
    })
}

fn detect_from_package_json(dir: &Path) -> Option<DetectedProject> {
    let path = dir.join("package.json");
    let content = fs::read_to_string(&path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    
    // Skip if root monorepo package
    let name = json.get("name").and_then(|v| v.as_str())?;
    if is_monorepo_root(&json) {
        return None;
    }
    
    Some(DetectedProject {
        name: Some(to_title_case(name)),
        version: json.get("version").and_then(|v| v.as_str()).map(String::from),
        description: json.get("description").and_then(|v| v.as_str()).map(String::from),
        executable: Some(name.to_string()),
        icon: None,
        build_script: None,
        build_input: None,
    })
}

/// Check if this is a monorepo root (should be skipped)
fn is_monorepo_root(json: &serde_json::Value) -> bool {
    // Check for private: true
    if json.get("private").and_then(|v| v.as_bool()).unwrap_or(false) {
        // Check for workspaces field
        if json.get("workspaces").is_some() {
            return true;
        }
        // Check for common root names
        if let Some(name) = json.get("name").and_then(|v| v.as_str()) {
            let lower = name.to_lowercase();
            if lower == "root" || lower == "monorepo" || lower.ends_with("-monorepo") {
                return true;
            }
        }
    }
    false
}

/// Detect from monorepo structures
fn detect_from_monorepo(dir: &Path) -> Option<DetectedProject> {
    // Common monorepo package locations
    let app_patterns = [
        // Lerna/Yarn workspaces patterns
        "packages/app/package.json",
        "packages/app-desktop/package.json",
        "packages/desktop/package.json",
        "packages/client/package.json",
        "packages/main/package.json",
        // Apps directory
        "apps/desktop/package.json",
        "apps/app/package.json",
        "apps/web/package.json",
        "apps/client/package.json",
        // Electron specific
        "packages/renderer/package.json",
        "packages/electron/package.json",
        // Common names
        "app/package.json",
        "desktop/package.json",
    ];
    
    for pattern in app_patterns {
        let path = dir.join(pattern);
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    let name = json.get("name").and_then(|v| v.as_str());
                    let version = json.get("version").and_then(|v| v.as_str());
                    let description = json.get("description").and_then(|v| v.as_str());
                    
                    if name.is_some() {
                        return Some(DetectedProject {
                            name: name.map(|s| to_title_case(s)),
                            version: version.map(String::from),
                            description: description.map(String::from),
                            executable: name.map(String::from),
                            icon: None,
                            build_script: None,
                            build_input: None,
                        });
                    }
                }
            }
        }
    }
    
    // Try to find electron-builder config in monorepo
    if let Some(info) = detect_from_electron_builder(dir) {
        return Some(info);
    }
    
    None
}

fn detect_from_electron_builder(dir: &Path) -> Option<DetectedProject> {
    // Check for electron-builder.yml or electron-builder.json
    let yml_paths = [
        "electron-builder.yml",
        "electron-builder.yaml",
        "build/electron-builder.yml",
    ];
    
    for yml_path in yml_paths {
        let path = dir.join(yml_path);
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                // Simple YAML parsing for key fields
                let mut name = None;
                let mut app_id = None;
                
                for line in content.lines() {
                    let line = line.trim();
                    if line.starts_with("productName:") {
                        name = line.strip_prefix("productName:").map(|s| s.trim().trim_matches('"').to_string());
                    }
                    if line.starts_with("appId:") {
                        app_id = line.strip_prefix("appId:").map(|s| s.trim().trim_matches('"').to_string());
                    }
                }
                
                if name.is_some() {
                    return Some(DetectedProject {
                        name,
                        version: None,
                        description: None,
                        executable: app_id.as_ref().and_then(|id| id.split('.').last()).map(String::from),
                        icon: None,
                        build_script: Some(r#"rm -rf dist && \
cp -r release/linux-unpacked dist && \
cp build/icon.png dist/ 2>/dev/null || echo "No icon""#.to_string()),
                        build_input: Some("./dist".to_string()),
                    });
                }
            }
        }
    }
    
    None
}

fn detect_from_tauri_conf(dir: &Path) -> Option<DetectedProject> {
    // Try Tauri config locations
    let paths = [
        dir.join("src-tauri/tauri.conf.json"),
        dir.join("tauri.conf.json"),
        dir.join("src-tauri/Cargo.toml"),
    ];
    
    for path in paths {
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                    // Tauri 2.x structure
                    let name = json.get("productName")
                        .or_else(|| json.get("package").and_then(|p| p.get("productName")))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    
                    let version = json.get("version")
                        .or_else(|| json.get("package").and_then(|p| p.get("version")))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    
                    let identifier = json
                        .get("identifier")
                        .or_else(|| json.get("bundle").and_then(|b| b.get("identifier")))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    
                    if name.is_some() || version.is_some() {
                        return Some(DetectedProject {
                            name: name.clone(),
                            version,
                            description: None,
                            executable: identifier.as_ref().and_then(|id| id.split('.').last()).map(String::from),
                            icon: detect_tauri_icon(dir),
                            // NOTE: This is detected from tauri.conf.json, so we try to provide a generic build script
                            // but tauri cargo detection is preferred/better
                            build_script: name.as_ref().map(|n| format!(r#"rm -rf dist && mkdir -p dist && \
cp src-tauri/target/release/{} dist/ && \
cp src-tauri/icons/128x128.png dist/icon.png 2>/dev/null || echo "No icon""#, n)),
                            build_input: Some("./dist".to_string()),
                        });
                    }
                }
            }
        }
    }
    None
}

fn detect_tauri_icon(dir: &Path) -> Option<String> {
    let icon_paths = [
        "src-tauri/icons/128x128.png",
        "src-tauri/icons/icon.png",
        "src-tauri/icons/icon.ico",
    ];
    
    for path in icon_paths {
        if dir.join(path).exists() {
            return Some(path.to_string());
        }
    }
    None
}

fn detect_from_pyproject(dir: &Path) -> Option<DetectedProject> {
    let path = dir.join("pyproject.toml");
    let content = fs::read_to_string(&path).ok()?;
    let toml: toml::Value = content.parse().ok()?;
    
    // Try PEP 621 [project] table first, then Poetry
    let project = toml.get("project")
        .or_else(|| toml.get("tool").and_then(|t| t.get("poetry")))?;
    
    Some(DetectedProject {
        name: project.get("name").and_then(|v| v.as_str()).map(|s| to_title_case(s)),
        version: project.get("version").and_then(|v| v.as_str()).map(String::from),
        description: project.get("description").and_then(|v| v.as_str()).map(String::from),
        executable: project.get("name").and_then(|v| v.as_str()).map(String::from),
        icon: None,
        build_script: project.get("name").and_then(|v| v.as_str()).map(|n| format!(r#"rm -rf dist && mkdir -p dist && \
python3 -m venv venv 2>/dev/null || true && \
source venv/bin/activate && \
pip install -q pyinstaller && \
pip install -q -r requirements.txt 2>/dev/null || true && \
pyinstaller --onefile --name {} --clean main.py --distpath dist && \
cp icon.png dist/ 2>/dev/null || echo "No icon""#, n)),
        build_input: Some("./dist".to_string()),
    })
}

/// Parse setup.py for name and version (best effort)
fn detect_from_setup_py(dir: &Path) -> Option<DetectedProject> {
    let path = dir.join("setup.py");
    let content = fs::read_to_string(&path).ok()?;
    
    let mut name = None;
    let mut version = None;
    let mut description = None;
    
    // Look for setup() call arguments
    for line in content.lines() {
        let line = line.trim();
        
        // Match name="value" or name='value'
        if line.starts_with("name=") || line.starts_with("name =") {
            name = extract_string_value(line);
        }
        if line.starts_with("version=") || line.starts_with("version =") {
            version = extract_string_value(line);
        }
        if line.starts_with("description=") || line.starts_with("description =") {
            description = extract_string_value(line);
        }
        
        // Also check for NAME = "..." style constants
        if line.starts_with("NAME =") || line.starts_with("NAME=") {
            if let Some(v) = extract_string_value(line) {
                name = Some(v);
            }
        }
        if line.starts_with("VERSION =") || line.starts_with("VERSION=") {
            if let Some(v) = extract_string_value(line) {
                version = Some(v);
            }
        }
    }
    
    if name.is_some() || version.is_some() {
        Some(DetectedProject {
            name: name.clone().map(|s| to_title_case(&s)),
            version,
            description,
            executable: None,
            icon: None,
            build_script: name.as_ref().map(|n| format!(r#"rm -rf dist && mkdir -p dist && \
python3 -m venv venv 2>/dev/null || true && \
source venv/bin/activate && \
pip install -q pyinstaller && \
pip install -q -r requirements.txt 2>/dev/null || true && \
pyinstaller --onefile --name {} --clean main.py --distpath dist && \
cp icon.png dist/ 2>/dev/null || echo "No icon""#, n)),
            build_input: Some("./dist".to_string()),
        })
    } else {
        None
    }
}

/// Parse setup.cfg for metadata
fn detect_from_setup_cfg(dir: &Path) -> Option<DetectedProject> {
    let path = dir.join("setup.cfg");
    let content = fs::read_to_string(&path).ok()?;
    
    let mut name = None;
    let mut version = None;
    let mut description = None;
    let mut in_metadata = false;
    
    for line in content.lines() {
        let line = line.trim();
        
        if line == "[metadata]" {
            in_metadata = true;
            continue;
        }
        if line.starts_with('[') {
            in_metadata = false;
            continue;
        }
        
        if in_metadata {
            if let Some(value) = line.strip_prefix("name =").or_else(|| line.strip_prefix("name=")) {
                name = Some(value.trim().to_string());
            }
            if let Some(value) = line.strip_prefix("version =").or_else(|| line.strip_prefix("version=")) {
                version = Some(value.trim().to_string());
            }
            if let Some(value) = line.strip_prefix("description =").or_else(|| line.strip_prefix("description=")) {
                description = Some(value.trim().to_string());
            }
        }
    }
    
    if name.is_some() || version.is_some() {
        Some(DetectedProject {
            name: name.clone().map(|s| to_title_case(&s)),
            version,
            description,
            executable: None,
            icon: None,
            build_script: name.as_ref().map(|n| format!(r#"rm -rf dist && mkdir -p dist && \
python3 -m venv venv 2>/dev/null || true && \
source venv/bin/activate && \
pip install -q pyinstaller && \
pip install -q -r requirements.txt 2>/dev/null || true && \
pyinstaller --onefile --name {} --clean main.py --distpath dist && \
cp icon.png dist/ 2>/dev/null || echo "No icon""#, n)),
            build_input: Some("./dist".to_string()),
        })
    } else {
        None
    }
}

/// Extract string value from Python-style assignment
fn extract_string_value(line: &str) -> Option<String> {
    // Find the value part after =
    let parts: Vec<&str> = line.splitn(2, '=').collect();
    if parts.len() != 2 {
        return None;
    }
    
    let value = parts[1].trim();
    
    // Handle quoted strings
    if (value.starts_with('"') && value.contains('"')) || 
       (value.starts_with('\'') && value.contains('\'')) {
        let quote = value.chars().next()?;
        let end = value[1..].find(quote)?;
        return Some(value[1..=end].to_string());
    }
    
    // Handle unquoted (but skip function calls)
    if !value.contains('(') && !value.contains(')') {
        let clean = value.trim_end_matches(',').trim();
        if !clean.is_empty() {
            return Some(clean.to_string());
        }
    }
    
    None
}

fn detect_executable(dir: &Path) -> Option<String> {
    // Directories to scan for executables
    let search_dirs = [
        "dist",
        "build",
        "target/release",
        "out",
        "bin",
        "output",
    ];
    
    for search_dir in search_dirs {
        let path = dir.join(search_dir);
        if path.is_dir() {
            if let Ok(entries) = fs::read_dir(&path) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if entry_path.is_file() && is_executable(&entry_path) {
                        if let Some(name) = entry_path.file_name().and_then(|s| s.to_str()) {
                            // Skip common non-app files
                            if !name.ends_with(".d") && !name.starts_with('.') && !name.ends_with(".pdb") {
                                return Some(name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn detect_icon(dir: &Path) -> Option<String> {
    // Common icon locations and names (priority order)
    let icon_patterns = [
        "icon.png",
        "logo.png",
        "app-icon.png",
        "assets/icon.png",
        "assets/logo.png",
        "src-tauri/icons/128x128.png",
        "resources/icon.png",
        "icons/icon.png",
        "images/icon.png",
        "share/icons/hicolor/128x128/apps/*.png",
        "xdg/icon.png",
    ];
    
    for pattern in icon_patterns {
        // Handle glob patterns
        if pattern.contains('*') {
            if let Some(path) = find_glob_icon(dir, pattern) {
                return Some(path);
            }
        } else {
            let path = dir.join(pattern);
            if path.exists() {
                return Some(pattern.to_string());
            }
        }
    }
    
    // Search for any .png in root
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if ext == "png" {
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                        return Some(name.to_string());
                    }
                }
            }
        }
    }
    
    None
}

fn find_glob_icon(dir: &Path, pattern: &str) -> Option<String> {
    // Simple glob handling for icon patterns
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() != 2 {
        return None;
    }
    
    let prefix = parts[0];
    let parent = dir.join(prefix.trim_end_matches('/'));
    
    if parent.is_dir() {
        if let Ok(entries) = fs::read_dir(&parent) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().map(|e| e == "png").unwrap_or(false) {
                    if let Ok(rel) = path.strip_prefix(dir) {
                        return Some(rel.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    
    None
}

// === Helper Functions ===

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = path.metadata() {
            return metadata.permissions().mode() & 0o111 != 0;
        }
    }
    
    // Fallback: check if it's an ELF file
    if let Ok(mut file) = fs::File::open(path) {
        use std::io::Read;
        let mut magic = [0u8; 4];
        if file.read_exact(&mut magic).is_ok() {
            return magic == [0x7f, b'E', b'L', b'F'];
        }
    }
    false
}

fn to_title_case(s: &str) -> String {
    s.split(|c: char| c == '-' || c == '_' || c.is_whitespace())
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn to_kebab_case(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_whitespace() || c == '_' { '-' } else { c.to_ascii_lowercase() })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_to_title_case() {
        assert_eq!(to_title_case("my-cool-app"), "My Cool App");
        assert_eq!(to_title_case("my_app"), "My App");
        assert_eq!(to_title_case("myapp"), "Myapp");
        assert_eq!(to_title_case("openshot-qt"), "Openshot Qt");
    }
    
    #[test]
    fn test_to_kebab_case() {
        assert_eq!(to_kebab_case("My Cool App"), "my-cool-app");
        assert_eq!(to_kebab_case("My_App"), "my-app");
    }
    
    #[test]
    fn test_generate_app_id() {
        let id = generate_app_id("My App");
        assert!(id.starts_with("com."));
        assert!(id.ends_with(".my-app"));
    }
    
    #[test]
    fn test_extract_string_value() {
        assert_eq!(extract_string_value("name=\"myapp\""), Some("myapp".to_string()));
        assert_eq!(extract_string_value("name='myapp'"), Some("myapp".to_string()));
        assert_eq!(extract_string_value("name = \"myapp\","), Some("myapp".to_string()));
    }
    
    #[test]
    fn test_is_monorepo_root() {
        let root_pkg: serde_json::Value = serde_json::json!({
            "name": "root",
            "private": true,
            "workspaces": ["packages/*"]
        });
        assert!(is_monorepo_root(&root_pkg));
        
        let app_pkg: serde_json::Value = serde_json::json!({
            "name": "my-app",
            "version": "1.0.0"
        });
        assert!(!is_monorepo_root(&app_pkg));
    }
}
