// Writes every file of images/ from the geometry below, on macOS: swift script/gen-images.swift
//
// The mark is a T in three pieces on a 256-unit grid: the left arm, the right arm and the stem,
// in graphite, light grey and green; docs/mark-colours.md lists the other colour schemes. The
// wordmark is "triad" in Schibsted Grotesk SemiBold (SIL OFL 1.1), kerned and outlined at 284
// units, so no file needs the font.

import AppKit
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let outDir = "images"
let grid: CGFloat = 256
let cornerStroke: CGFloat = 10
let plateRadius: CGFloat = 56
let plateColour = "#F7F7F9"
/// The mark's share of a plate.
let markScale: CGFloat = 0.8
/// Apple's icon grid: the plate fills 824 of 1024 units.
let macPlateShare: CGFloat = 824.0 / 1024.0
let macPlateRadiusShare: CGFloat = 185.4 / 824.0

/// The ink of the mark, stroke included.
let inkLeft: CGFloat = 27, inkRight: CGFloat = 229, inkTop: CGFloat = 37, inkBottom: CGFloat = 219
let wordmarkScale: CGFloat = 0.83
let wordmarkGap: CGFloat = 38
let wordmarkWidth: CGFloat = 596 * wordmarkScale
let wordmarkOvershoot: CGFloat = 4

struct Tones { let left, right, stem, text: String }
let onLight = Tones(left: "#2A2F3A", right: "#A9B0BC", stem: "#00B140", text: "#1F2430")
let onDark = Tones(left: "#E4E7EC", right: "#5A6170", stem: "#3EE07A", text: "#F2F3F5")

let leftArm = "M32 42H118V130L32 84Z"
let rightArm = "M224 42H138V130L224 84Z"
let stem = "M98 142L128 158L158 142V214H98Z"
let wordmark = "M59.2 0Q45.6 0 37.6 -7.3Q29.5 -14.6 29.5 -27.3L29.5 -123.7L7.1 -123.7L7.1 -149.8L29.5 -149.8L29.5 -179.9L63.8 -187.9L63.8 -149.8L92.4 -149.8L92.4 -123.7L63.8 -123.7L63.8 -33.8Q63.8 -29.1 65.1 -27.6Q66.4 -26.1 70.7 -26.1L93.6 -26.1L93.6 0ZM118.7 0L118.7 -149.8L151.6 -149.8L151.6 -116.8L150.3 -120.9Q157.3 -137.1 170.3 -145.2Q183.3 -153.2 198 -153.2Q202.2 -153.2 206.4 -152.5Q210.6 -151.8 214.5 -150.2L211.5 -120.1Q203 -122.6 195 -122.6Q189 -122.6 181.7 -120.4Q174.5 -118.3 167.9 -112.5Q161.4 -106.6 157.2 -95.8Q153 -84.9 153 -67.5L153 0ZM234.4 0L234.4 -149.8L268.6 -149.8L268.6 0ZM251.4 -170.4Q240.7 -170.4 234 -177.2Q227.2 -184 227.2 -194.7Q227.2 -205.5 234 -212.2Q240.7 -219 251.4 -219Q262.2 -219 269 -212.2Q275.7 -205.5 275.7 -194.7Q275.7 -184 269 -177.2Q262.2 -170.4 251.4 -170.4ZM345.4 3.5Q330.9 3.5 319.2 -1.8Q307.6 -7.1 300.8 -17.1Q294 -27.2 294 -41.2Q294 -56.3 302.8 -67.6Q311.6 -78.9 328.9 -85.1Q346.1 -91.4 371.7 -91.4L394.1 -91.4L391.6 -88.1L391.6 -96.5Q391.6 -112.6 385.5 -120.8Q379.4 -129 365.3 -129Q351.8 -129 344.6 -121.9Q337.4 -114.8 336.3 -102.3L300.4 -109.1Q302.2 -121.5 310.4 -131.5Q318.7 -141.4 333 -147.4Q347.3 -153.4 367.4 -153.4Q386.5 -153.4 399.4 -146.9Q412.3 -140.3 418.7 -126.7Q425.2 -113 425.2 -91.4L425.2 -33.8Q425.2 -29.1 426.5 -27.6Q427.8 -26.1 432.1 -26.1L439.6 -26.1L439.6 0L421.7 0Q408.1 0 400.5 -7.3Q392.9 -14.6 392.9 -27.3L392.9 -35.8L398.7 -34.1Q396.8 -28.2 393 -21.6Q389.3 -15.1 383 -9.4Q376.8 -3.7 367.6 -0.1Q358.3 3.5 345.4 3.5ZM354.9 -21.4Q365.3 -21.4 373.7 -26.3Q382.1 -31.2 387 -41.4Q391.9 -51.6 391.9 -67.1L391.9 -69.6L372.2 -69.6Q356 -69.6 346.8 -65.4Q337.7 -61.2 333.9 -55Q330.2 -48.8 330.2 -42.7Q330.2 -37.4 332.6 -32.5Q335 -27.6 340.5 -24.5Q345.9 -21.4 354.9 -21.4ZM514.2 3.5Q495.8 3.5 481.8 -6Q467.9 -15.4 460.3 -33.1Q452.8 -50.8 452.8 -75.2Q452.8 -99.6 460.4 -117Q468 -134.5 482 -143.9Q495.9 -153.2 514.6 -153.2Q530.4 -153.2 544 -145.7Q557.6 -138.3 565.4 -120.8L561.8 -120.2L561.8 -208L596.2 -208L596.2 0L563.6 0L563.6 -30.5L564.1 -28.4Q557.2 -12.1 543.8 -4.3Q530.4 3.5 514.2 3.5ZM525.9 -23.2Q536.4 -23.2 544.9 -28.7Q553.3 -34.3 558.3 -45.7Q563.3 -57.1 563.3 -74.6Q563.3 -100.8 552.5 -113.7Q541.7 -126.6 526.1 -126.6Q509.8 -126.6 499.6 -112.9Q489.4 -99.3 489.4 -74.3Q489.4 -50.2 499.2 -36.7Q509.1 -23.2 525.9 -23.2Z"

// SVG

func markGroup(_ t: Tones) -> String {
    """
    <g stroke-width="\(Int(cornerStroke))" stroke-linejoin="round">
        <path fill="\(t.left)" stroke="\(t.left)" d="\(leftArm)"/>
        <path fill="\(t.right)" stroke="\(t.right)" d="\(rightArm)"/>
        <path fill="\(t.stem)" stroke="\(t.stem)" d="\(stem)"/>
      </g>
    """
}

func svg(viewBox: String, _ body: String) -> String {
    "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"\(viewBox)\">\n  \(body)\n</svg>\n"
}

let wordmarkX = inkRight + wordmarkGap
let logoWidth = wordmarkX + wordmarkWidth - inkLeft
let logoHeight = inkBottom - inkTop + wordmarkOvershoot

func logoSvg(_ t: Tones) -> String {
    let text = "<path fill=\"\(t.text)\" transform=\"translate(\(wordmarkX) \(inkBottom)) scale(\(wordmarkScale))\" d=\"\(wordmark)\"/>"
    return svg(viewBox: "\(inkLeft) \(inkTop) \(logoWidth) \(logoHeight)", markGroup(t) + "\n  " + text)
}

func iconSvg() -> String {
    let offset = grid * (1 - markScale) / 2
    let plate = "<rect width=\"\(Int(grid))\" height=\"\(Int(grid))\" rx=\"\(Int(plateRadius))\" fill=\"\(plateColour)\"/>"
    let mark = "<g transform=\"translate(\(offset) \(offset)) scale(\(markScale))\">\n  \(markGroup(onLight))\n  </g>"
    return svg(viewBox: "0 0 \(Int(grid)) \(Int(grid))", plate + "\n  " + mark)
}

// Paths, for the raster files. Reads the absolute M, L, H, V, Q, C and Z the paths above use.

func path(_ d: String) -> CGPath {
    let path = CGMutablePath()
    let scanner = Scanner(string: d)
    scanner.charactersToBeSkipped = CharacterSet.whitespaces
    let commands = CharacterSet(charactersIn: "MLHVQCZ")
    func numbers(_ count: Int) -> [CGFloat] {
        (0..<count).map { _ in CGFloat(scanner.scanDouble()!) }
    }
    while let command = scanner.scanCharacters(from: commands) {
        for letter in command {
            switch letter {
            case "M": let n = numbers(2); path.move(to: CGPoint(x: n[0], y: n[1]))
            case "L": let n = numbers(2); path.addLine(to: CGPoint(x: n[0], y: n[1]))
            case "H": path.addLine(to: CGPoint(x: numbers(1)[0], y: path.currentPoint.y))
            case "V": path.addLine(to: CGPoint(x: path.currentPoint.x, y: numbers(1)[0]))
            case "Q":
                let n = numbers(4)
                path.addQuadCurve(to: CGPoint(x: n[2], y: n[3]), control: CGPoint(x: n[0], y: n[1]))
            case "C":
                let n = numbers(6)
                path.addCurve(
                    to: CGPoint(x: n[4], y: n[5]), control1: CGPoint(x: n[0], y: n[1]),
                    control2: CGPoint(x: n[2], y: n[3]))
            default: path.closeSubpath()
            }
        }
    }
    return path
}

func colour(_ hex: String) -> CGColor {
    let value = Int(hex.dropFirst(), radix: 16)!
    let channel = { (shift: Int) in CGFloat((value >> shift) & 0xFF) / 255 }
    return CGColor(srgbRed: channel(16), green: channel(8), blue: channel(0), alpha: 1)
}

func drawMark(_ t: Tones, in context: CGContext) {
    for (d, hex) in [(leftArm, t.left), (rightArm, t.right), (stem, t.stem)] {
        context.addPath(path(d))
        context.setFillColor(colour(hex))
        context.setStrokeColor(colour(hex))
        context.setLineWidth(cornerStroke)
        context.setLineJoin(.round)
        context.drawPath(using: .fillStroke)
    }
}

func canvas(width: Int, height: Int, draw: (CGContext) -> Void) -> CGImage {
    let context = CGContext(
        data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
        space: CGColorSpace(name: CGColorSpace.sRGB)!,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    context.translateBy(x: 0, y: CGFloat(height))
    context.scaleBy(x: 1, y: -1)
    draw(context)
    return context.makeImage()!
}

func drawPlate(side: CGFloat, radius: CGFloat, in context: CGContext) {
    context.addPath(
        CGPath(
            roundedRect: CGRect(x: 0, y: 0, width: side, height: side), cornerWidth: radius,
            cornerHeight: radius, transform: nil))
    context.setFillColor(colour(plateColour))
    context.fillPath()
    context.scaleBy(x: side / grid, y: side / grid)
    let offset = grid * (1 - markScale) / 2
    context.translateBy(x: offset, y: offset)
    context.scaleBy(x: markScale, y: markScale)
    drawMark(onLight, in: context)
}

/// The plate that fills the image: Linux, Windows and the README.
func icon(_ size: Int) -> CGImage {
    canvas(width: size, height: size) { context in
        let side = CGFloat(size)
        drawPlate(side: side, radius: side * plateRadius / grid, in: context)
    }
}

/// The plate inside Apple's margin.
func macIcon(_ size: Int) -> CGImage {
    canvas(width: size, height: size) { context in
        let side = CGFloat(size) * macPlateShare
        let margin = (CGFloat(size) - side) / 2
        context.translateBy(x: margin, y: margin)
        drawPlate(side: side, radius: side * macPlateRadiusShare, in: context)
    }
}

func mark(_ t: Tones, _ size: Int) -> CGImage {
    canvas(width: size, height: size) { context in
        context.scaleBy(x: CGFloat(size) / grid, y: CGFloat(size) / grid)
        drawMark(t, in: context)
    }
}

func logo(_ t: Tones, height: Int) -> CGImage {
    let scale = CGFloat(height) / logoHeight
    return canvas(width: Int((logoWidth * scale).rounded(.up)), height: height) { context in
        context.scaleBy(x: scale, y: scale)
        context.translateBy(x: -inkLeft, y: -inkTop)
        drawMark(t, in: context)
        context.translateBy(x: wordmarkX, y: inkBottom)
        context.scaleBy(x: wordmarkScale, y: wordmarkScale)
        context.addPath(path(wordmark))
        context.setFillColor(colour(t.text))
        context.fillPath()
    }
}

// Files

func png(_ image: CGImage) -> Data {
    let data = NSMutableData()
    let destination = CGImageDestinationCreateWithData(
        data, UTType.png.identifier as CFString, 1, nil)!
    CGImageDestinationAddImage(destination, image, nil)
    CGImageDestinationFinalize(destination)
    return data as Data
}

func write(_ name: String, _ data: Data) throws {
    try data.write(to: URL(fileURLWithPath: "\(outDir)/\(name)"))
}

func little<T: FixedWidthInteger>(_ value: T) -> Data {
    withUnsafeBytes(of: value.littleEndian) { Data($0) }
}

/// An ICO of PNG frames. A size byte of 0 means 256.
func ico(sizes: [Int]) -> Data {
    let headerBytes = 6, entryBytes = 16, colourPlanes: UInt16 = 1, bitsPerPixel: UInt16 = 32
    let iconType: UInt16 = 1
    let frames = sizes.map { png(icon($0)) }
    var out = little(UInt16(0)) + little(iconType) + little(UInt16(sizes.count))
    var offset = headerBytes + entryBytes * sizes.count
    for (size, frame) in zip(sizes, frames) {
        let side = UInt8(size % 256)
        out += Data([side, side, 0, 0]) + little(colourPlanes) + little(bitsPerPixel)
        out += little(UInt32(frame.count)) + little(UInt32(offset))
        offset += frame.count
    }
    return frames.reduce(out, +)
}

func icns() throws {
    let iconset = NSTemporaryDirectory() + "triad-\(ProcessInfo.processInfo.processIdentifier).iconset"
    try FileManager.default.createDirectory(atPath: iconset, withIntermediateDirectories: true)
    defer { try? FileManager.default.removeItem(atPath: iconset) }
    for points in [16, 32, 128, 256, 512] {
        try png(macIcon(points)).write(to: URL(fileURLWithPath: "\(iconset)/icon_\(points)x\(points).png"))
        try png(macIcon(points * 2)).write(to: URL(fileURLWithPath: "\(iconset)/icon_\(points)x\(points)@2x.png"))
    }
    let tool = Process()
    tool.executableURL = URL(fileURLWithPath: "/usr/bin/iconutil")
    tool.arguments = ["-c", "icns", iconset, "-o", "\(outDir)/triad.icns"]
    try tool.run()
    tool.waitUntilExit()
    guard tool.terminationStatus == 0 else { fatalError("iconutil failed") }
}

try FileManager.default.createDirectory(atPath: outDir, withIntermediateDirectories: true)
let markBox = "0 0 \(Int(grid)) \(Int(grid))"
try write("triad-mark.svg", Data(svg(viewBox: markBox, markGroup(onLight)).utf8))
try write("triad-mark-dark.svg", Data(svg(viewBox: markBox, markGroup(onDark)).utf8))
try write("triad-icon.svg", Data(iconSvg().utf8))
try write("triad-logo.svg", Data(logoSvg(onLight).utf8))
try write("triad-logo-dark.svg", Data(logoSvg(onDark).utf8))
for size in [16, 32, 64, 128, 256, 512, 1024] {
    try write("triad-icon-\(size).png", png(icon(size)))
}
let markPixels = 512, logoPixels = 256
try write("triad-mark.png", png(mark(onLight, markPixels)))
try write("triad-mark-dark.png", png(mark(onDark, markPixels)))
try write("triad-logo.png", png(logo(onLight, height: logoPixels)))
try write("triad-logo-dark.png", png(logo(onDark, height: logoPixels)))
try write("triad.ico", ico(sizes: [16, 24, 32, 48, 64, 128, 256]))
try icns()
