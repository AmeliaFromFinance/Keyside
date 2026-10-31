# Stream tools

Three things that help while you're live: a ready-made dashboard page, overlays for OBS, and chat triggers.

## Stream dashboard

In the editor, click **+** next to the page tabs and pick **Stream dashboard**. Keyside adds a page with tiles for the apps that are connected right now:

| App | Tiles |
| --- | --- |
| OBS Studio | Time live, time recording, bitrate, dropped frames, CPU |
| Twitch | Viewers, followers, subscribers |
| YouTube | Viewers, likes |
| Streamlabs Desktop | Streaming and recording on or off |
| Any app with chat | Chat messages per minute, and the chat itself |

Connect the apps first (see [Plugin setup](plugins.md)), then add the page. Push to phones and the tiles update live. They are ordinary keys, so you can move, resize, rename or delete them.

OBS figures come from **Stream health** keys. OBS is only asked for them, every 2 seconds, while at least one such key is in a layout. They're also available as `{obs.bitrate}` (kb/s), `{obs.dropped}` (%), `{obs.cpu}` (%) and `{obs.fps}`.

`{core.chat_rate}` counts chat messages from the last minute, across every connected chat. A message that comes in twice, through Twitch and Streamer.bot say, counts once.

## Overlays for OBS

Keyside serves small pages you can add to OBS as a **Browser** source. Their background is transparent and they update by themselves.

They only open on the PC Keyside runs on, at the editor address (port `7442` unless you changed it). They need no sign-in and show nothing but the values you ask for: no layout, no keys, no settings.

### Timer

```
http://127.0.0.1:7442/overlay/timer/<name>
```

`<name>` is the name of a **Timer** step. In the editor, open the Timer step and click **Copy overlay link**.

### Any text

```
http://127.0.0.1:7442/overlay/text?t={twitch.viewers} viewers
```

`t` is text with `{variables}`, the same as on keys. Browsers fill in the spaces and braces for you; if OBS complains, write `%7B` for `{` and `%7D` for `}`.

### Options

The easiest way to style an overlay is in the editor: open the **Timer** step, or pick **Text overlay for OBS** in the editor menu (the gear). Change the font, colours and effects while watching the preview, then click **Copy overlay link**.

You can also add options to the address yourself with `&` (or `?` when it's the first one), like `.../overlay/timer/break?size=96&color=ffcc00&hide=1`. Colours are hex without `#`: `rgb`, `rrggbb`, or `rrggbbaa` to make them see-through. A few names work too, like `white` or `red`. Numbers outside the range are moved to the nearest end. Anything Keyside doesn't understand is left at its default.

| Option | Does | Values | Default |
| --- | --- | --- | --- |
| `font` | Font | `sans`, `serif`, `mono`, `inter`, `jetbrains-mono`, or the name of a font installed on the PC (letters, digits, spaces and `-`, up to 40) | `sans` |
| `size` | Text size in pixels | 8 to 1000 | 120 for timers, 48 for text |
| `weight` | How bold | 100 to 900, `normal`, `bold` | 700 |
| `color` | Text colour | colour | `fff` |
| `bg` | Background colour | colour | none |
| `align` | Alignment | `left`, `center`, `right` | `center` for timers, `left` for text |
| `pad` | Space around the text in pixels | 0 to 200 | a little, growing with the size |
| `spacing` | Space between letters in pixels | -20 to 50 | 0 |
| `upper` | `1` shows all capitals | `1` | off |
| `shadow` | Shadow colour, or `0` for no shadow | colour or `0` | `00000099` |
| `shadow-x`, `shadow-y` | How far the shadow moves right and down, in pixels | -50 to 50 | 0 and 2 |
| `shadow-blur` | How soft the shadow is | 0 to 100 | 6 |
| `glow` | Glow colour | colour | off |
| `glow-size` | How far the glow reaches | 1 to 100 | 10 |
| `outline` | Outline colour | colour | off |
| `outline-width` | Outline thickness in pixels | 1 to 20 | 2 |
| `format` | Timers only: how the time is written | `auto` (`04:59`, `1:02:05`), `hh:mm:ss`, `mm:ss`, `ss` (seconds only) | `auto` |
| `hide` | `1` hides a timer while it's stopped, or text while it's empty | `1` | off |

Inter and JetBrains Mono come with Keyside, so they look the same on every PC.

## Follow the scene

A page can open by itself when the live scene changes. In the editor, open the page, and under **Page settings** pick the app (OBS Studio, Meld Studio or Streamlabs Desktop) and the scene. Phones switch to that page when the scene changes to it, and stay free to move to other pages.

## Chat triggers

A chat trigger presses a key when something happens in chat:

- **Chat command**: a message that starts with the command, like `!brb`. Typing `brb` means `!brb`.
- **Words in chat**: the words anywhere in a message. "hi" doesn't match "this".
- **Channel point reward**: someone redeems a Twitch reward with this name. Any reward works, including ones you made on Twitch.

Open the editor menu (the gear) and pick **Chat triggers**. For each trigger choose:

- **From**: any platform, Twitch, YouTube, or Kick (Kick comes through Streamer.bot).
- **Who can use it**: everyone, subscribers and members, moderators, or only you. Each level includes the ones after it, so moderators count as subscribers too.
- **Wait before it can run again**: a cooldown in seconds, so chat can't run it over and over.
- **Press this key**: the key whose steps run. Toggle keys switch on and off.

Changes take effect after **Push to phones**. Chat triggers can only be set up in the editor on the PC, since they let chat press keys.

Steps can use what came with the message:

| Variable | Holds |
| --- | --- |
| `{core.trigger.user}` | Who typed the command or redeemed the reward |
| `{core.trigger.input}` | The words after the command, the whole message for words in chat, or the text sent with a reward |
| `{core.trigger.source}` | `twitch`, `youtube` or `kick` |

For example, a **Send chat message** step with `Be right back, thanks {core.trigger.user}!`.

Triggers use the chat the plugins already show, so **Show chat and alerts** must be on in the Twitch plugin (and **Mirror chat and alerts** in Streamer.bot). Streamer.bot sends Twitch reward redemptions too.
