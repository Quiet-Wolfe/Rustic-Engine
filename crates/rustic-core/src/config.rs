use std::env;
use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("Rustic");
        }
    }

    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
        return PathBuf::from(xdg).join("rustic");
    }

    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home).join(".config").join("rustic");
    }

    PathBuf::from(".").join(".rustic")
}
