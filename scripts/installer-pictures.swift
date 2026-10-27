// Draws the Windows setup's pictures (bundle > windows > nsis in tauri.conf.json) from the app icon, as the 24-bit
// BMPs NSIS takes: sidebar.bmp (164 x 314, the welcome and finish pages) and header.bmp (150 x 57, the other pages).
// The BMPs are committed; run this only when the icon changes, and commit the result. macOS only:
//   swiftc -O scripts/installer-pictures.swift -o /tmp/installer-pictures &&
//     /tmp/installer-pictures crates/otaq-desktop/icons/icon.png crates/otaq-desktop/bundle/windows
import AppKit
let args = CommandLine.arguments
let icon = NSImage(contentsOfFile: args[1])!
let src = NSBitmapImageRep(data: icon.tiffRepresentation!)!
let navy = src.colorAt(x: 256, y: 440)!.usingColorSpace(.sRGB)!  // the tile's lower colour
func bmp(_ rep: NSBitmapImageRep) -> Data {
  let w = rep.pixelsWide, h = rep.pixelsHigh, row = (w * 3 + 3) & ~3, size = 54 + row * h
  var d = Data(count: size)
  func put(_ o: Int, _ v: Int, _ n: Int) { for i in 0..<n { d[o + i] = UInt8((v >> (8 * i)) & 0xff) } }
  d[0] = 0x42; d[1] = 0x4d; put(2, size, 4); put(10, 54, 4); put(14, 40, 4); put(18, w, 4); put(22, h, 4)
  put(26, 1, 2); put(28, 24, 2); put(34, row * h, 4)
  for y in 0..<h { for x in 0..<w {
    let c = rep.colorAt(x: x, y: h - 1 - y)!.usingColorSpace(.sRGB)!
    let o = 54 + y * row + x * 3
    d[o] = UInt8(c.blueComponent * 255 + 0.5); d[o + 1] = UInt8(c.greenComponent * 255 + 0.5); d[o + 2] = UInt8(c.redComponent * 255 + 0.5)
  } }
  return d
}
func draw(_ w: Int, _ h: Int, _ name: String, _ body: (CGRect) -> Void) {
  let out = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: w, pixelsHigh: h, bitsPerSample: 8, samplesPerPixel: 4,
    hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 32)!
  NSGraphicsContext.saveGraphicsState()
  NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: out)
  NSGraphicsContext.current!.imageInterpolation = .high
  body(CGRect(x: 0, y: 0, width: w, height: h))
  NSGraphicsContext.restoreGraphicsState()
  try! bmp(out).write(to: URL(fileURLWithPath: args[2] + "/" + name))
}
// Welcome and finish pages: the logo on the tile's navy, in the upper part (the page's text sits to the right).
draw(164, 314, "sidebar.bmp") { r in
  navy.setFill(); r.fill()
  icon.draw(in: CGRect(x: 14, y: 314 - 40 - 136, width: 136, height: 136))
}
// Header of the inner pages: white, the logo at the right.
draw(150, 57, "header.bmp") { r in
  NSColor.white.setFill(); r.fill()
  icon.draw(in: CGRect(x: 150 - 53, y: 2, width: 53, height: 53))
}
