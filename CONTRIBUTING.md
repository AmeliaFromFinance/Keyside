# Contributing

Thanks for helping out. Bug reports, plugins, translations and fixes are all welcome.

## Before you start

- For a bug, open an issue with steps to reproduce, your OS and PhoneDeck version.
- For a bigger change, open an issue first so we can agree on the approach.
- Security problems go through [SECURITY.md](SECURITY.md), not issues.

## Setup

You need Rust 1.88+, Node.js 22+, pnpm 10 and [Deno](https://deno.com) 2 on your PATH.

```sh
pnpm install
pnpm build
cargo run -- --plugins plugins
cargo run -- open        # in a second terminal, signs you in to the editor
```

For live reload of the web app, run `pnpm dev` and open the printed URL.

## Checks

Run these before opening a pull request. CI runs the same.

```sh
cargo fmt --all
cargo clippy --all-targets -- -D warnings
cargo test
pnpm test:js
pnpm test:e2e
```

## Style

- Keep changes focused. One pull request, one purpose.
- Comments explain why, not what. Skip comments for code that reads clearly on its own.
- User-facing text is short, plain and in sentence case. Wrap it in `t()` in the web app, then add the German, Spanish and French text in `apps/client/src/i18n/`. `pnpm test:js` tells you what's missing.
- Anything that touches pairing, the editor listener or the plugin sandbox needs a test that shows the attack doesn't work.

## Plugins

New plugins can live in their own repository and go into the [registry](registry/index.json). See [docs/plugin-guide.md](docs/plugin-guide.md).

To add a bundled plugin here, put it in `plugins/<name>` with `manifest.json`, `src/main.ts` and tests against a fake of the app it talks to. Look at `plugins/obs` for the pattern.

## Commits

Write commit messages in the imperative ("Add Hue scenes", not "Added"). Keep the first line under 72 characters.

By contributing you agree your work is released under the project's licenses (GPL-3.0 for the app, MIT for `packages/`).
