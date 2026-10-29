# Registry

`index.json` lists plugins and layouts people can install from the editor (**Browse plugins**, **Browse layouts**).

## Add a plugin

1. Sign your package: `phonedeck plugin pack . --key my-key.json`
2. Upload the `.pdp` file somewhere stable over HTTPS, such as a GitHub release.
3. Open a pull request adding an entry to `plugins`:

```json
{
  "id": "com.example.counter",
  "name": "Counter",
  "version": "1.0.0",
  "author": "You",
  "description": "Counts things.",
  "url": "https://github.com/you/counter/releases/download/v1.0.0/com.example.counter-1.0.0.pdp",
  "sha256": "<printed by phonedeck plugin pack>",
  "signer": "<your public key>",
  "homepage": "https://github.com/you/counter",
  "tags": ["utility"]
}
```

PhoneDeck refuses a download whose hash or signature doesn't match the entry.

## Add a layout

Export the layout from the editor (**Share and gallery > Make a share code**), save the code as a file, upload it, and add an entry to `layouts` with `id`, `name`, `author`, `description`, `url` and `sha256`.

## Review

Entries are reviewed before merging. Plugins must ask only for the permissions they need and must not collect data beyond what their feature requires.
