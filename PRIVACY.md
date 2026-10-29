# Privacy

Keyside has no account, no cloud service and no telemetry. Nothing is sent to the Keyside project.

## What stays on your PC

The companion keeps everything in one folder:

| System | Folder |
| --- | --- |
| Windows | `%APPDATA%\keyside\Keyside\data` |
| macOS | `~/Library/Application Support/dev.keyside.Keyside` |
| Linux | `~/.local/share/keyside` |

It holds your layouts, settings, uploaded images and sounds, the list of paired devices, API tokens (stored hashed), and plugin settings.

Passwords and sign-in tokens for plugins (OBS password, Twitch session and so on) go into your system keychain. If there is no keychain, they go into a file only your user account can read.

## What stays on your phone

The phone stores the PCs it's paired with: name, addresses, and a device key it can use but can't export. Your layout isn't stored on the phone; it comes from the PC each time.

## Network

Keyside only talks to:

- **Your own devices** on your network (or your Tailscale network if you set that up).
- **The apps and services your plugins connect to**, using the details you enter. For example the Twitch plugin talks to Twitch, the OBS plugin talks to OBS. Each plugin can only reach the hosts listed in its permissions.
- **The plugin and layout registry**, a public file on GitHub, only when you open *Browse plugins* or *Browse layouts*.

Webhook, WebSocket, OSC and UDP keys send exactly what you set up, to where you set it up.

The companion announces itself on your local network (mDNS) so phones can find it. You can turn this off in settings.

## Camera

The phone app uses the camera only to scan the pairing QR code. Images aren't saved or sent anywhere.

## Removing your data

- **A phone:** choose *Forget this PC* on the phone, and *Revoke* in the editor.
- **Everything:** uninstall Keyside and delete the folder above. Plugin secrets are removed from the keychain when you remove the plugin.

## Questions

Open an issue on GitHub. For anything sensitive, see [SECURITY.md](SECURITY.md).
