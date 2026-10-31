import Foundation
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

// Static Dock counterpart of the refined Assembly mark, on a macOS icon tile.
let destination = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
try FileManager.default.createDirectory(at: destination, withIntermediateDirectories: true)
for size in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let pixels = size * scale
        let context = CGContext(data: nil, width: pixels, height: pixels, bitsPerComponent: 8,
            bytesPerRow: pixels * 4, space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
        context.scaleBy(x: CGFloat(pixels) / 1024, y: CGFloat(pixels) / 1024)
        let tile = CGPath(roundedRect: CGRect(x: 96, y: 96, width: 832, height: 832),
            cornerWidth: 184, cornerHeight: 184, transform: nil)
        context.addPath(tile)
        context.setFillColor(CGColor(red: 0.10, green: 0.115, blue: 0.12, alpha: 1))
        context.fillPath()
        context.addPath(tile)
        context.setStrokeColor(CGColor(gray: 1, alpha: 0.13))
        context.setLineWidth(2)
        context.strokePath()
        context.translateBy(x: 160, y: 864)
        context.scaleBy(x: 5.5, y: -5.5)
        context.setFillColor(CGColor(red: 199/255, green: 163/255, blue: 107/255, alpha: 1))
        for bar in [CGRect(x: 44, y: 18, width: 66, height: 20),
                    CGRect(x: 90, y: 44, width: 20, height: 66),
                    CGRect(x: 18, y: 90, width: 66, height: 20),
                    CGRect(x: 18, y: 18, width: 20, height: 66)] { context.fill(bar) }
        let suffix = scale == 2 ? "@2x" : ""
        let url = destination.appendingPathComponent("icon_\(size)x\(size)\(suffix).png")
        let output = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)!
        CGImageDestinationAddImage(output, context.makeImage()!, nil)
        precondition(CGImageDestinationFinalize(output))
    }
}
