# Synapse desktop

Tauri 2 + React + TypeScript shell for Synapse. It is a **local UI only**: it does not start agents,
read your filesystem, open a vault, call a provider or talk to the Python runtime. Pages render
clearly labelled sample data. See [`docs/implementation/desktop-architecture.md`](../docs/implementation/desktop-architecture.md).

This directory is separate from the inherited MCO npm package; the root `package.json` does not
reference it and its `files` list does not publish it.

## Commands

Requires Node 22+, pnpm 12 and a stable Rust toolchain (`rustup`).

```sh
pnpm install --frozen-lockfile
pnpm dev                        # Vite only, http://localhost:1420
pnpm tauri dev                  # desktop window with hot reload
pnpm lint && pnpm typecheck && pnpm test
pnpm build                      # production frontend into dist/
pnpm tauri build --debug --bundles app   # macOS .app under src-tauri/target/debug/bundle
```

Keyboard: `⌘1`–`⌘6` (Ctrl on other platforms) switch pages; the rail is reachable with Tab.
