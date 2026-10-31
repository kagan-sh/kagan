#!/bin/sh
# Rebuild the app icon PNG files, kagan.icns, kagan.ico and the book's favicon from assets/brand/kagan-app-icon.svg.
# Needs rsvg-convert (librsvg), iconutil (macOS) and magick (ImageMagick). Safe to re-run.
set -eu

cd "$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)"

src=assets/brand/kagan-app-icon.svg
png=assets/brand/png
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

for size in 16 32 64 128 256 512 1024; do
  rsvg-convert -w "$size" -h "$size" "$src" -o "$png/kagan-app-icon-$size.png"
done

set_dir="$work/kagan.iconset"
mkdir "$set_dir"
for pair in 16:icon_16x16 32:icon_16x16@2x 32:icon_32x32 64:icon_32x32@2x \
  128:icon_128x128 256:icon_128x128@2x 256:icon_256x256 512:icon_256x256@2x \
  512:icon_512x512 1024:icon_512x512@2x; do
  cp "$png/kagan-app-icon-${pair%%:*}.png" "$set_dir/${pair#*:}.png"
done
rm -f assets/brand/kagan.icns
iconutil -c icns "$set_dir" -o assets/brand/kagan.icns

rm -f assets/brand/kagan.ico
magick "$png/kagan-app-icon-16.png" "$png/kagan-app-icon-32.png" "$png/kagan-app-icon-64.png" \
  "$png/kagan-app-icon-128.png" "$png/kagan-app-icon-256.png" assets/brand/kagan.ico
cp "$src" book/theme/favicon.svg
cp "$png/kagan-app-icon-256.png" book/theme/favicon.png
echo "brand-icons: wrote $png/kagan-app-icon-{16..1024}.png, assets/brand/kagan.icns, assets/brand/kagan.ico, book/theme/favicon.{svg,png}"
