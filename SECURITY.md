# Security

## Reporting a problem

Please report security issues privately through [GitHub security advisories](https://github.com/AmeliaFromFinance/keyside/security/advisories/new), not in public issues.

Include what you found, how to reproduce it, and what an attacker could do with it. You'll get a reply within a week. Fixes go out as soon as they're ready, and you'll be credited unless you'd rather not be.

Only the latest release gets security fixes.

## How Keyside protects you

**Pairing.** A new device scans a single-use QR code that expires after two minutes. Both screens then show a 6-digit code derived from the key exchange, and you approve the device on the PC. A device can't pair without that approval. An optional PIN locks the deck on the phone, with growing delays after wrong tries. The PC keeps a copy of each PIN in the system keychain so you can look it up in the editor there. Other devices, editors included, can't read it.

**Encryption.** Every message between a device and the PC is end-to-end encrypted (P-256 key exchange, AES-256-GCM, a fresh key per connection). Replayed or reordered messages are rejected. This holds even over plain HTTP. See [docs/protocol.md](docs/protocol.md).

**Access levels.** Each device is either deck only, editor, or moderator (chosen pages only). Revoking a device in the editor disconnects it straight away. Steps that start programs can only be added on the PC itself, and hotkey or typing steps added from another device stay off until you approve them on the PC.

**The editor.** The full editor only listens on `127.0.0.1`. It checks the `Host` header (against DNS rebinding) and the request origin (against other websites in your browser), and sign-in needs a link only the desktop app or `keyside open` can produce.

**Plugins.** Each plugin runs in its own [Deno](https://deno.com) process with:

- no file access outside its own folders,
- no starting programs, no native code, no environment variables,
- network access only to the hosts it declares, which you see before installing.

Plugins that ship with Keyside may run a few exact commands listed in their manifest, such as Equibop's `--toggle-mic`. Nothing else can be passed to them, there's no shell, and plugins from anywhere else can't ask for this.

Every message a plugin sends is checked: it can only change its own keys and its own variables. A plugin that floods messages or keeps crashing is stopped. Plugins from the registry are checked against a pinned hash and signing key.

**Stream Deck plugins.** JavaScript plugins run in the same sandbox with no internet access unless you grant it. Programs (`.exe`, macOS apps) can't be sandboxed, so Keyside only runs them after you explicitly agree. Their settings pages load from a separate origin in a sandboxed frame.

**Imported layouts.** Steps in an imported layout that control apps or the PC stay off until you approve them on the PC.

**HTTP API.** Every request needs a token with the right permission. Tokens are stored hashed and rate limited. The API is only on `127.0.0.1` unless you enable it on the network, where it requires HTTPS.
