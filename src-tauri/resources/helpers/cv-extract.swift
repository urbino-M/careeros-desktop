import AppKit
import PDFKit
import Vision

// Local-only PDF text extraction, with Vision OCR for scanned pages.
guard CommandLine.arguments.count == 2,
      let document = PDFDocument(url: URL(fileURLWithPath: CommandLine.arguments[1])),
      !document.isLocked, document.pageCount > 0, document.pageCount <= 80 else {
    fputs("Unable to read PDF (locked, invalid, or over 80 pages).\n", stderr)
    exit(1)
}
var extractedCharacters = 0
for index in 0..<document.pageCount {
    guard let page = document.page(at: index) else { exit(1) }
    var content = page.string ?? ""
    if content.trimmingCharacters(in: .whitespacesAndNewlines).count < 30 {
        let bounds = page.bounds(for: .mediaBox)
        let scale = min(2.0, 2200 / max(bounds.width, bounds.height))
        guard let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil,
            pixelsWide: max(1, Int(bounds.width * scale)), pixelsHigh: max(1, Int(bounds.height * scale)),
            bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0),
            let context = NSGraphicsContext(bitmapImageRep: bitmap) else { exit(1) }
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = context
        NSColor.white.setFill()
        NSRect(x: 0, y: 0, width: bitmap.pixelsWide, height: bitmap.pixelsHigh).fill()
        context.cgContext.scaleBy(x: scale, y: scale)
        page.draw(with: .mediaBox, to: context.cgContext)
        NSGraphicsContext.restoreGraphicsState()
        guard let image = bitmap.cgImage else { exit(1) }
        let request = VNRecognizeTextRequest()
        request.recognitionLevel = .accurate
        request.usesLanguageCorrection = true
        if #available(macOS 13.0, *) { request.automaticallyDetectsLanguage = true }
        do {
            try VNImageRequestHandler(cgImage: image).perform([request])
            content = (request.results ?? []).compactMap { $0.topCandidates(1).first?.string }.joined(separator: "\n")
        } catch { fputs("PDF OCR failed.\n", stderr); exit(3) }
    }
    extractedCharacters += content.trimmingCharacters(in: .whitespacesAndNewlines).count
    print("\n--- Source CV page \(index + 1) ---\n\(content)")
}
if extractedCharacters < 30 { fputs("PDF contains no readable CV text.\n", stderr); exit(2) }
