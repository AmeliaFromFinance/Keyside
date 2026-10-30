# Setup, history and backups

## Setup wizard

On a fresh install (nothing pushed or paired yet) the editor opens a short setup:

1. Pick the apps you use: OBS, Streamer.bot, Streamlabs, Meld, Discord, Spotify, Twitch, YouTube, StreamElements, VTube Studio, veadotube, GoXLR, Voicemeeter, Home Assistant, Hue or Key Light.
2. Keyside adds a new layout with a page of keys for each app and switches to it. Your current layouts stay unless you tick **Remove the example layout** (offered on a fresh install only). Lists like OBS scenes, Streamer.bot actions or Home Assistant devices are read from the app when its plugin can already reach it.
3. Each app's plugin setup comes next, with its steps, settings and a connection test. Once an app shows connected, **Fill the page** reads its scenes, actions or devices into its page.
4. Pair a phone.

Open it again any time from the editor menu: **Setup wizard**.

## Push history

The PC keeps the last 20 pushed layouts, with the time and the device that pushed them. Open **History** in the editor menu, pick a version to see what restoring it would change, then **Restore**. That loads it into your draft; press **Push to phones** to make it live. The layout from before the very first push is kept too.

## Backup and restore

**Settings > Backup and restore** makes one file with:

- layouts, settings and user variables
- installed plugins and their settings
- paired devices and API tokens
- images and sounds
- Stream Deck plugins and their settings, and the Stream Deck hardware Keyside has seen
- the push history

Passwords and keys (plugin passwords and sign-ins, the keys paired devices use, device PINs) are left out unless you tick **Include passwords and keys** and pick a passphrase of at least 8 characters. They are then encrypted with it (Argon2id and AES-256-GCM). Without the passphrase they can't be read, so keep it somewhere safe.

To restore, pick a backup file under **Restore from file**. Keyside shows what's inside and asks before it replaces anything. It then reloads the data and restarts the plugins. A few things stay as they are on this PC: the editor port, the editor zoom and the sign-in link.

A backup without passwords and keys keeps the ones already on this PC. Paired devices this PC doesn't know have to pair again. When a restore changes network settings, or brings the identity phones paired with from another PC, Keyside asks for a restart.

Only the editor on the PC itself can make or restore backups.
