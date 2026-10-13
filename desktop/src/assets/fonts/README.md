# Bundled fonts

Latin-subset WOFF2 files, extracted unmodified from the supplied design export
(`reference/Synapse — telas.html`, which embeds them). They are served from the app bundle;
the app never requests fonts from a remote host.

| File | Family | Use | Licence | SHA-256 |
| --- | --- | --- | --- | --- |
| `ibm-plex-sans-latin.woff2` | IBM Plex Sans (variable weight) | controls and body text | SIL OFL 1.1; reserved font name “Plex” | `056e4e2459f57a0033c8c9c844ff19d6e42ac8602027803d4345823bcc939818` |
| `jetbrains-mono-latin.woff2` | JetBrains Mono (variable weight) | data, agent names, identifiers | SIL OFL 1.1 | `2c32b9b3ee358c119e210f6f5195f9bd34894d78a785ff2e95d60e718e400af4` |
| `newsreader-latin.woff2` | Newsreader (variable weight, roman) | editorial headings | SIL OFL 1.1 | `01817351be3edfc1714fe6d60ddea6a22a169a5ebd033b50c7f9495e5d9c386a` |

The hashes match the three Latin resources embedded in every page of the supplied export. Google
Fonts identifies all three families as OFL-licensed. Their copyright notices and the complete OFL
1.1 text are in `FONT-LICENSES.txt`, which Tauri includes in the application bundle. The export only
embeds the Latin subset and no Newsreader italic, so other scripts and italics fall back to system
fonts.
