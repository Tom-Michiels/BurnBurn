// Reference tool (research only, never an application dependency): independent QR oracle for
// burnburn-generators' encoder tests, using Apple's CoreImage generator and Vision decoder.
//
//   swift reference.swift generate <L|M|Q|H> <message>   -> module matrix rows ('1' = dark), no quiet zone
//   swift reference.swift decode <matrix-file>            -> payload string Vision reads from the matrix
import CoreImage
import Foundation
import Vision

func render(_ rows: [[Bool]], scale: Int = 8, quiet: Int = 4) -> CGImage? {
    let n = rows.count
    let side = (n + 2 * quiet) * scale
    var pixels = [UInt8](repeating: 255, count: side * side)
    for (r, row) in rows.enumerated() {
        for (c, dark) in row.enumerated() where dark {
            for y in 0..<scale {
                for x in 0..<scale {
                    pixels[((r + quiet) * scale + y) * side + (c + quiet) * scale + x] = 0
                }
            }
        }
    }
    let provider = CGDataProvider(data: Data(pixels) as CFData)!
    return CGImage(width: side, height: side, bitsPerComponent: 8, bitsPerPixel: 8, bytesPerRow: side,
                   space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGBitmapInfo(rawValue: 0),
                   provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)
}

let args = CommandLine.arguments
if args.count >= 4 && args[1] == "generate" {
    let filter = CIFilter(name: "CIQRCodeGenerator")!
    filter.setValue(args[3].data(using: .utf8)!, forKey: "inputMessage")
    filter.setValue(args[2], forKey: "inputCorrectionLevel")
    let image = filter.outputImage!
    let ctx = CIContext()
    let cg = ctx.createCGImage(image, from: image.extent)!
    let w = cg.width, h = cg.height
    var buf = [UInt8](repeating: 0, count: w * h)
    let gray = CGContext(data: &buf, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w,
                         space: CGColorSpaceCreateDeviceGray(), bitmapInfo: 0)!
    gray.draw(cg, in: CGRect(x: 0, y: 0, width: w, height: h))
    // Strip the generator's quiet zone (all-light border rows/columns).
    let dark = { (r: Int, c: Int) in buf[r * w + c] < 128 }
    let rows = (0..<h).filter { r in (0..<w).contains { dark(r, $0) } }
    let cols = (0..<w).filter { c in (0..<h).contains { dark($0, c) } }
    for r in rows.first!...rows.last! {
        print(String((cols.first!...cols.last!).map { dark(r, $0) ? "1" : "0" }))
    }
} else if args.count >= 3 && args[1] == "decode" {
    let text = try! String(contentsOfFile: args[2], encoding: .utf8)
    let rows = text.split(separator: "\n").map { $0.map { $0 == "1" } }
    let request = VNDetectBarcodesRequest()
    request.symbologies = [.qr]
    try! VNImageRequestHandler(cgImage: render(rows)!, options: [:]).perform([request])
    for r in request.results ?? [] {
        print(r.payloadStringValue ?? "<no string>")
    }
} else {
    print("usage: reference.swift generate <L|M|Q|H> <message> | decode <matrix-file>")
    exit(2)
}
