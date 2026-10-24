# eddy brand assets

An eddy is where a stream turns and flows another way, which is what eddy does to a shared link. The mark is a lowercase **e** whose bar runs out as an arrow: the link, sent on to your own app.

| File | Use |
|---|---|
| `eddy-icon.svg` | Flat icon (tile + glyph), for the web and favicons |
| `eddy-glyph.svg` | The glyph alone, 24-unit grid, 2-unit round stroke |
| `eddy-icon-1024.png` | The macOS app icon at 1024 px (rendered by `scripts/make-icon.swift`) |

## Colors

| Name | Hex | Use |
|---|---|---|
| Amber | `#F6A623` | Icon tile, accent |
| Ink | `#141414` | Glyph, text on amber |

## Wordmark

Lowercase **eddy**, bold, tight letter spacing (concepts used Manrope 800). In sentences the name is written "eddy".

## Regenerating the app icon

```sh
swift scripts/make-icon.swift   # → macos/Resources/AppIcon.icns + eddy-icon-1024.png
```

The glyph geometry lives in three places that must match: the SVGs here, `scripts/make-icon.swift` and the menu bar icon in `macos/Sources/Eddy/EddyGlyph.swift`.
