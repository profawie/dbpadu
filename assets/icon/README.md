# dbpadu icon

`dbpadu.svg` is the source drawing, made for this project in the same
family as the termker icon: a database of three stacked discs, with a green
bar for the row limit. The other files are rendered from it:

| File | Used for |
| --- | --- |
| `dbpadu.ico` | Windows program icon (16 to 256 px), embedded by `build.rs` |
| `dbpadu.icns` | macOS app icon, copied into the bundle by `build.sh` |
| `dbpadu-256.png` | Window and taskbar icon, and the logo inside the app |
| `dbpadu-macos-512.png` | macOS Dock icon while running (drawn at 80% with a clear margin, as macOS icons are) |

After changing the drawing, render it again at each size and rebuild the
`.ico` and `.icns` from those renders, so small sizes stay sharp.
