#!/bin/zsh
# Builds subwy in release and assembles target/bundle/subwy.app, signed ad hoc.
#   scripts/bundle.sh          the app
#   scripts/bundle.sh --dmg    the app and target/bundle/subwy-<version>.dmg
# It only writes under target/; copying subwy.app to /Applications is up to you.
set -euo pipefail

dmg=false
case ${1:-} in
  --dmg) dmg=true ;;
  "") ;;
  *) echo "usage: scripts/bundle.sh [--dmg]" >&2; exit 2 ;;
esac

repo=${0:A:h:h}
cd "$repo"

# `cargo pkgid` ends in "#subwy@<version>".
pkgid=$(cargo pkgid -p subwy)
version=${pkgid##*@}

cargo build --release -p subwy

out=target/bundle
app=$out/subwy.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/subwy "$app/Contents/MacOS/subwy"
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist"

# The icon. AppKit draws the SVG onto a transparent canvas (qlmanage would paint a white square
# behind the rounded corners); sips scales it and iconutil packs the sizes macOS asks for.
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
swift - assets/brand/icon.svg "$work/icon.png" 1024 <<'SWIFT'
import AppKit
let args = Array(CommandLine.arguments.suffix(3))
let size = Int(args[2])!
guard let image = NSImage(contentsOfFile: args[0]) else { fatalError("cannot read \(args[0])") }
let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size, bitsPerSample: 8,
    samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
    bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
image.draw(in: NSRect(x: 0, y: 0, width: size, height: size))
NSGraphicsContext.restoreGraphicsState()
try rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: args[1]))
SWIFT
iconset=$work/subwy.iconset
mkdir "$iconset"
for size in 16 32 128 256 512; do
  sips -z $size $size "$work/icon.png" --out "$iconset/icon_${size}x${size}.png" >/dev/null
  sips -z $((size * 2)) $((size * 2)) "$work/icon.png" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/subwy.icns"

# Ad hoc: enough to run on this Mac. Giving it to others needs a Developer ID and notarization.
codesign --force --deep -s - "$app"
codesign --verify --deep --strict "$app"
echo "$app ($version)"

if $dmg; then
  stage=$work/dmg
  mkdir "$stage"
  cp -R "$app" "$stage/"
  ln -s /Applications "$stage/Applications"
  image=$out/subwy-$version.dmg
  hdiutil create -volname subwy -srcfolder "$stage" -ov -format UDZO "$image" >/dev/null
  echo "$image"
fi
