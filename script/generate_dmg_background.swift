import AppKit

// Render at 2x while retaining a 640 x 400 point canvas for Finder.
let canvas = NSSize(width: 640, height: 400)
let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 1280, pixelsHigh: 800,
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
bitmap.size = canvas
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
let background = NSImage(contentsOfFile: "assets/brand/macos/installer-background.png")!
background.draw(in: NSRect(origin: .zero, size: canvas))
// Finder does not stretch folder backgrounds when the window grows. Fade the
// exposed right and bottom boundaries into its white picture-view canvas.
let edgeFade = NSGradient(starting: NSColor.white.withAlphaComponent(0), ending: .white)!
edgeFade.draw(from: NSPoint(x: 608, y: 200), to: NSPoint(x: 636, y: 200),
              options: [.drawsAfterEndingLocation])
edgeFade.draw(from: NSPoint(x: 320, y: 48), to: NSPoint(x: 320, y: 8),
              options: [.drawsAfterEndingLocation])

func text(_ value: String, top: CGFloat, size: CGFloat, weight: NSFont.Weight, color: NSColor) {
    let paragraph = NSMutableParagraphStyle()
    paragraph.alignment = .center
    (value as NSString).draw(in: NSRect(x: 32, y: canvas.height - top - 32, width: 576, height: 32),
        withAttributes: [.font: NSFont.systemFont(ofSize: size, weight: weight),
                         .foregroundColor: color, .paragraphStyle: paragraph])
}
let ink = NSColor(srgbRed: 0.22, green: 0.20, blue: 0.17, alpha: 1)
let muted = NSColor(srgbRed: 0.43, green: 0.39, blue: 0.33, alpha: 1)
text("Install Planning", top: 34, size: 28, weight: .semibold, color: ink)
text("Drag Planning into Applications.", top: 78, size: 15, weight: .regular, color: muted)
NSGraphicsContext.restoreGraphicsState()
try bitmap.representation(using: .tiff, properties: [:])!.write(
    to: URL(fileURLWithPath: CommandLine.arguments[1]))
