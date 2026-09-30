import Foundation
import CoreGraphics
import ImageIO
import PDFKit
import Vision

// Private document OCR. The only input is a local file path; stdout contains
// extracted text and stderr contains diagnostics without document contents.
let maximumPages = 30
let maximumTextBytes = 64 * 1024
let maximumImagePixels = 40_000_000
let renderLongEdge = 3600

enum OCRError: Error {
    case message(String)
}

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data((message + "\n").utf8))
    exit(1)
}

func recognizedText(_ image: CGImage) throws -> String {
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate
    // Legal IDs and case numbers should not be silently rewritten by a
    // language corrector. The user still reviews the OCR result in the UI.
    request.usesLanguageCorrection = false
    request.recognitionLanguages = ["es-ES", "en-US"]
    let handler = VNImageRequestHandler(cgImage: image, options: [:])
    try handler.perform([request])
    let observations = request.results ?? []
    return observations.compactMap { $0.topCandidates(1).first?.string }.joined(separator: "\n")
}

func rasterizedPage(_ page: PDFPage) throws -> CGImage {
    guard let reference = page.pageRef else { throw OCRError.message("No se pudo leer una página del PDF.") }
    let box = reference.getBoxRect(.mediaBox)
    guard box.width > 0, box.height > 0, box.width < 20_000, box.height < 20_000 else {
        throw OCRError.message("El PDF tiene una página de tamaño no admitido.")
    }
    let scale = min(4.0, CGFloat(renderLongEdge) / max(box.width, box.height))
    let width = max(1, Int(ceil(box.width * scale)))
    let height = max(1, Int(ceil(box.height * scale)))
    guard width * height <= maximumImagePixels else {
        throw OCRError.message("La página del PDF es demasiado grande para OCR.")
    }
    guard let context = CGContext(data: nil, width: width, height: height,
                                  bitsPerComponent: 8, bytesPerRow: 0,
                                  space: CGColorSpaceCreateDeviceRGB(),
                                  bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else {
        throw OCRError.message("No se pudo preparar la página para OCR.")
    }
    context.setFillColor(CGColor(gray: 1, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    context.concatenate(reference.getDrawingTransform(.mediaBox,
                        rect: CGRect(x: 0, y: 0, width: width, height: height),
                        rotate: 0, preserveAspectRatio: true))
    context.drawPDFPage(reference)
    guard let image = context.makeImage() else {
        throw OCRError.message("No se pudo renderizar la página del PDF.")
    }
    return image
}

func imageText(_ url: URL) throws -> String {
    guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else {
        throw OCRError.message("La imagen no es válida o es demasiado grande para OCR.")
    }
    let count = CGImageSourceGetCount(source)
    guard count > 0, count <= maximumPages else {
        throw OCRError.message("La imagen supera 30 páginas o no contiene una página legible.")
    }
    let options: [CFString: Any] = [
        kCGImageSourceCreateThumbnailFromImageAlways: true,
        kCGImageSourceCreateThumbnailWithTransform: true,
        kCGImageSourceThumbnailMaxPixelSize: renderLongEdge,
    ]
    var pages: [String] = []
    for index in 0..<count {
        guard let properties = CGImageSourceCopyPropertiesAtIndex(source, index, nil) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? Int,
              let height = properties[kCGImagePropertyPixelHeight] as? Int,
              width > 0, height > 0,
              width <= maximumImagePixels / height,
              let image = CGImageSourceCreateThumbnailAtIndex(source, index, options as CFDictionary) else {
            throw OCRError.message("Una página de la imagen no es válida o es demasiado grande para OCR.")
        }
        pages.append(try recognizedText(image))
        if pages.joined(separator: "\n\n").utf8.count > maximumTextBytes {
            throw OCRError.message("El texto extraído supera 64 KB; divide el documento.")
        }
    }
    return pages.joined(separator: "\n\n")
}

func pdfText(_ url: URL) throws -> String {
    guard let document = PDFDocument(url: url), document.pageCount > 0 else {
        throw OCRError.message("El PDF no es válido.")
    }
    guard document.pageCount <= maximumPages else {
        throw OCRError.message("El PDF supera 30 páginas. Divide el archivo antes de importarlo.")
    }
    var pages: [String] = []
    for index in 0..<document.pageCount {
        guard let page = document.page(at: index) else {
            throw OCRError.message("No se pudo leer una página del PDF.")
        }
        let embedded = (page.string ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        let ocr = try recognizedText(rasterizedPage(page))
            .trimmingCharacters(in: .whitespacesAndNewlines)
        // Prefer exact embedded text when present. OCR rescues image-only
        // pages and mixed pages whose scanned body dwarfs their text header.
        let selected = ocr.utf8.count > embedded.utf8.count * 3 / 2 + 60 ? ocr : embedded
        pages.append(selected.isEmpty ? ocr : selected)
        if pages.joined(separator: "\n\n").utf8.count > maximumTextBytes {
            throw OCRError.message("El texto extraído supera 64 KB; divide el documento.")
        }
    }
    return pages.joined(separator: "\n\n")
}

guard CommandLine.arguments.count == 2 else { fail("Falta el archivo local para OCR.") }
let url = URL(fileURLWithPath: CommandLine.arguments[1])
do {
    let text = try (url.pathExtension.lowercased() == "pdf" ? pdfText(url) : imageText(url))
        .trimmingCharacters(in: .whitespacesAndNewlines)
    guard !text.isEmpty else { throw OCRError.message("No se encontró texto legible en el documento.") }
    guard text.utf8.count <= maximumTextBytes else {
        throw OCRError.message("El texto extraído supera 64 KB; divide el documento.")
    }
    FileHandle.standardOutput.write(Data(text.utf8))
} catch let error as OCRError {
    switch error { case .message(let message): fail(message) }
} catch {
    fail("No se pudo completar el OCR local. Revisa la calidad del documento.")
}
