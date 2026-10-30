# Changelog

All notable changes to Keyside. Versions follow [semantic versioning](https://semver.org).

## Unreleased

- Streamer.bot: works with versions that don't send a Hello, with authentication that isn't enforced, and reads globals in the shape Streamer.bot sends. Kick chat and alerts.
- Meld Studio: keys for vertical screenshots and for showing or dismissing the replay of the last clip.
- OBS: scenes are listed in the order OBS shows them.
- Spotify: podcast episodes show up as now playing.
- StreamElements: more alert types in the feed, including Super Chats and YouTube members.
- Twitch: ad schedule times are read in either format Twitch sends.
- Plugins get a shutdown message so `stop()` runs, and plugin processes end when Keyside quits.
- A key that repeats while held stops when the phone disconnects or changes page mid-press. Push-to-talk keys are released too.
- Stream Deck plugin settings can no longer make the stored layout invalid.
- Hotkey and typing steps added from another device wait for review on the PC.
- The editor keeps dynamic dropdowns from retrying nonstop while a plugin is off, shows the label you're typing, and recovers when the layout fails to load.
- Phones send their screen size again after reconnecting, so tablets keep their layout.
- A damaged settings, devices or tokens file is set aside instead of keeping Keyside from starting.
- User variables are saved shortly after they change instead of on every slider move.
- Editor: drag a key to move it, or onto another key to swap them. Works with mouse, touch and pen.
- Editor: right-click a key, or press and hold on a touch screen, for test run, duplicate, copy, cut, move and delete. Empty cells offer paste, new key and new folder.
- Editor: Ctrl+C, Ctrl+X, Ctrl+V and Ctrl+D for keys, arrow keys move the selected key, and deleting shows an Undo button.
- Editor: select several keys with Shift-click, Ctrl+A, or "Select more" on touch screens. Drag, delete, copy, cut, duplicate or nudge them together. Pasted keys keep their arrangement.
- Editor: drop keys on a page tab, or pick "Move to page", to move them to another page.
- Editor: while dragging, a small copy of the key follows your finger, the cells it would land on light up, and it says so before you let go if it won't fit.
- Editor: moving keys in the sideways phone preview now puts them in the right place.
- Editor: the toolbar Undo and Redo buttons now turn on as soon as there's something to undo.
- Devices: click a paired device to rename it, change its access, set, change or remove its PIN, or revoke it. The PIN shows on hover, or on a tap on touch screens.
- Plugins: setup steps in the Plugins panel with clickable links and one-click copy for redirect URLs, tokens and sign-in codes.
- Editor: a key whose app isn't connected says why and has a Set up button. The app names at the top open their setup too.
- Plugins: connection messages, like a sign-in code or what to do next, show in the Plugins panel as they happen.
- Discord: says to open Discord and press Authorize, and explains what to do if it was missed, cancelled or the redirect is missing.
- Equibop: found in `~/.local/bin` and other usual folders even when they aren't on the desktop's PATH, and a crashing Equibop is no longer reported as not installed.
- Linux AppImage: programs, links and sign-in pages Keyside opens no longer inherit the AppImage's library settings, which could stop them from starting.
- Deck: tap the connection bar to fold it into one slim row with a status dot per app. The phone remembers the choice.
- Editor: drag the edges of the button palette, the side panel and Pair a phone to resize them. The small arrow on each edge, or the Pair a phone title, folds them away. Sizes are remembered.
- Stream Decks: a newly plugged-in deck gets its own layout sized to its keys, like 3 × 2 for a Mini, and keeps showing it when the active layout changes. Unlink it in Manage layouts to follow the active layout again.
- Hotkey: click the box and press the shortcut to record it. The pencil still lets you type one.
- Linux: hotkeys and typed text work on Wayland desktops like Hyprland and Sway, without getting in the way of fcitx5 or IBus. Media keys control the playing app and system volume directly.
- Hotkey, Type text and Media key keys show why when they can't send keys, instead of doing nothing.
- Timer: optional "Reset when stopped".
- Spotify: Like uses Spotify's new library endpoint, so it works with apps made after February 2026. Refusals say what Spotify actually said instead of always blaming Premium.
- Stream Deck plugins: JavaScript plugins with their own node_modules start (including the Streamer.bot plugin). Plugins that use sharp for images still show their keys. A plugin that keeps stopping says why.
- Stream Deck plugins: settings pages built with Vue or schema checkers (like Streamer.bot's) now draw, and can reach the same addresses as their plugin, plain ws:// included. A new key's settings open as soon as it's pushed.
- Physical Stream Decks go back to the Elgato logo when Keyside quits, including at logout.
- Devices: changing a phone's name, PIN or access on the PC no longer makes the phone forget it's paired.
- Deck: tablets and landscape phones with a PIN keep their layout after unlocking.
- Editor: folders can be deleted in a layout with only one page. Changes made while pushing are no longer lost.
- Plugins can't change the addresses they're allowed to reach. A plugin no longer restarts when a background request fails.
- HTTP API: revoking a token also closes its open event stream.
- YouTube: viewer and health checks no longer double up when one broadcast ends and the next starts.
- Deck: tap a toast to dismiss it. Toasts also go away on their own.
- Deck: the lock button works on phones that cancel a long press for their own menus, and a quick tap explains that it needs a hold. Held keys with pictures no longer get cut short by the iPhone's "Save image" callout.
- "Last seen" shows days and years instead of thousands of hours.
- Sliders send their value once, when you let go, instead of many times while you drag. The companion also sends one value per slider at a time, newest first.
- A second finger on a key that's already held doesn't press it again.
- Discord can't start two sign-ins at once after reconnecting.
- Keyboard: Enter, Space and arrow keys work on sliders and the lock button, and disabled keys can't be pressed from the keyboard.
- Android: pairing keys are left out of device backups.
- Plugin SDK: `key.set` always replaces the view, and the published package ships compiled JavaScript so `npm test` works in a new plugin.

## [0.1.0] - 2026-10-29

First release.

- Deck for phones, tablets and browsers, with layouts that adapt to every screen.
- Desktop editor with drag and drop, undo, live preview and per-screen arrangements. It sizes itself for big screens and zooms with Ctrl and + or -, or Ctrl and the scroll wheel. Palette groups fold away, and search finds plugins and actions.
- QR pairing with a confirmation code, end-to-end encryption and optional PIN.
- Deck, editor and moderator access levels.
- Built-in actions: hotkeys, text, media keys, links, apps, sounds, timers, variables, webhooks, WebSocket, OSC, UDP and MIDI.
- Now playing key with track and cover from any player (Linux and Windows), and media.* variables.
- Per-app mute and volume on Linux and Windows.
- OBS screenshots, split recordings, audio monitoring and a stream health key.
- Twitch polls, predictions, channel point rewards and shield mode.
- Discord noise suppression, echo cancellation and automatic gain control.
- Home Assistant thermostats, light colour temperature, media players with covers and camera snapshots.
- VTube Studio items and sliders that move model parameters.
- Voicemeeter macro buttons, presets and recorder controls.
- Spotify keys that queue a song or move playback to another device.
- YouTube stream health and a key to change the title.
- Streamer.bot keys that set global variables, and action arguments that can use Keyside variables.
- Plugins for OBS Studio, Meld Studio, Streamlabs Desktop, Streamer.bot, StreamElements, Twitch, YouTube Live, VTube Studio, VNyan, Warudo, veadotube, Discord, Spotify, Voicemeeter, GoXLR, Elgato Key Light, Philips Hue and Home Assistant.
- Discord keys also work with Equibop on Linux.
- Sandboxed plugin runtime with a TypeScript SDK and test harness.
- Stream Deck plugin support, including their settings pages.
- Physical Elgato Stream Deck support.
- Layout sharing with codes, files and a gallery.
- Step recorder, multi-actions, toggles, folders, sliders and chat widgets.
- If and otherwise steps that check a variable, and keys that repeat while held.
- HTTP API with scoped tokens.
- English, German, Spanish and French.
- Apps for Windows, macOS, Linux, Android and iOS.
