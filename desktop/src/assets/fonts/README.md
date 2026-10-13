# Bundled fonts

Latin-subset WOFF2 files, extracted unmodified from the supplied design export
(`reference/Synapse — telas.html`, which embeds them). They are served from the app bundle;
the app never requests fonts from a remote host.

| File | Family | Use | Licence |
| --- | --- | --- | --- |
| `ibm-plex-sans-latin.woff2` | IBM Plex Sans (variable weight) | controls and body text | SIL OFL 1.1 |
| `jetbrains-mono-latin.woff2` | JetBrains Mono (variable weight) | data, agent names, identifiers | SIL OFL 1.1 |
| `newsreader-latin.woff2` | Newsreader (variable weight, roman) | editorial headings | SIL OFL 1.1 |

Licence status is taken from the families' public Google Fonts listings; the licence texts are not
vendored here. Confirm and add them before distributing a release build. The export only embeds the
Latin subset and no Newsreader italic, so other scripts and italics fall back to system fonts.
