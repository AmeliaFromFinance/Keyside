# Protocol

How devices talk to the companion. You only need this to build your own client. The reference implementation is in [packages/protocol](../packages/protocol) (TypeScript) and [crates/companion/src/crypto](../crates/companion/src/crypto) (Rust); both are tested against the same vectors in `schema/vectors`.

## Transport

Devices connect to the companion's LAN port (default `7443`) at `/ws`:

- Browsers use `wss://` because the page itself comes from the companion over HTTPS (self-signed certificate).
- The native app uses plain `ws://`.

Either way, the WebSocket only carries the handshake and then encrypted frames. TLS is not relied on for security.

## Handshake

All handshake messages are JSON text frames.

**1. hello** (device to companion)

```json
{ "t": "hello", "v": 1, "mode": "pair", "cid": "...", "id": "...", "e": "...", "n": "...", "device": { "name": "Pixel 8", "kind": "phone", "app": "android" } }
```

| Field | Meaning |
| --- | --- |
| `mode` | `pair` for a new device, `resume` for a paired one |
| `cid` | Companion id from the QR code |
| `id` | Pairing id (pair) or device id (resume) |
| `e` | Device's ephemeral P-256 public key, uncompressed SEC1, base64url |
| `n` | 16 random bytes, base64url |
| `device` | Name, kind (`phone`, `tablet`, `desktop`, `browser`) and app (`web`, `android`, `ios`, `desktop`), pair mode only |

**2. challenge** (companion to device): `{ "t": "challenge", "e": "...", "n": "...", "mac": "..." }` with the companion's own ephemeral key and nonce.

**3. confirm** (device to companion): `{ "t": "confirm", "mac": "..." }`

### Keys

```
lp(x)      = u16 big-endian length || x
transcript = lp("keyside/1") || lp(mode) || lp(cid) || lp(id) || lp(e_device) || lp(n_device) || lp(e_companion) || lp(n_companion)
TH         = SHA-256(transcript)
shared     = ECDH(P-256)
okm        = HKDF-SHA256(salt = PSK, ikm = shared, info = lp("keyside/1 keys") || TH, length = 128)
c2s, s2c, macS, macC = okm split into four 32-byte keys
```

The PSK is the 16-byte pairing code from the QR code (pair mode) or the 32-byte device secret (resume mode).

- Companion MAC: `HMAC-SHA256(macS, "server" || TH)`
- Device MAC: `HMAC-SHA256(macC, "client" || TH)`

Both sides check the other MAC in constant time. Without the PSK, an attacker can't produce a valid MAC, so a man in the middle fails here.

In pair mode both screens show a 6-digit code, `u32be(HMAC-SHA256(macS, "sas" || TH)[0..4]) mod 1000000`, and the person approves the device on the PC. The companion then sends `paired` with the new device id and 32-byte secret over the encrypted channel.

## Frames

After the handshake every message is a binary frame:

```
u64 big-endian counter || AES-256-GCM(key, nonce = 4 zero bytes || counter, aad = direction byte, json)
```

- Device to companion uses `c2s` and direction byte `1`, the other way `s2c` and `2`.
- Counters start at 0 and must go up by exactly one. Anything else closes the connection.
- Frames are at most 2 MiB.

## Messages

Inside the frames, JSON objects with a `t` field. The full list with types is in [packages/protocol/src/messages.ts](../packages/protocol/src/messages.ts).

From the device: `viewport`, `input` (key down, up, cancel), `slide`, `nav`, `ping`, `unlock` (PIN), `req` (editor operations).

From the companion: `welcome`, `page`, `keys` (changed keys only, also after a push that leaves the page itself as it was), `status`, `toast`, `pong`, `result`, `event`, `locked`, `unlocked`, `bye`.

## Limits

- Pairing codes are single use and expire after two minutes.
- After 10 failed handshakes from one address, it's blocked for five minutes. Each address may open 32 connections.
- PINs allow five tries, then wait 30 seconds, doubling up to an hour.
