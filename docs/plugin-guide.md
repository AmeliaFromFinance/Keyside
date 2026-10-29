# Writing plugins

A plugin is a TypeScript file that runs in Keyside's sandbox. It adds actions to the palette, reacts to key presses and can change how its keys look.

## Start a plugin

```sh
npm create keyside-plugin my-plugin
cd my-plugin
npm install
npm run build     # bundles src/main.ts into main.js
npm test
```

You get:

```
manifest.json     what the plugin is and what it may do
src/main.ts       the code
test/main.test.ts tests that run without Keyside
```

## The manifest

```json
{
  "id": "com.example.counter",
  "name": "Counter",
  "version": "1.0.0",
  "author": "You",
  "prefix": "counter",
  "host": "^1.0",
  "entry": "main.js",
  "permissions": { "net": ["{host}:{port}"] },
  "connection": {
    "type": "object",
    "properties": {
      "host": { "type": "string", "title": "Address", "default": "127.0.0.1" },
      "port": { "type": "integer", "title": "Port", "default": 8080 },
      "password": { "type": "string", "format": "secret", "title": "Password" }
    }
  },
  "actions": [
    {
      "id": "add",
      "name": "Add one",
      "icon": "plus",
      "settings": { "type": "object", "properties": { "step": { "type": "integer", "title": "Step", "default": 1 } } }
    }
  ],
  "variables": [{ "name": "count", "title": "Count", "type": "number" }]
}
```

- **id**: reverse domain name you control.
- **prefix**: short name for your variables, so `count` becomes `{counter.count}`.
- **connection**: settings for the whole plugin, shown in the Plugins panel. `"format": "secret"` keeps a value in the system keychain. `"x-hidden": true` hides a field you manage yourself, like a sign-in session.
- **actions**: each has its own settings form. Use `"x-options": "dynamic"` on a text field to fill a dropdown from your plugin.
- **icon**: any name from the editor's icon picker.

### Permissions

Plugins get nothing by default. Ask for what you need:

| Permission | Allows |
| --- | --- |
| `"net": ["api.example.com"]` | Connections to that host |
| `"net": ["{host}:{port}"]` | Connections to the address in your connection settings |
| `"udp": true` | Sending UDP to hosts in `net`, through `host.udp()` |
| `"ipc": ["discord"]` | Talking to the Discord app through `host.ipc` |

Wildcards aren't allowed. People see this list before installing, and again if an update asks for more.

## The code

```ts
import { definePlugin, setting, type Host } from "@keyside/sdk";

let host: Host;
let count = 0;

export default definePlugin({
  start(h) {
    host = h;
    host.status("connected");
    host.setVar("count", count);
  },
  actions: {
    add: {
      press({ key, settings }) {
        count += setting(settings, "step", 1);
        key.set({ badge: String(count), tone: count > 9 ? "accent" : undefined });
        host.setVar("count", count);
      },
    },
  },
});
```

### Actions

| Handler | Called when |
| --- | --- |
| `press(step)` | The key is pressed |
| `release(step)` | The key is let go (push to talk) |
| `slide({ value })` | A slider moves, `value` is 0 to 1 |
| `options(field, settings)` | The editor needs a dynamic dropdown |
| `appear(key)` / `disappear(key)` | A key using this action is added or removed |

Throw an error to show it as a toast on the phone.

Add `batch(steps, phase)` to the definition to handle several steps of one key in one go, like OBS does for "mute all".

### Keys

`host.keys(action?)` lists your placed keys. `key.set(view)` changes how a key looks, `key.clear()` resets it. `key.label` and `key.icon` are what the person chose in the editor.

| Field | Shows |
| --- | --- |
| `label`, `sub` | Text lines |
| `icon` | Icon name |
| `badge`, `badgeTone` | Small tag, like ON AIR |
| `tone` | `accent` or `danger` outline, `success`, `warning`, `info`, `muted` |
| `meta` | Small text top right |
| `since` | A running clock from this time (ms) |
| `value`, `progress` | 0 to 1, for sliders and bars |
| `image` | PNG, JPEG, WebP, GIF or SVG data URL, up to 350 KB |
| `disabled` | Greyed out and not pressable |

Updates are batched, so calling `set` often is fine.

### Host

| Call | Does |
| --- | --- |
| `host.connection` | Current connection settings, secrets included |
| `host.status(state, detail?, pills?)` | Connection state in the deck header, plus up to three pills like REC |
| `host.setVar(name, value)` | Publishes `{prefix.name}` |
| `host.feed(name, item)` | Adds a line to a feed declared in the manifest (chat, alerts) |
| `host.toast(text, tone?)` | Short message on every device |
| `host.saveConnection(values)` | Saves values like tokens into the connection settings |
| `host.openBrowser(url)` | Opens an https page on the PC |
| `host.authorize(url)` | Browser sign-in with a redirect back to Keyside. Returns the code and redirect URI |
| `host.udp(host, port, bytes)` | Sends a UDP datagram |
| `host.log.info(...)` | Writes to the Keyside log |
| `host.dataDir` | A folder only this plugin can read and write |

`console.log` goes to the log too, since the plugin's output is reserved for talking to Keyside.

## Testing

`@keyside/sdk/testing` runs your plugin in memory:

```ts
import { createTestHost } from "@keyside/sdk/testing";
import plugin from "../src/main.ts";

const h = createTestHost(plugin);
await h.start();
await h.sync([{ id: "k1", action: "add", settings: { step: 2 } }]);
await h.press("k1", "add", { step: 2 });
await h.settle();
assert.equal(h.view("k1")?.badge, "2");
```

Import the testing module before your plugin. For network plugins, start a fake server in the test, like the bundled plugins do.

## Share it

```sh
keyside plugin keygen my-key.json      # once; keep this file private
keyside plugin pack . --key my-key.json
```

That writes a signed `.pdp` file. Anyone can install it with **Plugins > Install from file**. Signed updates must use the same key.

To list it in the registry, open a pull request that adds an entry to [registry/index.json](../registry/index.json) with the download URL, SHA-256 and your public key.

## Limits

- 192 MB memory, 200 messages a second.
- A plugin that crashes five times in five minutes is turned off.
- No file access outside its own data folder, no subprocesses, no native code.
