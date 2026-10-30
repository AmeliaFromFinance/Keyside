# Stream Deck plugins

Keyside runs plugins made for Elgato's Stream Deck app, so you can use them from your phone. It also drives physical Stream Decks plugged into your PC.

## Install one

1. Download the plugin (a `.streamDeckPlugin` file) from its author or the Elgato Marketplace.
2. In the editor, open the menu and choose **Stream Deck plugins > Add Stream Deck plugin**.
3. Check what Keyside tells you about it, then **Install**.

Its actions appear in the palette like any other. Select a key and open a step to see the plugin's own settings page.

## What runs

| Plugin type | How Keyside runs it |
| --- | --- |
| JavaScript (Node.js), most plugins made with Elgato's current SDK | In the sandbox. No internet unless you allow it. |
| Programs (`.exe` on Windows, apps on macOS) | Only if you agree to run them unsandboxed. They can do anything you can. |
| Web page plugins (older SDK, `CodePath` ends in `.html`) | Not supported. They need Elgato's built-in browser. |

JavaScript plugins can't load native add-ons. For `sharp`, which many plugins use to turn SVG into PNG, Keyside passes the SVG through and draws it itself. Other image conversions in such plugins fail.

If a plugin keeps stopping, the reason shows next to it in **Plugins** and in the message when Keyside turns it off.

## Internet access

JavaScript plugins start with no network access except to Keyside itself. If a plugin needs a web service (weather, a game API), open it in **Stream Deck plugins** and pick:

- **Only these sites**: list the hosts it needs, one per line, like `api.example.com`. Apps on this PC count too: the Streamer.bot plugin needs `127.0.0.1:8080`.
- **Any site**: includes devices on your network. Use this only for plugins you trust.

The plugin's settings page can reach the same sites.

## Settings pages

A plugin's settings page is its own web page. Keyside loads it from a separate local address in a sandboxed frame, so it can't read the editor or act as you. Settings it saves are stored with the key right away.

Settings pages open in the editor on the PC only.

## Supported events

Keys: `keyDown`, `keyUp`, `willAppear`, `willDisappear`, `didReceiveSettings`, `didReceiveGlobalSettings`, `propertyInspectorDidAppear`, `propertyInspectorDidDisappear`, `sendToPlugin`, `deviceDidConnect`.

Commands: `setTitle`, `setImage`, `setState`, `showAlert`, `showOk`, `setSettings`, `getSettings`, `setGlobalSettings`, `getGlobalSettings`, `openUrl`, `logMessage`, `sendToPropertyInspector`.

Two-state actions flip state on release, as on a Stream Deck. Dials, touch strips and profile switching aren't supported yet.

## Physical Stream Decks

Plug one in and it shows your active layout, drawn the same way as on your phone. Folders and back keys work as usual. When Keyside quits, the deck goes back to its Elgato logo. Turn it off under **Settings > Use Stream Deck hardware plugged into this PC**.

On Linux, give your user access to the device first:

```sh
sudo tee /etc/udev/rules.d/70-streamdeck.rules <<'EOF'
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="0fd9", MODE="0660", TAG+="uaccess"
SUBSYSTEM=="usb", ATTRS{idVendor}=="0fd9", MODE="0660", TAG+="uaccess"
EOF
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Close Elgato's app first. Only one program can talk to a Stream Deck at a time.
