use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use keyside::app::Options;

const HELP: &str = "\
Keyside companion

Usage:
  keyside [run]                    Start the companion
  keyside open                     Open the editor in your browser
  keyside pair                     Print a pairing link for a new device
  keyside reset-sign-in            Make a new editor sign-in link (signs out old ones)
  keyside plugin keygen <file>     Create a signing key for your plugins
  keyside plugin pack <dir> [--key <file>] [--out <file>]
                                     Pack (and sign) a plugin folder as a .pdp file

Options:
  --data-dir <dir>                   Keep settings and data in this folder
  --plugins <dir>                    Install plugins found in this folder on start
  -h, --help                         Show this help
";

struct Args {
    cmd: Vec<String>,
    data_dir: Option<PathBuf>,
    plugins: Option<PathBuf>,
    key: Option<PathBuf>,
    out: Option<PathBuf>,
}

fn parse() -> Result<Args> {
    let mut a = Args { cmd: Vec::new(), data_dir: None, plugins: None, key: None, out: None };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        let mut path = |name: &str| it.next().map(PathBuf::from).with_context(|| format!("{name} needs a value"));
        match x.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                std::process::exit(0);
            }
            "--data-dir" => a.data_dir = Some(path("--data-dir")?),
            "--plugins" => a.plugins = Some(path("--plugins")?),
            "--key" => a.key = Some(path("--key")?),
            "--out" => a.out = Some(path("--out")?),
            s if s.starts_with('-') => bail!("unknown option {s}\n\n{HELP}"),
            _ => a.cmd.push(x),
        }
    }
    Ok(a)
}

fn default_plugins_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    [dir.join("plugins"), dir.join("../Resources/plugins"), dir.join("resources/plugins")].into_iter().find(|p| p.is_dir())
}

#[tokio::main]
async fn main() -> Result<()> {
    keyside::logs::init(false);
    let args = parse()?;
    let opts = || Options { data_dir: args.data_dir.clone(), bundled_plugins: args.plugins.clone().or_else(default_plugins_dir) };
    let cmd: Vec<&str> = args.cmd.iter().map(String::as_str).collect();
    match cmd.as_slice() {
        [] | ["run"] => {
            let app = keyside::start(opts()).await?;
            println!("Keyside is running. Editor: http://127.0.0.1:{}  (run `keyside open` to sign in)", app.settings.read().admin_port);
            keyside::quit_signal().await;
            keyside::shutdown(&app).await;
            Ok(())
        }
        ["open"] => {
            let paths = keyside::config::Paths::new(args.data_dir.clone())?;
            let id = keyside::config::Identity::load_or_create(&paths)?;
            let port = keyside::config::Settings::load(&paths)?.admin_port;
            keyside::programs::open(&format!("http://127.0.0.1:{port}/#login={}", id.admin_token))?;
            Ok(())
        }
        ["reset-sign-in"] => {
            let paths = keyside::config::Paths::new(args.data_dir.clone())?;
            let mut id = keyside::config::Identity::load_or_create(&paths)?;
            id.rotate_admin_token(&paths)?;
            println!("Done. Restart Keyside, then run `keyside open`.");
            Ok(())
        }
        ["pair"] => {
            let app = keyside::start(opts()).await?;
            let (pid, code) = app.devices.lock().start_pairing(keyside::devices::Scope::Deck, vec![], None)?;
            let p = keyside::server::pairing_payload(&app, &pid, &code);
            println!("Open this on the new device within two minutes, then approve it in the editor:\n\n  {}\n", p["url"].as_str().unwrap_or(""));
            tokio::signal::ctrl_c().await?;
            Ok(())
        }
        ["plugin", "keygen", file] => {
            let key = keyside::plugins::package::new_signing_key();
            let json = serde_json::json!({
                "secret": keyside::crypto::channel::b64(&key.to_bytes()),
                "public": keyside::crypto::channel::b64(&key.verifying_key().to_bytes()),
            });
            keyside::store::write_json_private(&PathBuf::from(file), &json)?;
            println!("Public key (put this in the registry entry): {}", json["public"].as_str().unwrap_or(""));
            Ok(())
        }
        ["plugin", "pack", dir] => {
            let key = match &args.key {
                Some(k) => {
                    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(k)?)?;
                    let raw = keyside::crypto::channel::unb64(v["secret"].as_str().unwrap_or(""), Some(32)).map_err(|_| anyhow::anyhow!("key file is invalid"))?;
                    let arr: [u8; 32] = raw.try_into().map_err(|_| anyhow::anyhow!("key file is invalid"))?;
                    Some(ed25519_dalek::SigningKey::from_bytes(&arr))
                }
                None => None,
            };
            let bytes = keyside::plugins::package::pack(&PathBuf::from(dir), key.as_ref())?;
            let pkg = keyside::plugins::package::Package::from_zip(&bytes)?;
            let out = args.out.clone().unwrap_or_else(|| PathBuf::from(format!("{}-{}.pdp", pkg.manifest.id, pkg.manifest.version)));
            std::fs::write(&out, &bytes)?;
            println!("Wrote {} (sha256 {})", out.display(), pkg.sha256);
            Ok(())
        }
        _ => bail!("unknown command\n\n{HELP}"),
    }
}
