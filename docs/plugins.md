# Plugin setup

Every bundled plugin is ready to use. Open **Plugins** in the editor, pick one, fill in its settings and save. The dot next to it turns green once it's connected.

Plugins only connect to the addresses you enter here.

## OBS Studio

1. In OBS: **Tools > WebSocket Server Settings**. Turn on the server.
2. Click **Show Connect Info** and copy the password.
3. In Keyside, enter the address (`127.0.0.1` if OBS is on this PC), port `4455` and the password.

Tip: turn on **Show a live thumbnail** on a scene key to see the scene on the key.

## Streamlabs Desktop

1. In Streamlabs: **Settings > Remote Control**. Click the QR code, then **Show details**.
2. Copy the API token into Keyside. The port is `59650`.

## Streamer.bot

1. In Streamer.bot: **Servers/Clients > WebSocket Server**. Turn it on (default port `8080`).
2. If you set a password there, enter it in Keyside.

Your global variables show up as `{sb.name}`. To push a value from a Streamer.bot action, add a **WebSocket Server Broadcast** sub-action with:

```json
{ "keyside": { "var": "deaths", "value": 3 } }
```

That sets `{sb.deaths}`.

## Twitch

Twitch needs an app registration of your own. It's free and takes a minute.

1. Go to [dev.twitch.tv/console/apps](https://dev.twitch.tv/console/apps) and register an application.
2. Set **Client Type** to **Public**. Any OAuth redirect URL works (`https://localhost`).
3. Paste the **Client ID** into Keyside and save.
4. Keyside shows a code. Open [twitch.tv/activate](https://www.twitch.tv/activate) and enter it.

Some events (subs, cheers, ads) need Affiliate or Partner status.

## YouTube Live

1. In the [Google Cloud console](https://console.cloud.google.com), create a project and enable **YouTube Data API v3**.
2. Under **Credentials**, create an **OAuth client ID** of type **TVs and Limited Input devices**.
3. Paste the client ID and secret into Keyside and save.
4. Open the link Keyside shows and enter the code.

Chat updates every 15 seconds to stay within YouTube's daily quota.

## Spotify

Playback control needs Spotify Premium.

1. Create an app at [developer.spotify.com/dashboard](https://developer.spotify.com/dashboard) with **Web API** ticked.
2. Add this redirect URI: `http://127.0.0.1:7442/oauth/callback`
3. Paste the **Client ID** into Keyside and save.
4. Press any Spotify key. Your browser opens to sign in.

If you changed the editor port in Keyside settings, use that port in the redirect URI.

## Discord

1. Create an application at [discord.com/developers/applications](https://discord.com/developers/applications).
2. Under **OAuth2**, add the redirect `http://127.0.0.1`.
3. Paste the **Application ID** and **Client Secret** into Keyside and save.
4. Discord asks you to allow Keyside. Click **Authorize**.

The Discord desktop app must be running on this PC.

## VTube Studio

1. In VTube Studio: **Settings**, turn on **Start API** (port `8001`).
2. Save the plugin settings in Keyside. VTube Studio asks to allow Keyside. Click **Allow**.

## VNyan

1. In VNyan: **Settings > Misc**, turn on the WebSocket server (port `8000`).
2. In a node graph, start with a **WebSocket Command** node. Put the same text in the Keyside key.

## Warudo

1. In a blueprint, add an **On WebSocket Action** node and give it an action name.
2. Put the same name in the Keyside key. The optional data text reaches the node's data output.

## veadotube

1. In veadotube, open the program settings and turn on the WebSocket server. Set a fixed port.
2. Enter that port in Keyside.

## Voicemeeter

1. In Voicemeeter: **Menu > VBAN**. Turn on VBAN and the incoming text stream named `Command1`, allowing this PC's address.
2. In Keyside, enter the Voicemeeter PC's address (`127.0.0.1` for this PC). The port is `6980`.

VBAN text can't report state back, so toggle keys follow what Keyside last sent.

## Elgato Key Light

Enter each light's IP address, found in Elgato Control Center under the light's settings. Up to four lights.

## Philips Hue

1. Enter the bridge's IP address (Hue app: **Settings > Bridges**, tap the info icon) and save.
2. Press the round button on the bridge within 90 seconds.

## Home Assistant

1. In Home Assistant, open your profile, then **Security > Long-lived access tokens > Create token**.
2. Enter the address (`homeassistant.local` or an IP), port `8123` and the token. Tick **Use HTTPS** if you reach Home Assistant over HTTPS.
3. To show sensors anywhere, list them under **Entities as variables**, one per line. Use them as `{ha.sensor.living_room_temperature}`.
