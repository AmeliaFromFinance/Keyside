# HTTP API

Other tools (Streamer.bot, scripts, home automation) can press keys, set variables and restyle keys through a small HTTP API.

## Tokens

In the editor, open **Settings > API tokens** and create one. Tick only what the tool needs:

| Permission | Allows |
| --- | --- |
| Press keys | `POST /keys/press`, `POST /actions/run`, the event stream |
| Variables | Reading variables and setting `user.*` ones |
| Change keys and pages | Restyling keys, switching pages and layouts |

The token is shown once. Send it as a header:

```
Authorization: Bearer <token>
```

Each token can make about 20 requests a second.

## Where

`http://127.0.0.1:7442/api/v1` on the PC itself.

To reach it from another computer, turn on **Allow the HTTP API from other computers** in settings. It's then at `https://<pc address>:7443/api/v1` (HTTPS only).

## Endpoints

### Press a key

```http
POST /keys/press
{ "page": "scenes", "key": "brb" }
```

Optional `"profile"`; the active layout is used otherwise. Select a key in the editor to see its page and key id under its name. Toggle keys flip.

### Run an action

```http
POST /actions/run
{ "action": { "type": "dev.keyside.obs/scene", "settings": { "scene": "BRB" } } }
```

Actions that start programs can't be run from the API.

### Variables

```http
GET /variables
PUT /variables/user.sub_goal
{ "value": "42 / 50" }
```

Only names starting with `user.` can be set. Use them on keys as `{user.sub_goal}`.

### Restyle a key

```http
PATCH /keys/main/scenes/brb
{ "label": "Back soon", "badge": "LIVE", "tone": "danger" }
```

Fields: `label`, `sub`, `icon`, `color`, `badge`, `badgeTone`, `tone`, `meta`, `value`, `progress`, `since`, `disabled`. Send `null` to reset the key.

### Switch page or layout

```http
POST /pages/show
{ "page": "chat" }

POST /profiles/activate
{ "profile": "gaming" }
```

`/pages/show` takes an optional `"device"` id; without it every device switches.

### Events

```
ws://127.0.0.1:7442/api/v1/events?token=<token>
```

You get a message for every key press and layout switch:

```json
{ "event": "press", "profile": "main", "page": "scenes", "key": "brb", "label": "BRB", "device": "Pixel 8" }
{ "event": "profile", "profile": "gaming" }
```

## Errors

Errors come back as `{ "error": "message" }` with the usual status codes: `401` (missing or wrong token), `403` (token lacks the permission), `404`, `429` (slow down).

## Example

```sh
curl -X POST http://127.0.0.1:7442/api/v1/keys/press \
  -H "Authorization: Bearer $KEYSIDE_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"page":"scenes","key":"brb"}'
```
