import AppKit
import PDFKit
import ImageIO
import UniformTypeIdentifiers
let root = URL(fileURLWithPath: CommandLine.arguments[1], isDirectory: true)
try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
let text = "SUPPLEMENTAL MATERIAL ACCEPTANCE\nThis is a test scenario, not market evidence.\nPDF observation code: MEMORY-PDF-7341\nAssumption: delay review until the next earnings release.\n补充材料：这是验收情景，不是真实市场事实。"
func draw(_ text: String, _ ctx: CGContext) {
    ctx.setFillColor(NSColor.white.cgColor); ctx.fill(CGRect(x: 0, y: 0, width: 1200, height: 720))
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(cgContext: ctx, flipped: false)
    (text as NSString).draw(in: CGRect(x: 40, y: 100, width: 1120, height: 560), withAttributes: [.font: NSFont.systemFont(ofSize: 30), .foregroundColor: NSColor.black])
    NSGraphicsContext.restoreGraphicsState()
}
var box = CGRect(x: 0, y: 0, width: 1200, height: 720)
let pdf = CGContext(root.appendingPathComponent("supplement.pdf") as CFURL, mediaBox: &box, nil)!
pdf.beginPDFPage(nil); draw(text,pdf); pdf.endPDFPage(); pdf.closePDF()
let bitmap = CGContext(data: nil, width: 1200, height: 720, bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
draw("SCREENSHOT MATERIAL ACCEPTANCE\nThis is a test scenario, not market evidence.\nScreenshot observation code: MEMORY-IMAGE-9265\nAssumption: prioritize downside risks before any recommendation.\n截图补充：优先核对风险。", bitmap)
let image = bitmap.makeImage()!
let png = CGImageDestinationCreateWithURL(root.appendingPathComponent("supplement.png") as CFURL, UTType.png.identifier as CFString, 1, nil)!
CGImageDestinationAddImage(png, image, nil);CGImageDestinationFinalize(png)
let scanned = CGContext(root.appendingPathComponent("scanned.pdf") as CFURL, mediaBox: &box, nil)!
scanned.beginPDFPage(nil); scanned.draw(image,in:box); scanned.endPDFPage();scanned.closePDF()
let blank = CGContext(root.appendingPathComponent("blank.pdf") as CFURL, mediaBox: &box, nil)!
blank.beginPDFPage(nil);blank.endPDFPage();blank.closePDF()
print(root.path)
