// Renders Eddy's app icon (macos/Resources/AppIcon.icns) from the brand geometry.
//
//   swift scripts/make-icon.swift
//
// The glyph is drawn on a 24-unit grid, like assets/brand/eddy-glyph.svg; keep the
// two in sync. The menu bar icon draws the same glyph in the app (EddyGlyph.swift).

import AppKit

let background = CGColor(srgbRed: 0xF6 / 255, green: 0xA6 / 255, blue: 0x23 / 255, alpha: 1)
let ink = CGColor(srgbRed: 0x14 / 255, green: 0x14 / 255, blue: 0x14 / 255, alpha: 1)

/// The glyph in 24-unit grid coordinates, y pointing down: a lowercase e whose bar
/// runs out as an arrow.
func glyph(in ctx: CGContext) {
    ctx.move(to: CGPoint(x: 3, y: 12))
    ctx.addLine(to: CGPoint(x: 21.5, y: 12))
    ctx.move(to: CGPoint(x: 18.5, y: 9.5))
    ctx.addLine(to: CGPoint(x: 21.5, y: 12))
    ctx.addLine(to: CGPoint(x: 18.5, y: 14.5))
    // The bowl: from the bar's right end, up and around, open at the bottom right.
    ctx.move(to: CGPoint(x: 16, y: 12))
    ctx.addArc(center: CGPoint(x: 9.5, y: 12), radius: 6.5,
               startAngle: 0, endAngle: 58 * .pi / 180, clockwise: true)
}

/// macOS icon grid: an 824-point tile with rounded corners on a 1024 canvas, glyph
/// at 60% of the tile.
func render(size: Int) -> Data {
    let s = CGFloat(size)
    let ctx = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0,
                        space: CGColorSpace(name: CGColorSpace.sRGB)!,
                        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    let unit = s / 1024
    let tile = CGRect(x: 100 * unit, y: 100 * unit, width: 824 * unit, height: 824 * unit)

    ctx.saveGState()
    ctx.setShadow(offset: CGSize(width: 0, height: -10 * unit), blur: 24 * unit,
                  color: CGColor(gray: 0, alpha: 0.28))
    ctx.addPath(CGPath(roundedRect: tile, cornerWidth: 185 * unit, cornerHeight: 185 * unit, transform: nil))
    ctx.setFillColor(background)
    ctx.fillPath()
    ctx.restoreGState()

    // Flip to the glyph's y-down grid, 24 units → 60% of the tile.
    let scale = tile.width * 0.6 / 24
    ctx.translateBy(x: tile.minX + tile.width * 0.2, y: tile.maxY - tile.height * 0.2)
    ctx.scaleBy(x: scale, y: -scale)
    glyph(in: ctx)
    ctx.setStrokeColor(ink)
    ctx.setLineWidth(2)
    ctx.setLineCap(.round)
    ctx.setLineJoin(.round)
    ctx.strokePath()

    let rep = NSBitmapImageRep(cgImage: ctx.makeImage()!)
    return rep.representation(using: .png, properties: [:])!
}

let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
let iconset = FileManager.default.temporaryDirectory.appendingPathComponent("AppIcon.iconset")
try? FileManager.default.removeItem(at: iconset)
try FileManager.default.createDirectory(at: iconset, withIntermediateDirectories: true)
for points in [16, 32, 128, 256, 512] {
    try render(size: points).write(to: iconset.appendingPathComponent("icon_\(points)x\(points).png"))
    try render(size: points * 2).write(to: iconset.appendingPathComponent("icon_\(points)x\(points)@2x.png"))
}
try render(size: 1024).write(to: root.appendingPathComponent("assets/brand/eddy-icon-1024.png"))

let output = root.appendingPathComponent("macos/Resources/AppIcon.icns")
let iconutil = Process()
iconutil.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
iconutil.arguments = ["-c", "icns", iconset.path, "-o", output.path]
try iconutil.run()
iconutil.waitUntilExit()
print(iconutil.terminationStatus == 0 ? "Wrote \(output.path)" : "iconutil failed")
