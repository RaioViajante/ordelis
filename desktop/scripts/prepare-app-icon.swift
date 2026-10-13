// Prepares the application icon master from the supplied icon.png.
// Keeps the largest opaque region, crops to it and centres it, unscaled in aspect, on a square transparent canvas.
// swiftc -O -o /tmp/iconprep desktop/scripts/prepare-app-icon.swift
// /tmp/iconprep reference/icon.png desktop/src-tauri/icons/app-icon.png 1024 832 8 && (cd desktop && pnpm tauri icon src-tauri/icons/app-icon.png)
import CoreGraphics
import ImageIO
import Foundation
import UniformTypeIdentifiers

// usage: iconprep <in.png> <out.png> <canvas> <artworkLongSide> [alphaThreshold]
let a = CommandLine.arguments
let src = CGImageSourceCreateWithURL(URL(fileURLWithPath: a[1]) as CFURL, nil)!
let img = CGImageSourceCreateImageAtIndex(src, 0, nil)!
let canvas = Int(a[3])!, longSide = Double(a[4])!
let thr = a.count > 5 ? UInt8(a[5])! : 8
let w = img.width, h = img.height
let cs = CGColorSpaceCreateDeviceRGB()
var px = [UInt8](repeating: 0, count: w*h*4)
let ctx = CGContext(data: &px, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w*4, space: cs, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
ctx.draw(img, in: CGRect(x: 0, y: 0, width: w, height: h))
// connected components on alpha > thr
var label = [Int32](repeating: 0, count: w*h)
var sizes: [Int] = [0]
var next: Int32 = 1
var stack = [Int]()
for start in 0..<(w*h) where px[start*4+3] > thr && label[start] == 0 {
  var n = 0; stack.append(start); label[start] = next
  while let p = stack.popLast() {
    n += 1
    let x = p % w, y = p / w
    for (dx, dy) in [(1,0),(-1,0),(0,1),(0,-1)] {
      let nx = x+dx, ny = y+dy
      if nx < 0 || ny < 0 || nx >= w || ny >= h { continue }
      let q = ny*w+nx
      if px[q*4+3] > thr && label[q] == 0 { label[q] = next; stack.append(q) }
    }
  }
  sizes.append(n); next += 1
}
let keep = Int32(sizes.enumerated().max(by: { $0.element < $1.element })!.offset)
print("components:", sizes.count-1, "kept size:", sizes[Int(keep)], "others:", sizes.dropFirst().enumerated().filter{ Int32($0.offset+1) != keep }.map{$0.element}.reduce(0,+))
var minX = w, minY = h, maxX = 0, maxY = 0
for p in 0..<(w*h) {
  if label[p] != keep { px[p*4] = 0; px[p*4+1] = 0; px[p*4+2] = 0; px[p*4+3] = 0 }
  else { let x = p % w, y = p / w; minX = min(minX,x); maxX = max(maxX,x); minY = min(minY,y); maxY = max(maxY,y) }
}
let bw = maxX-minX+1, bh = maxY-minY+1
print("bbox", minX, minY, bw, bh, "aspect", Double(bw)/Double(bh))
let cleaned = ctx.makeImage()!
let crop = cleaned.cropping(to: CGRect(x: minX, y: minY, width: bw, height: bh))!
let scale = longSide / Double(max(bw, bh))
let dw = Double(bw)*scale, dh = Double(bh)*scale
let out = CGContext(data: nil, width: canvas, height: canvas, bitsPerComponent: 8, bytesPerRow: 0, space: cs, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
out.interpolationQuality = .high
out.draw(crop, in: CGRect(x: (Double(canvas)-dw)/2, y: (Double(canvas)-dh)/2, width: dw, height: dh))
let dest = CGImageDestinationCreateWithURL(URL(fileURLWithPath: a[2]) as CFURL, UTType.png.identifier as CFString, 1, nil)!
CGImageDestinationAddImage(dest, out.makeImage()!, nil)
CGImageDestinationFinalize(dest)
print("wrote", a[2], canvas, "artwork", Int(dw), "x", Int(dh))
