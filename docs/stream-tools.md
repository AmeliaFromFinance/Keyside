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

Add them to the address with `&` (or `?` when it's the first one), like `.../overlay/timer/break?size=96&color=ffcc00&hide=1`.

| Option | Does | Example |
| --- | --- | --- |
| `size` | Text size in pixels. Timers default to 120, text to 48 | `size=96` |
| `color` | Text colour, as hex without `#` or a colour name | `color=ffcc00`, `color=white` |
| `font` | A font installed on the PC | `font=Inter` |
| `weight` | `normal`, `bold` or 100 to 900 | `weight=900` |
| `align` | `left`, `center` or `right` | `align=right` |
| `shadow` | `0` turns the soft shadow off | `shadow=0` |
| `hide` | `1` hides a timer while it's stopped, or text while it's empty | `hide=1` |

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
