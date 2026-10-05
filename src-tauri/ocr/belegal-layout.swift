import Foundation
import AppKit
import CoreGraphics
import CoreText
import ImageIO
import PDFKit
import Vision
import NaturalLanguage

struct NamedEntity: Codable { var kind: String; var text: String }
func aliasSize(_ redaction: Redaction) -> Double {
    let name = NSFont(name: redaction.fontName, size: redaction.fontSize)?.fontName ?? "Helvetica"
    let size = min(redaction.fontSize, redaction.rect[3] * 0.85)
    let font = CTFontCreateWithName(name as CFString, size, nil)
    let line = CTLineCreateWithAttributedString(NSAttributedString(string: redaction.alias, attributes: [NSAttributedString.Key(kCTFontAttributeName as String): font]))
    let measured = CTLineGetTypographicBounds(line, nil, nil, nil)
    return measured > redaction.rect[2] - 1 ? size * (redaction.rect[2] - 1) / measured : size
}
func namedEntities(_ source: String) throws -> [NamedEntity] {
    let text = source.components(separatedBy: "\n").map {
        $0.split(whereSeparator: { $0.isWhitespace }).joined(separator: " ")
    }.joined(separator: "\n")
    let recognizer = NLLanguageRecognizer(); recognizer.processString(text)
    let language = recognizer.dominantLanguage ?? .spanish
    try require(NLTagger.availableTagSchemes(for: .word, language: language).contains(.nameType), "El detector local no admite el idioma de este documento.")
    let tagger = NLTagger(tagSchemes: [.nameType]); tagger.string = text
    tagger.setLanguage(language, range: text.startIndex..<text.endIndex)
    var result: [NamedEntity] = []
    tagger.enumerateTags(in: text.startIndex..<text.endIndex, unit: .word, scheme: .nameType, options: [.omitWhitespace, .omitPunctuation, .joinNames]) { tag, range in
        // In Spanish production tables, the organization tag labels ordinary
        // headings such as activities as companies. Keep this fast complement
        // focused on people; organization extraction remains with Qwen/rules.
        if tag == .personalName {
            result.append(NamedEntity(kind: "PERSON", text: String(text[range])))
        }
        return true
    }
    return result
}

// Local layout extraction and destructive pixel redaction. The output PDF is
// built only from modified pixels: no source objects, metadata or text layer.
struct Glyph: Codable {
    var start: Int; var end: Int; var line: Int; var rect: [Double]
    var fontSize: Double; var fontName: String; var color: [Double]
    var confidence: Double
}
struct Page: Codable {
    var width: Double; var height: Double; var imageBase64: String
    var glyphs: [Glyph]; var source: String
}
struct Document: Codable { var text: String; var pages: [Page] }
struct Redaction: Codable {
    var entityIndex: Int?
    var page: Int; var rect: [Double]; var alias: String
    var fontSize: Double; var fontName: String; var color: [Double]
}
struct RichRun: Codable { var text: String; var font: String; var size: Double; var color: [Double]; var bold: Bool; var italic: Bool; var underline: Bool }
struct RichParagraph: Codable { var runs: [RichRun]; var left: Double; var right: Double; var first: Double; var align: String }
struct RichOverlay: Codable { var page: Int; var eraseRect: [Double]; var eraseRegions: [[Double]]; var rect: [Double]; var paragraphs: [RichParagraph] }
struct ExportPlan: Codable { var pages: [Page]; var redactions: [Redaction]; var overlays: [RichOverlay]? }
// A PDF may contain illustrations or table borders without text coordinates.
// Do not let an expanded draft overwrite those pixels merely because OCR
// reports no neighbouring words there.
func checkExpansion(_ image: CGImage, rect: [Double], erased: [Double], page: Page) throws {
    let left = rect[0], top = rect[1], right = left + rect[2], bottom = top + rect[3]
    let el = erased[0], et = erased[1], er = el + erased[2], eb = et + erased[3]
    let overlapTop = max(top, et), overlapBottom = min(bottom, eb)
    let strips = [[left,top,rect[2],min(bottom,et)-top],
                  [left,max(top,eb),rect[2],bottom-max(top,eb)],
                  [left,overlapTop,min(right,el)-left,overlapBottom-overlapTop],
                  [max(left,er),overlapTop,right-max(left,er),overlapBottom-overlapTop]]
    let components = try background(image, rect:erased, page:page).components ?? [1,1,1,1]
    let expected = components.prefix(3).map { Int(($0 * 255).rounded()) }
    let sx = Double(image.width)/page.width, sy = Double(image.height)/page.height
    for strip in strips where strip[2] > 0.5 && strip[3] > 0.5 {
        let crop = CGRect(x:ceil(strip[0]*sx),y:ceil(strip[1]*sy),width:floor(strip[2]*sx),height:floor(strip[3]*sy))
        guard crop.width >= 1, crop.height >= 1, let sample = image.cropping(to:crop) else { continue }
        let context = try bitmap(sample.width,sample.height)
        context.draw(sample,in:CGRect(x:0,y:0,width:sample.width,height:sample.height))
        guard let data = context.data?.assumingMemoryBound(to:UInt8.self) else { throw LayoutError.message("No se pudo comprobar el espacio libre.") }
        var occupied = 0
        for y in 0..<sample.height { for x in 0..<sample.width {
            let index = y*context.bytesPerRow+x*4
            if (0..<3).contains(where:{ abs(Int(data[index+$0])-expected[$0]) > 45 }) {
                occupied += 1
                try require(occupied <= 8,"El área ampliada contiene una imagen, una línea o contenido existente. Ajusta los bordes para conservarlo.")
            }
        } }
    }
}
func drawRich(_ overlay: RichOverlay, page: Page, image: CGImage, context: CGContext) throws {
    let e = overlay.eraseRect, r = overlay.rect
    try require(e.count == 4 && r.count == 4 && (e + r).allSatisfy { $0.isFinite }, "Zona de edición no válida.")
    try checkExpansion(image, rect:r, erased:e, page:page)
    try checkOriginalGraphics(image, overlay:overlay, page:page)
    let erase = CGRect(x: e[0], y: page.height-e[1]-e[3], width: e[2], height: e[3])
    context.setFillColor(try background(image, rect: e, page: page)); context.fill(erase)
    let text = NSMutableAttributedString(string: "")
    for (index, paragraph) in overlay.paragraphs.enumerated() {
        if index > 0 { text.append(NSAttributedString(string: "\n")) }
        let start = text.length
        for run in paragraph.runs {
            try require(run.size.isFinite && run.size >= 4 && run.size <= 96 && run.color.count == 3 && run.color.allSatisfy { $0.isFinite && $0 >= 0 && $0 <= 1 }, "Formato de texto no válido.")
            let names = ["system":"Helvetica", "helvetica":"Helvetica", "times":"Times New Roman", "courier":"Courier New"]
            let requested = names[run.font.lowercased()] ?? run.font
            var font = NSFont(name: requested, size: run.size) ?? NSFont(name: "Helvetica", size: run.size)!
            if run.bold { font = NSFontManager.shared.convert(font, toHaveTrait: .boldFontMask) }
            if run.italic { font = NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask) }
            text.append(NSAttributedString(string: run.text, attributes: [.font:font, .foregroundColor:NSColor(red:run.color[0],green:run.color[1],blue:run.color[2],alpha:1), .underlineStyle:run.underline ? NSUnderlineStyle.single.rawValue : 0]))
        }
        let style = NSMutableParagraphStyle()
        style.alignment = paragraph.align == "center" ? .center : paragraph.align == "right" ? .right : .left
        style.headIndent = paragraph.left; style.firstLineHeadIndent = paragraph.left + paragraph.first; style.tailIndent = -paragraph.right
        style.lineBreakMode = .byWordWrapping
        text.addAttribute(.paragraphStyle, value:style, range:NSRange(location:start,length:text.length-start))
    }
    if text.length == 0 { return }
    let rect = CGRect(x:r[0],y:page.height-r[1]-r[3],width:r[2],height:r[3])
    let setter = CTFramesetterCreateWithAttributedString(text)
    let frame = CTFramesetterCreateFrame(setter,CFRange(location:0,length:0),CGPath(rect:rect,transform:nil),nil)
    let visible = CTFrameGetVisibleStringRange(frame)
    try require(visible.length == text.length, "El texto corregido no cabe en su zona. Reduce el tamaño o acorta el texto antes de guardar.")
    context.saveGState(); context.textMatrix = .identity
    CTFrameDraw(frame,context); context.restoreGState()
}
// The bounding rectangle also contains spaces between letters and lines.
// Only the immutable source glyphs may be replaced; reject other marks in
// that rectangle before painting anything. This intentionally fails closed
// on complex backgrounds instead of silently flattening them.
func checkOriginalGraphics(_ image: CGImage, overlay: RichOverlay, page: Page) throws {
    let e = overlay.eraseRect
    let sx = Double(image.width)/page.width, sy = Double(image.height)/page.height
    let x = max(0, floor(e[0]*sx)), y = max(0, floor(e[1]*sy))
    let right = min(Double(image.width), ceil((e[0]+e[2])*sx))
    let bottom = min(Double(image.height), ceil((e[1]+e[3])*sy))
    let crop = CGRect(x:x,y:y,width:right-x,height:bottom-y)
    guard crop.width > 0, crop.height > 0, let sample = image.cropping(to:crop) else {
        throw LayoutError.message("Zona de edición no válida.")
    }
    let pixels = try bitmap(sample.width,sample.height)
    pixels.draw(sample,in:CGRect(x:0,y:0,width:sample.width,height:sample.height))
    let mask = try bitmap(sample.width,sample.height)
    mask.setShouldAntialias(false); mask.setFillColor(CGColor(gray:0,alpha:1))
    try require(!overlay.eraseRegions.isEmpty,"No se pudo comprobar el texto original.")
    for g in overlay.eraseRegions {
        try require(g.count == 4 && g.allSatisfy { $0.isFinite } && g[2] >= 0 && g[3] >= 0,"Zona de texto original no válida.")
        // Match the existing 0.7pt erasure padding, including antialiasing.
        let left = floor((g[0]-0.7)*sx)-x, top = floor((g[1]-0.7)*sy)-y
        let width = ceil((g[0]+g[2]+0.7)*sx)-x-left
        let height = ceil((g[1]+g[3]+0.7)*sy)-y-top
        mask.fill(CGRect(x:left,y:Double(sample.height)-top-height,width:width,height:height))
    }
    let expected = (try background(image,rect:e,page:page).components ?? [1,1,1,1]).prefix(3).map { Int(($0*255).rounded()) }
    guard let data = pixels.data?.assumingMemoryBound(to:UInt8.self), let allowed = mask.data?.assumingMemoryBound(to:UInt8.self) else {
        throw LayoutError.message("No se pudo comprobar el contenido de la página.")
    }
    for row in 0..<sample.height { for column in 0..<sample.width {
        if allowed[row*mask.bytesPerRow+column*4] == 0 { continue }
        let index = row*pixels.bytesPerRow+column*4
        try require(!(0..<3).contains { abs(Int(data[index+$0])-expected[$0]) > 8 },
            "El párrafo contiene una imagen, una línea o un borde de tabla. No se guardó para conservar ese contenido.")
    } }
}
enum LayoutError: Error { case message(String) }
func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data((message + "\n").utf8)); exit(1)
}
func require(_ condition: Bool, _ message: String) throws {
    if !condition { throw LayoutError.message(message) }
}
func bitmap(_ width: Int, _ height: Int) throws -> CGContext {
    try require(width > 0 && height > 0 && width <= 40_000_000 / height, "Página demasiado grande.")
    guard let context = CGContext(data: nil, width: width, height: height, bitsPerComponent: 8,
        bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(), bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
    else { throw LayoutError.message("No se pudo preparar la página.") }
    context.setFillColor(CGColor(gray: 1, alpha: 1)); context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    return context
}
func png(_ image: CGImage) throws -> String {
    let bytes = NSMutableData()
    guard let target = CGImageDestinationCreateWithData(bytes, "public.png" as CFString, 1, nil)
    else { throw LayoutError.message("No se pudo preparar la vista previa.") }
    CGImageDestinationAddImage(target, image, nil)
    try require(CGImageDestinationFinalize(target), "No se pudo preparar la vista previa.")
    return (bytes as Data).base64EncodedString()
}
func topRect(_ rect: CGRect, _ width: Double, _ height: Double) -> [Double] {
    let clipped = rect.intersection(CGRect(x: 0, y: 0, width: width, height: height))
    if clipped.isNull { return [0, 0, 0, 0] }
    return [clipped.minX, height - clipped.maxY, clipped.width, clipped.height]
}
func ocr(_ image: CGImage, width: Double, height: Double, offset: Int, lineOffset: Int) throws -> (String, [Glyph]) {
    let request = VNRecognizeTextRequest()
    request.recognitionLevel = .accurate; request.usesLanguageCorrection = false
    request.recognitionLanguages = ["es-ES", "en-US"]
    try VNImageRequestHandler(cgImage: image, options: [:]).perform([request])
    var text = ""; var glyphs: [Glyph] = []
    for (line, observation) in (request.results ?? []).enumerated() {
        guard let candidate = observation.topCandidates(1).first else { continue }
        if !text.isEmpty { text += "\n" }
        var position = offset + text.utf8.count
        for index in candidate.string.indices {
            let next = candidate.string.index(after: index)
            let character = String(candidate.string[index..<next])
            if !character.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                guard let box = try candidate.boundingBox(for: index..<next)?.boundingBox else {
                    throw LayoutError.message("OCR sin coordenadas: no se puede conservar el formato con seguridad.")
                }
                let rect = CGRect(x: box.minX * width, y: box.minY * height, width: box.width * width, height: box.height * height)
                glyphs.append(Glyph(start: position, end: position + character.utf8.count, line: lineOffset + line,
                    rect: topRect(rect, width, height), fontSize: max(5, rect.height * 0.85), fontName: "Helvetica",
                    color: [0, 0, 0], confidence: Double(candidate.confidence)))
            }
            position += character.utf8.count
        }
        text += candidate.string
    }
    return (text, glyphs)
}
func embedded(_ page: PDFPage, text: String, transform: CGAffineTransform, width: Double, height: Double, offset: Int) -> [Glyph] {
    var result: [Glyph] = []; var position = offset; var line = 0
    let attributes = page.attributedString
    for index in text.indices {
        let next = text.index(after: index); let character = String(text[index..<next])
        let range = NSRange(index..<next, in: text)
        if character.contains("\n") { line += 1 }
        if !character.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            // PDFKit characterBounds indexes physical glyphs and skips
            // synthesized line breaks. Selection ranges index page.string,
            // keeping UTF-16 positions aligned after every newline/ligature.
            let rect = page.selection(for: range)?.bounds(for: page) ?? CGRect.null
            if !rect.isNull {
                let values = (attributes != nil && range.location < attributes!.length) ? attributes!.attributes(at: range.location, effectiveRange: nil) : [:]
                let font = values[.font] as? NSFont
                let color = (values[.foregroundColor] as? NSColor)?.usingColorSpace(.deviceRGB)
                result.append(Glyph(start: position, end: position + character.utf8.count, line: line,
                    rect: topRect(rect.applying(transform), width, height), fontSize: Double(font?.pointSize ?? rect.height * 0.8),
                    fontName: font?.fontName ?? "Helvetica", color: color.map { [Double($0.redComponent), Double($0.greenComponent), Double($0.blueComponent)] } ?? [0, 0, 0], confidence: 1))
            }
        }
        position += character.utf8.count
    }
    return result
}
func extract(_ url: URL) throws -> Document {
    var document = Document(text: "", pages: []); var totalPixels = 0
    if url.pathExtension.lowercased() == "pdf" {
        guard let input = PDFDocument(url: url), !input.isLocked else { throw LayoutError.message("El PDF no es válido o está bloqueado.") }
        try require(input.pageCount > 0 && input.pageCount <= 30, "Se admiten hasta 30 páginas.")
        for number in 0..<input.pageCount {
            guard let page = input.page(at: number), let reference = page.pageRef else { throw LayoutError.message("No se pudo leer una página.") }
            let bounds = reference.getBoxRect(.cropBox)
            let rotated = abs(reference.rotationAngle) % 180 != 0
            let width = rotated ? bounds.height : bounds.width; let height = rotated ? bounds.width : bounds.height
            try require(width > 0 && height > 0 && max(width, height) <= 20_000, "Tamaño de página no admitido.")
            let scale = min(4, 3600 / max(width, height))
            let pixelsWide = Int(ceil(width * scale)); let pixelsHigh = Int(ceil(height * scale))
            totalPixels += pixelsWide * pixelsHigh
            try require(totalPixels <= 100_000_000, "El documento supera el límite de resolución. Divide sus páginas.")
            let context = try bitmap(pixelsWide, pixelsHigh)
            let transform = reference.getDrawingTransform(.cropBox, rect: CGRect(x: 0, y: 0, width: width, height: height), rotate: 0, preserveAspectRatio: true)
            context.scaleBy(x: Double(pixelsWide) / width, y: Double(pixelsHigh) / height)
            context.concatenate(transform); context.drawPDFPage(reference)
            guard let image = context.makeImage() else { throw LayoutError.message("No se pudo leer una página.") }
            if !document.pages.isEmpty { document.text += "\n\n" }
            let offset = document.text.utf8.count
            let native = page.string ?? ""
            let (recognized, recognizedGlyphs) = try ocr(image, width: width, height: height, offset: offset, lineOffset: 0)
            let useOCR = native.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || recognized.utf8.count > native.utf8.count * 3 / 2 + 60
            let selected = useOCR ? recognized : native
            let glyphs = useOCR ? recognizedGlyphs : embedded(page, text: native, transform: transform, width: width, height: height, offset: offset)
            document.text += selected
            document.pages.append(Page(width: width, height: height, imageBase64: try png(image), glyphs: glyphs, source: useOCR ? "ocr" : "embedded"))
            try require(document.text.utf8.count <= 64_000, "El texto supera 64 KB. Divide el documento.")
        }
    } else {
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { throw LayoutError.message("Imagen no válida.") }
        let count = CGImageSourceGetCount(source)
        try require(count > 0 && count <= 30, "Se admiten hasta 30 páginas.")
        for number in 0..<count {
            let options: [CFString: Any] = [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceCreateThumbnailWithTransform: true, kCGImageSourceThumbnailMaxPixelSize: 3600]
            guard let image = CGImageSourceCreateThumbnailAtIndex(source, number, options as CFDictionary) else { throw LayoutError.message("Imagen no válida.") }
            totalPixels += image.width * image.height
            try require(totalPixels <= 100_000_000, "El documento supera el límite de resolución.")
            if number > 0 { document.text += "\n\n" }
            let width = Double(image.width) / 4; let height = Double(image.height) / 4
            let (text, glyphs) = try ocr(image, width: width, height: height, offset: document.text.utf8.count, lineOffset: 0)
            document.text += text
            document.pages.append(Page(width: width, height: height, imageBase64: try png(image), glyphs: glyphs, source: "ocr"))
            try require(document.text.utf8.count <= 64_000, "El texto supera 64 KB. Divide el documento.")
        }
    }
    try require(!document.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty, "No se encontró texto legible.")
    return document
}
// Match the surrounding flat background instead of introducing white holes
// into colored tables/headers. Complex imagery still requires visual review.
func background(_ image: CGImage, rect: [Double], page: Page) throws -> CGColor {
    let scaleX = Double(image.width) / page.width, scaleY = Double(image.height) / page.height
    let points = [(rect[0] - 0.5, rect[1] - 0.5), (rect[0] + rect[2] + 0.5, rect[1] - 0.5),
                  (rect[0] - 0.5, rect[1] + rect[3] + 0.5), (rect[0] + rect[2] + 0.5, rect[1] + rect[3] + 0.5)]
    var frequencies: [UInt32: Int] = [:]
    for (x, y) in points {
        let crop = CGRect(x: max(0, min(Double(image.width - 3), x * scaleX)), y: max(0, min(Double(image.height - 3), y * scaleY)), width: 3, height: 3)
        guard let sample = image.cropping(to: crop) else { continue }
        let context = try bitmap(3, 3); context.draw(sample, in: CGRect(x: 0, y: 0, width: 3, height: 3))
        guard let bytes = context.data?.assumingMemoryBound(to: UInt8.self) else { continue }
        for row in 0..<3 { for column in 0..<3 {
            let offset = row * context.bytesPerRow + column * 4
            let rgb = UInt32(bytes[offset]) << 16 | UInt32(bytes[offset + 1]) << 8 | UInt32(bytes[offset + 2])
            frequencies[rgb, default: 0] += 1
        } }
    }
    let rgb = frequencies.max { $0.value < $1.value }?.key ?? 0xFFFFFF
    return CGColor(red: Double((rgb >> 16) & 255) / 255, green: Double((rgb >> 8) & 255) / 255, blue: Double(rgb & 255) / 255, alpha: 1)
}
func export(_ plan: ExportPlan, preview: Bool = false) throws -> Data {
    var previews: [Page] = []
    try require(!plan.pages.isEmpty && plan.pages.count <= 30 && plan.redactions.count <= 4000, "Exportación fuera de los límites.")
    let bytes = NSMutableData()
    guard let consumer = CGDataConsumer(data: bytes), let pdf = CGContext(consumer: consumer, mediaBox: nil, nil)
    else { throw LayoutError.message("No se pudo preparar el PDF.") }
    for (number, page) in plan.pages.enumerated() {
        guard let data = Data(base64Encoded: page.imageBase64), let source = CGImageSourceCreateWithData(data as CFData, nil), let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { throw LayoutError.message("Vista previa no válida.") }
        let context = try bitmap(image.width, image.height)
        context.scaleBy(x: Double(image.width) / page.width, y: Double(image.height) / page.height)
        context.draw(image, in: CGRect(x: 0, y: 0, width: page.width, height: page.height))
        for redaction in plan.redactions where redaction.page == number {
            let r = redaction.rect
            try require(r.count == 4 && r.allSatisfy { $0.isFinite } && r[0] >= 0 && r[1] >= 0 && r[2] > 0 && r[3] > 0 && r[0] + r[2] <= page.width + 0.01 && r[1] + r[3] <= page.height + 0.01, "Coordenadas no válidas.")
            let rect = CGRect(x: r[0], y: page.height - r[1] - r[3], width: r[2], height: r[3])
            context.setFillColor(try background(image, rect: r, page: page)); context.fill(rect)
            if !redaction.alias.isEmpty {
                let name = NSFont(name: redaction.fontName, size: redaction.fontSize)?.fontName ?? "Helvetica"
                let size = aliasSize(redaction)
                func line(_ size: Double) -> CTLine {
                    let font = CTFontCreateWithName(name as CFString, size, nil)
                    let color = redaction.color.count == 3 ? redaction.color : [0, 0, 0]
                    return CTLineCreateWithAttributedString(NSAttributedString(string: redaction.alias, attributes: [NSAttributedString.Key(kCTFontAttributeName as String): font, NSAttributedString.Key(kCTForegroundColorAttributeName as String): CGColor(red: color[0], green: color[1], blue: color[2], alpha: 1)]))
                }
                // The private pixels were already erased. A microscopic label
                // must not invalidate protection or be printed clipped.
                if size < 4 { continue }
                let text = line(size); var ascent: CGFloat = 0; var descent: CGFloat = 0
                _ = CTLineGetTypographicBounds(text, &ascent, &descent, nil)
                context.saveGState(); context.clip(to: rect); context.textMatrix = .identity
                context.textPosition = CGPoint(x: rect.minX + 0.5, y: rect.midY - (ascent - descent) / 2)
                CTLineDraw(text, context); context.restoreGState()
            }
        }
        for overlay in plan.overlays ?? [] where overlay.page == number {
            try drawRich(overlay, page:page, image:image, context:context)
        }
        guard let modified = context.makeImage() else { throw LayoutError.message("No se pudo proteger la página.") }
        let box = CGRect(x: 0, y: 0, width: page.width, height: page.height)
        if preview {
            previews.append(Page(width: page.width, height: page.height, imageBase64: try png(modified), glyphs: [], source: page.source))
        } else {
            let boxData = withUnsafeBytes(of: box) { Data($0) }
            pdf.beginPDFPage([kCGPDFContextMediaBox as String: boxData] as CFDictionary)
            pdf.draw(modified, in: box); pdf.endPDFPage()
        }
    }
    pdf.closePDF(); return preview ? try JSONEncoder().encode(previews) : bytes as Data
}
do {
    if CommandLine.arguments.count == 2 && CommandLine.arguments[1] == "--check-aliases" {
        let input = FileHandle.standardInput.readDataToEndOfFile()
        try require(input.count <= 5_000_000, "Demasiadas zonas de texto.")
        let areas = try JSONDecoder().decode([Redaction].self, from: input)
        let failed = Set(areas.filter { !$0.alias.isEmpty && aliasSize($0) < 4 }.compactMap { $0.entityIndex })
        FileHandle.standardOutput.write(try JSONEncoder().encode(Array(failed).sorted()))
    } else if CommandLine.arguments.count == 2 && CommandLine.arguments[1] == "--entities" {
        let input = FileHandle.standardInput.readDataToEndOfFile()
        try require(input.count <= 64_000, "Documento fuera del límite de texto.")
        guard let source = String(data: input, encoding: .utf8) else { fail("Texto no válido.") }
        FileHandle.standardOutput.write(try JSONEncoder().encode(namedEntities(source)))
    } else if CommandLine.arguments.count == 3 && CommandLine.arguments[1] == "--layout" {
        let document = try extract(URL(fileURLWithPath: CommandLine.arguments[2]))
        FileHandle.standardOutput.write(try JSONEncoder().encode(document))
    } else if CommandLine.arguments.count == 2 && ["--export", "--preview"].contains(CommandLine.arguments[1]) {
        let input = FileHandle.standardInput.readDataToEndOfFile()
        try require(input.count <= 180_000_000, "Documento demasiado grande.")
        let plan = try JSONDecoder().decode(ExportPlan.self, from: input)
        FileHandle.standardOutput.write(try export(plan, preview: CommandLine.arguments[1] == "--preview"))
    } else { fail("Uso del componente de formato no válido.") }
} catch LayoutError.message(let message) { fail(message) }
catch { fail("No se pudo conservar el formato del documento.") }
