# Remote access

Use your deck away from home, for example to control a stream PC from another room or across town, without opening ports on your router.

PhoneDeck works over [Tailscale](https://tailscale.com), a free private network between your own devices.

## Set it up

1. Install Tailscale on the PC and on your phone, and sign in to the same account on both.
2. Restart PhoneDeck. The editor shows *Tailscale is on* once it sees the Tailscale address.
3. Pair the phone (or pair it again if it was paired before you installed Tailscale). The pairing code now includes the Tailscale address.

That's it. At home the phone connects over Wi-Fi. Away, it connects through Tailscale. Everything stays end-to-end encrypted either way.

## Limit where PhoneDeck listens

By default PhoneDeck accepts devices on every network the PC is on. To only allow Tailscale, open **Settings** in the editor and set **Listen on** to your PC's Tailscale address (it starts with `100.`). Restart PhoneDeck to apply it.

## Don't port-forward

Opening PhoneDeck's port on your router isn't needed and isn't recommended. Pairing and encryption protect the connection, but every open port is something the internet can probe.

## Other VPNs

Any VPN that puts your phone and PC on the same private network works. PhoneDeck includes private addresses (`10.x`, `172.16-31.x`, `192.168.x`, `100.64-127.x` and IPv6 ULA) in the pairing code automatically.
