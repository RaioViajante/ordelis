# Bundled fonts

The interface uses the system fonts (SF Pro and SF Mono on macOS), as the approved desktop export does. One
Latin-subset WOFF2 file is bundled only as a monospace fallback for systems without SF Mono. It is served
from the app bundle; the app never requests fonts from a remote host.

| File | Family | Use | Licence | SHA-256 |
| --- | --- | --- | --- | --- |
| `jetbrains-mono-latin.woff2` | JetBrains Mono (variable weight) | monospace fallback after `SF Mono` and `ui-monospace` | SIL OFL 1.1 | `2c32b9b3ee358c119e210f6f5195f9bd34894d78a785ff2e95d60e718e400af4` |

The hash matches the Latin resource embedded in the earlier Synapse design export. Google Fonts identifies the
family as OFL-licensed. Its copyright notice and the complete OFL 1.1 text are in `FONT-LICENSES.txt`, which
Tauri includes in the application bundle. IBM Plex Sans and Newsreader, which the legacy Synapse styling used,
were removed in #72 together with their notices.
