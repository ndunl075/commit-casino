# Third-party notices

Original code, procedural table/wheel/chip graphics, application icon, and
procedural audio are MIT-licensed under [LICENSE](LICENSE). No downloaded fonts,
photos, textures, sounds, advertisements, or remote assets are used. Typography
uses platform fonts; fonts are not redistributed.

The exact resolved JavaScript dependencies and their license identifiers are in
`package-lock.json`. Major redistributed runtime components include:

| Component | License |
| --- | --- |
| React / React DOM | MIT |
| PixiJS | MIT |
| Zustand | MIT |
| Zod | MIT |
| Tauri and plugins | MIT OR Apache-2.0 |
| Rust standard library | MIT OR Apache-2.0 |
| rusqlite | MIT |
| SQLite | Public domain |

Cargo lockfiles record native dependencies. The build collects applicable license
texts and a resolved inventory into the installer and release license archive.
Rust 1.99 standard-library notices are preserved in `third-party/rust`, sourced
from the [official Rust release](https://github.com/rust-lang/rust/tree/1.99.0).

Microsoft Edge WebView2 Runtime, copyright Microsoft Corporation, is distributed
unmodified under Microsoft's separate terms; the project's MIT license does not
apply to it. The NSIS package embeds Microsoft's Evergreen Standalone Installer,
following [Microsoft's distribution documentation](https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution)
and [Tauri's offline installer configuration](https://v2.tauri.app/distribute/windows-installer/).
Microsoft controls Evergreen runtime updates independently of the game's offline
assets. Unsigned Windows installers may show reputation/security prompts.
