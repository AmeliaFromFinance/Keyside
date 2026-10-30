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

The first time you plug one in, Keyside makes a layout for it, named after the model and sized to its keys (a Stream Deck Mini gets 3 × 2). The deck always shows that layout, whatever your phones show. Edit it like any other layout: the editor draws it in the deck's shape.

To have a deck follow the active layout instead, open **Manage layouts** and press **Unlink** on its layout. Deleting the layout does the same, and Keyside won't make a new one for that deck.

### Dials and the touch strip

On a Stream Deck + or + XL, the editor shows a row of dials under the keys. Put a key on a dial like on any cell:

- Turning the dial moves a slider key, the same way dragging it on a phone does (plugin sliders included). Each click moves it by the step of the slider, or 2% without a step.
- Pressing the dial presses its key: it runs the steps, flips a toggle or opens a folder.
- The touch strip shows the label and value of each dial key. Tapping a part of the strip presses the key of the dial below it.

Layouts made for these decks before dials were supported get the dial row the next time the deck is plugged in or the layout is opened in the editor.

### Brightness and sleep

In **Manage layouts**, the card of a deck layout has the brightness of the deck and how long it waits before turning dark. Any press, turn or tap wakes it up again, and that press doesn't run anything. Both settings apply to all layouts of that deck.

### Layouts for games and apps

Press **Add another layout for this deck** to give a deck more layouts, for example one per game. Fill in the apps of a layout, and the deck shows it while one of those apps is in front. The rest of the time it shows its layout without apps (or its first one). This uses the same focus tracking as normal layouts, so **Switch layouts to match the app in front** needs to be on in the settings. Normal layouts don't switch for apps listed on deck layouts.

Folders and back keys work as usual. When Keyside quits, the deck goes back to its Elgato logo. Turn it off under **Settings > Use Stream Deck hardware plugged into this PC**.

On Linux, give your user access to the device first:

```sh
sudo tee /etc/udev/rules.d/70-streamdeck.rules <<'EOF'
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="0fd9", MODE="0660", TAG+="uaccess"
SUBSYSTEM=="usb", ATTRS{idVendor}=="0fd9", MODE="0660", TAG+="uaccess"
EOF
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Close Elgato's app first. Only one program can talk to a Stream Deck at a time.
