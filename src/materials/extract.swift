import Foundation
import PDFKit
import Vision
import ImageIO
import AppKit

struct PageText: Codable {
    let number: Int
    let method: String
    let text: String
}
struct MaterialText: Codable {
    let pages: [PageText]
    let total_pages: Int
    let truncated: Bool
    let warnings: [String]
}
struct ExtractionError: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}
let maxPages = 40
let maxCharacters = 18_000
let maxOCRPages = 20

func recognize(_ image: CGImage) throws -> String {
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate
    request.usesLanguageCorrection = true
    request.recognitionLanguages = ["zh-Hans", "zh-Hant", "en-US"]
    let handler = VNImageRequestHandler(cgImage: image, options: [:])
    try handler.perform([request])
    return (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }.joined(separator: "\n")
}

func extract(_ url: URL) throws -> MaterialText {
    var pages: [PageText] = []
    var warnings = ["OCR is a text transcription, not visual chart interpretation. Verify numbers, table structure and image-only meaning against the original."]
    var remaining = maxCharacters
    var truncated = false
    var totalPages = 1
    func append(_ text: String, number: Int, method: String) {
        let clean = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if clean.isEmpty { truncated = true; warnings.append("Page \(number): no readable text detected."); return }
        let content = String(clean.prefix(remaining))
        if clean.count > remaining { truncated = true }
        remaining -= content.count
        if !content.isEmpty { pages.append(PageText(number: number, method: method, text: content)) }
    }
    if url.pathExtension.lowercased() == "pdf" {
        guard let document = PDFDocument(url: url) else { throw ExtractionError("Cannot decode PDF. The file may be damaged.") }
        guard !document.isLocked else { throw ExtractionError("PDF is password protected. Add an unlocked copy.") }
        totalPages = document.pageCount
        guard totalPages > 0 else { throw ExtractionError("PDF has no pages.") }
        if totalPages > maxPages { truncated = true; warnings.append("Only the first \(maxPages) of \(totalPages) PDF pages were extracted. Split the document to include later pages.") }
        var ocrPages = 0
        for index in 0..<min(totalPages, maxPages) {
            if remaining == 0 { truncated = true; break }
            guard let page = document.page(at: index) else { truncated = true; warnings.append("Page \(index + 1): unavailable."); continue }
            let direct = (page.string ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
            if !direct.isEmpty {
                append(direct, number: index + 1, method: "PDFKit text")
            } else if ocrPages < maxOCRPages {
                ocrPages += 1
                let bounds = page.bounds(for: .mediaBox)
                guard bounds.width > 0, bounds.height > 0 else { truncated = true; warnings.append("Page \(index + 1): invalid dimensions."); continue }
                let scale = min(2.0, 2400.0 / max(bounds.width, bounds.height))
                let thumbnail = page.thumbnail(of: NSSize(width: bounds.width * scale, height: bounds.height * scale), for: .mediaBox)
                var rect = CGRect(origin: .zero, size: thumbnail.size)
                guard let image = thumbnail.cgImage(forProposedRect: &rect, context: nil, hints: nil) else { truncated = true; warnings.append("Page \(index + 1): cannot render for OCR."); continue }
                do { append(try recognize(image), number: index + 1, method: "Vision OCR") }
                catch { truncated = true; warnings.append("Page \(index + 1): OCR failed: \(error.localizedDescription)") }
            } else {
                truncated = true
                warnings.append("Page \(index + 1): skipped after the \(maxOCRPages)-page OCR limit. Split this PDF to read remaining scanned pages.")
            }
        }
    } else {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { throw ExtractionError("Cannot decode image.") }
        let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any]
        let width = properties?[kCGImagePropertyPixelWidth] as? Int ?? 0
        let height = properties?[kCGImagePropertyPixelHeight] as? Int ?? 0
        guard width > 0, height > 0, width <= 20_000, height <= 20_000, width * height <= 80_000_000 else { throw ExtractionError("Image dimensions exceed the 80-megapixel / 20,000-pixel edge limit.") }
        let options: [CFString: Any] = [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 4000, kCGImageSourceCreateThumbnailWithTransform: true]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { throw ExtractionError("Cannot decode image pixels.") }
        append(try recognize(image), number: 1, method: "Vision OCR")
        if width > 4000 || height > 4000 { warnings.append("Image was scaled to a maximum edge of 4,000 pixels for OCR; small text may be missed.") }
    }
    if remaining == 0 { warnings.append("Extraction reached the \(maxCharacters)-character limit. Split the material to read omitted content.") }
    guard !pages.isEmpty else { throw ExtractionError("No readable text found. Add a clearer screenshot, a searchable PDF, or describe the visual content in a comment.") }
    return MaterialText(pages: pages, total_pages: totalPages, truncated: truncated, warnings: warnings)
}
do {
    guard CommandLine.arguments.count == 2 else { throw ExtractionError("Expected one absolute material path.") }
    let result = try extract(URL(fileURLWithPath: CommandLine.arguments[1]))
    let data = try JSONEncoder().encode(result)
    FileHandle.standardOutput.write(data)
} catch {
    FileHandle.standardError.write(Data("Material extraction failed: \(error)\n".utf8))
    exit(1)
}
