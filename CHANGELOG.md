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
- Editor: the toolbar Undo and Redo buttons now turn on as soon as there's something to undo.
- Devices: click a paired device to rename it, change its access, set, change or remove its PIN, or revoke it. The PIN shows on hover, or on a tap on touch screens.
- Deck: tap a toast to dismiss it. Toasts also go away on their own.
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
