//! Builds the Deno command that runs a plugin. Plugins get no file access outside their
//! own folders, no subprocesses, no native code, no environment, and only the network
//! hosts the person approved.

use std::path::{Path, PathBuf};

use tokio::process::Command;

pub struct Sandbox<'a> {
    pub deno: &'a Path,
    /// Unpacked package, read-only.
    pub package: &'a Path,
    pub entry: &'a Path,
    /// Plugin's own writable folder.
    pub data: &'a Path,
    /// Deno's compile cache for this plugin.
    pub cache: &'a Path,
    pub net: &'a [String],
    pub extra_args: &'a [String],
}

/// Proxy and certificate settings passed through so plugins work behind corporate proxies.
const PASS_ENV: &[&str] = &["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy", "NO_PROXY", "no_proxy", "DENO_CERT"];

#[cfg(windows)]
const WINDOWS_ENV: &[&str] = &["SystemRoot", "windir", "TEMP", "TMP"];

pub fn args(s: &Sandbox) -> Vec<String> {
    let mut a: Vec<String> = [
        "run",
        "--quiet",
        "--no-prompt",
        "--no-config",
        "--no-lock",
        "--no-remote",
        "--no-npm",
        "--deny-import",
        "--deny-run",
        "--deny-ffi",
        "--deny-env",
        "--deny-sys",
        "--v8-flags=--max-old-space-size=192",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    a.push(format!("--allow-read={},{}", s.package.display(), s.data.display()));
    a.push(format!("--allow-write={}", s.data.display()));
    if s.net.is_empty() {
        a.push("--deny-net".into());
    } else {
        a.push(format!("--allow-net={}", s.net.join(",")));
    }
    a.push(s.entry.display().to_string());
    a.extend(s.extra_args.iter().cloned());
    a
}

pub fn command(s: &Sandbox) -> Command {
    let mut c = Command::new(s.deno);
    c.args(args(s))
        .current_dir(s.data)
        .env_clear()
        .env("DENO_DIR", s.cache)
        .env("DENO_NO_UPDATE_CHECK", "1")
        .env("DENO_NO_PACKAGE_JSON", "1")
        .env("NO_COLOR", "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    for k in PASS_ENV {
        if let Ok(v) = std::env::var(k) {
            c.env(k, v);
        }
    }
    #[cfg(windows)]
    {
        for k in WINDOWS_ENV {
            if let Ok(v) = std::env::var(k) {
                c.env(k, v);
            }
        }
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        c.creation_flags(CREATE_NO_WINDOW);
    }
    c
}

/// Finds the Deno binary: PHONEDECK_DENO, next to this program, the app resources, then PATH.
pub fn find_deno() -> Option<PathBuf> {
    let name = if cfg!(windows) { "deno.exe" } else { "deno" };
    if let Ok(p) = std::env::var("PHONEDECK_DENO") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        for c in [dir.join(name), dir.join("../Resources").join(name), dir.join("resources").join(name)] {
            if c.is_file() {
                return Some(c);
            }
        }
        let triple = format!("deno-{}", env!("PHONEDECK_TARGET"));
        let sidecar = dir.join(if cfg!(windows) { format!("{triple}.exe") } else { triple });
        if sidecar.is_file() {
            return Some(sidecar);
        }
    }
    std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|d| d.join(name)).find(|p| p.is_file()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locks_everything_down_by_default() {
        let p = PathBuf::from("/pkg");
        let d = PathBuf::from("/data");
        let s = Sandbox { deno: Path::new("deno"), package: &p, entry: &p.join("main.js"), data: &d, cache: &d, net: &[], extra_args: &[] };
        let a = args(&s);
        for flag in ["--deny-net", "--deny-run", "--deny-ffi", "--deny-env", "--deny-import", "--no-remote", "--no-npm", "--no-prompt"] {
            assert!(a.iter().any(|x| x == flag), "missing {flag}");
        }
        assert!(a.iter().any(|x| x == "--allow-write=/data"));
        assert!(!a.iter().any(|x| x.starts_with("--allow-net")));
        assert!(!a.iter().any(|x| x == "-A" || x == "--allow-all"));
    }

    #[test]
    fn grants_only_listed_hosts() {
        let p = PathBuf::from("/pkg");
        let net = vec!["127.0.0.1:4455".to_string(), "api.twitch.tv".to_string()];
        let s = Sandbox { deno: Path::new("deno"), package: &p, entry: &p.join("main.js"), data: &p, cache: &p, net: &net, extra_args: &[] };
        let a = args(&s);
        assert!(a.iter().any(|x| x == "--allow-net=127.0.0.1:4455,api.twitch.tv"));
        assert!(!a.iter().any(|x| x.starts_with("--unstable")), "raw sockets stay off; UDP goes through the host");
    }
}
