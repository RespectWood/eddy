import AppKit

/// The Eddy mark (a lowercase e whose bar runs out as an arrow), drawn as a menu bar
/// template image. Same geometry as assets/brand/eddy-glyph.svg.
enum EddyGlyph {
    /// `dimmed`: Eddy is disabled. Template images keep their alpha, so macOS still
    /// tints it for light and dark menu bars.
    static func menuBarImage(dimmed: Bool) -> NSImage {
        let size = NSSize(width: 18, height: 18)
        let image = NSImage(size: size, flipped: true) { rect in
            let path = NSBezierPath()
            path.move(to: NSPoint(x: 3, y: 12))
            path.line(to: NSPoint(x: 21.5, y: 12))
            path.move(to: NSPoint(x: 18.5, y: 9.5))
            path.line(to: NSPoint(x: 21.5, y: 12))
            path.line(to: NSPoint(x: 18.5, y: 14.5))
            path.move(to: NSPoint(x: 16, y: 12))
            // Decreasing angles from the bar's end: up over the top and around, open at
            // the bottom right (y points down here, as in the SVG).
            path.appendArc(withCenter: NSPoint(x: 9.5, y: 12), radius: 6.5,
                           startAngle: 0, endAngle: 58, clockwise: true)
            let scale = rect.width / 24
            path.transform(using: AffineTransform(scaleByX: scale, byY: scale))
            path.lineWidth = 2 * scale
            path.lineCapStyle = .round
            path.lineJoinStyle = .round
            NSColor.black.withAlphaComponent(dimmed ? 0.35 : 1).setStroke()
            path.stroke()
            return true
        }
        image.isTemplate = true
        image.accessibilityDescription = "Eddy"
        return image
    }
}
