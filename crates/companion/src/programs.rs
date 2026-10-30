//! Starting other programs the way the desktop would, even when Keyside runs as an AppImage.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Variables an AppImage sets for its own bundled libraries. Other apps break with them,
/// Electron apps like Equibop most of all.
const APPIMAGE_VARS: &[&str] = &[
    "APPDIR",
    "APPIMAGE",
    "ARGV0",
    "OWD",
    "LD_LIBRARY_PATH",
    "LD_PRELOAD",
    "GDK_BACKEND",
    "GDK_PIXBUF_MODULEDIR",
    "GDK_PIXBUF_MODULE_FILE",
    "GIO_EXTRA_MODULES",
    "GIO_MODULE_DIR",
    "GSETTINGS_SCHEMA_DIR",
    "GST_PLUGIN_PATH",
    "GST_PLUGIN_SCANNER",
    "GST_PLUGIN_SYSTEM_PATH",
    "GST_PLUGIN_SYSTEM_PATH_1_0",
    "GTK_DATA_PREFIX",
    "GTK_EXE_PREFIX",
    "GTK_IM_MODULE_FILE",
    "GTK_PATH",
    "GTK_THEME",
    "PERLLIB",
    "PYTHONHOME",
    "PYTHONPATH",
    "QT_PLUGIN_PATH",
];

/// Environment changes for a child program: AppImage variables removed, and AppImage folders
/// taken out of path lists. `None` means remove the variable.
pub fn outside_env() -> Vec<(OsString, Option<OsString>)> {
    env_changes(|k| std::env::var_os(k))
}

fn env_changes(var: impl Fn(&str) -> Option<OsString>) -> Vec<(OsString, Option<OsString>)> {
    let Some(appdir) = var("APPDIR").filter(|_| var("APPIMAGE").is_some()) else {
        return Vec::new();
    };
    let appdir = PathBuf::from(appdir);
    let mut out: Vec<(OsString, Option<OsString>)> = APPIMAGE_VARS.iter().map(|v| (OsString::from(v), None)).collect();
    for list in ["PATH", "XDG_DATA_DIRS"] {
        if let Some(v) = var(list) {
            let kept: Vec<PathBuf> = std::env::split_paths(&v).filter(|p| !p.starts_with(&appdir)).collect();
            out.push((list.into(), std::env::join_paths(kept).ok()));
        }
    }
    out
}

pub fn apply_std(cmd: &mut std::process::Command) {
    for (k, v) in outside_env() {
        match v {
            Some(v) => cmd.env(k, v),
            None => cmd.env_remove(k),
        };
    }
}

pub fn apply_tokio(cmd: &mut tokio::process::Command) {
    for (k, v) in outside_env() {
        match v {
            Some(v) => cmd.env(k, v),
            None => cmd.env_remove(k),
        };
    }
}

/// Opens a link or file in the default app.
pub fn open(target: &str) -> std::io::Result<()> {
    if outside_env().is_empty() {
        return open::that_detached(target);
    }
    let mut last = std::io::Error::new(std::io::ErrorKind::NotFound, "no app to open it with");
    for mut cmd in open::commands(target) {
        apply_std(&mut cmd);
        cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        match cmd.spawn() {
            Ok(mut child) => {
                std::thread::spawn(move || child.wait());
                return Ok(());
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// Finds a program by name on PATH, then in folders desktop sessions often leave off PATH.
pub fn find(name: &str) -> Option<PathBuf> {
    let path = outside_env().into_iter().find(|(k, _)| k == "PATH").map(|(_, v)| v).unwrap_or_else(|| std::env::var_os("PATH"));
    let mut dirs: Vec<PathBuf> = path.map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        dirs.extend([home.join(".local/bin"), home.join("bin"), home.join(".local/share/flatpak/exports/bin")]);
    }
    dirs.extend(["/usr/local/bin", "/usr/bin", "/bin", "/var/lib/flatpak/exports/bin", "/snap/bin"].map(PathBuf::from));
    dirs.into_iter().map(|d| d.join(name)).find(|p| runnable(p))
}

#[cfg(unix)]
fn runnable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn runnable(p: &Path) -> bool {
    p.is_file()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn leaves_appimage_settings_behind() {
        let env = |k: &str| -> Option<OsString> {
            match k {
                "APPDIR" => Some("/tmp/.mount_ks".into()),
                "APPIMAGE" => Some("/home/me/Keyside.AppImage".into()),
                "PATH" => Some("/tmp/.mount_ks/usr/bin:/usr/bin:/home/me/.local/bin".into()),
                _ => None,
            }
        };
        let out = env_changes(env);
        assert!(out.contains(&("LD_LIBRARY_PATH".into(), None)));
        assert!(out.contains(&("PATH".into(), Some("/usr/bin:/home/me/.local/bin".into()))));
        assert!(env_changes(|_| None).is_empty(), "nothing changes outside an AppImage");
    }

    #[test]
    fn finds_programs_on_path() {
        assert!(find("sh").is_some());
        assert!(find("definitely-not-a-program-keyside").is_none());
    }
}
