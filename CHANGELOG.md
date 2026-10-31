# Changelog

All notable changes to Keyside. Versions follow [semantic versioning](https://semver.org).

## Unreleased

## [1.0.0] - 2026-10-31

First release.

### Deck

- Works on the Android app, or any browser on iPhone, iPad and computers. Layouts fit portrait, landscape and big screens.
- Live keys: the scene on air, who is muted, how long you've been recording.
- Buttons, toggles, multi-state keys, folders, sliders, timers and chat keys.
- Long press, double tap, hold to confirm and repeat while held.
- Switch layouts from the deck header.
- Stream dashboard page with live time, bitrate, dropped frames, CPU and chat.
- Lock the keys with a long press, and fold the connection bar into one slim row.

### Editor

- Drag and drop actions, live preview, undo and redo, and arrangements per screen size.
- Move, swap, copy, paste and multi-select keys with mouse, touch or keyboard.
- Search every key with Ctrl+K, and see every page in the Pages panel.
- Key styles, templates, icon packs, and your own pictures with crop and zoom.
- Multi-actions with if and otherwise steps that check a variable, and a step recorder.
- Setup wizard: pick your apps and get a ready-made layout.
- Review changes before pushing, plus push history and backup and restore.
- Resizable, foldable panels. English, German, Spanish and French.

### Stream tools

- Chat triggers: a command, words in chat or a channel point reward can press a key.
- OBS overlays for timers and text, with your own font, colours, shadow, glow and outline.
- Chat keys drop messages that get deleted, timed out or cleared.
- Built-in hotkeys, typing, media keys, links, apps, sounds, timers, variables, webhooks, WebSocket, OSC, UDP and MIDI.
- Hotkeys and typing work on Wayland as well as X11, Windows and macOS.

### Apps

- OBS Studio, Meld Studio, Streamlabs Desktop, Streamer.bot, Twitch, YouTube Live, StreamElements, VTube Studio, VNyan, Warudo, veadotube, Discord, Spotify, Voicemeeter, GoXLR, Elgato Key Light, Philips Hue and Home Assistant.
- Step-by-step setup in the Plugins panel, with copy buttons for links and codes.
- Plugin health: uptime, crashes, last error and a connection test.

### Stream Decks

- Elgato Stream Decks get a layout sized to their keys, including dials and the touch strip on the Stream Deck +.
- Brightness, sleep, and layouts that follow the app in front.
- Plugins made for Elgato's Stream Deck app install too, settings pages included.
- Decks go back to the Elgato logo when Keyside quits.

### Privacy and security

- No account, no cloud, no telemetry.
- QR pairing with a code to compare on both screens, end-to-end encryption and an optional PIN.
- Deck, editor and moderator access for each device.
- Plugins run in a sandbox and only reach the addresses you allow.
- Shared layouts and steps added from other devices wait for review before they can control apps.

### For developers

- Plugin SDK in TypeScript with a test harness and a starter template.
- HTTP API with scoped tokens to press keys, set variables and change keys.

Apps for Windows, macOS, Linux and Android. iPhone and iPad use the deck in Safari.
