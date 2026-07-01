use std::path::{Path, PathBuf};

use super::{CONFIG_DIR_NAME, PROJECTS_DIR_NAME};

pub fn config_path(home_override: Option<&Path>) -> Result<PathBuf, String> {
    Ok(config_dir(home_override)?.join("config.toml"))
}

pub fn config_dir(home_override: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(home) = explicit_home(home_override) {
        return Ok(home.join(".config").join(CONFIG_DIR_NAME));
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = env_path("APPDATA") {
            return Ok(appdata.join(CONFIG_DIR_NAME));
        }
        if let Some(home) = env_path("USERPROFILE") {
            return Ok(home.join("AppData").join("Roaming").join(CONFIG_DIR_NAME));
        }
        if let Some(xdg) = env_path("XDG_CONFIG_HOME") {
            return Ok(xdg.join(CONFIG_DIR_NAME));
        }
        if let Some(home) = env_path("HOME") {
            return Ok(home.join(".config").join(CONFIG_DIR_NAME));
        }
    }

    let home = home_dir(None)?;
    Ok(home.join(".config").join(CONFIG_DIR_NAME))
}

pub fn data_dir(home_override: Option<&Path>) -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        let home = home_dir(home_override)?;
        Ok(home
            .join("Library")
            .join("Application Support")
            .join(CONFIG_DIR_NAME))
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(home) = explicit_home(home_override) {
            return Ok(home.join("AppData").join("Local").join(CONFIG_DIR_NAME));
        }
        if let Some(local_appdata) = env_path("LOCALAPPDATA") {
            return Ok(local_appdata.join(CONFIG_DIR_NAME));
        }
        if let Some(home) = env_path("USERPROFILE") {
            return Ok(home.join("AppData").join("Local").join(CONFIG_DIR_NAME));
        }
        if let Some(appdata) = env_path("APPDATA") {
            return Ok(appdata.join(CONFIG_DIR_NAME));
        }
        if let Some(xdg) = env_path("XDG_DATA_HOME") {
            return Ok(xdg.join(CONFIG_DIR_NAME));
        }
        let home = home_dir(None)?;
        Ok(home.join(".local").join("share").join(CONFIG_DIR_NAME))
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Some(xdg) = env_path("XDG_DATA_HOME") {
            return Ok(xdg.join(CONFIG_DIR_NAME));
        }
        let home = home_dir(home_override)?;
        Ok(home.join(".local").join("share").join(CONFIG_DIR_NAME))
    }
}

pub fn projects_dir(home_override: Option<&Path>) -> Result<PathBuf, String> {
    Ok(config_dir(home_override)?.join(PROJECTS_DIR_NAME))
}

pub(super) fn project_file(home_override: Option<&Path>, id: &str) -> Result<PathBuf, String> {
    Ok(projects_dir(home_override)?.join(format!("{id}.toml")))
}

pub(crate) fn home_dir(home_override: Option<&Path>) -> Result<PathBuf, String> {
    if let Some(home) = home_override {
        return Ok(home.to_path_buf());
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(home) = env_path("USERPROFILE") {
            return Ok(home);
        }
        if let Some(home) = windows_drive_home_dir() {
            return Ok(home);
        }
        if let Some(home) = env_path("HOME") {
            return Ok(home);
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(home) = env_path("HOME") {
            return Ok(home);
        }
    }

    Err("unable to resolve home directory".to_string())
}

fn explicit_home(home_override: Option<&Path>) -> Option<PathBuf> {
    home_override.map(Path::to_path_buf)
}

#[cfg(target_os = "windows")]
fn windows_drive_home_dir() -> Option<PathBuf> {
    let drive = std::env::var_os("HOMEDRIVE")?;
    let path = std::env::var_os("HOMEPATH")?;
    if drive.is_empty() || path.is_empty() || !Path::new(&path).has_root() {
        return None;
    }
    let mut combined = drive;
    combined.push(path);
    let candidate = PathBuf::from(combined);
    candidate.is_absolute().then_some(candidate)
}

fn env_path(name: &str) -> Option<PathBuf> {
    let value = std::env::var_os(name)?;
    (!value.is_empty()).then(|| PathBuf::from(value))
}
