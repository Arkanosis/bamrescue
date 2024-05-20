#! /bin/sh

pkgname=bamrescue
pkgver=0.3.0
pkgrel=1

pkgdir="$(mktemp -d)"

mkdir -p \
  "$pkgdir/usr/bin" \
  "$pkgdir/usr/share/doc/$pkgname" \
  "$pkgdir/usr/share/bash-completion/completions" \
  "$pkgdir/usr/share/zsh/site-functions" \
  "$pkgdir/DEBIAN"

cat > "$pkgdir/DEBIAN/control" <<EOF
Package: $pkgname
Version: $pkgver-$pkgrel
Section: utils
Priority: optional
Architecture: amd64
Depends:
Maintainer: Jérémie Roquet <jroquet@arkanosis.net>
Description: Utility to check Binary Sequence Alignment / Map (BAM) files for corruption and repair them
Homepage: https://bamrescue.arkanosis.net/
EOF

cargo build --release
ronn < "docs/man/$pkgname.1.ronn" | gzip -9 > "docs/man/$pkgname.1.gz"

install -Dm0755 "target/release/$pkgname" "$pkgdir/usr/bin/$pkgname"
install -Dm0644 "docs/man/$pkgname.1.gz" "$pkgdir/usr/share/man/man1/$pkgname.1.gz"
install -Dm0644 "completion/bash/$pkgname" "$pkgdir/usr/share/bash-completion/completions/$pkgname"
install -Dm0644 "completion/zsh/_$pkgname" "$pkgdir/usr/share/zsh/site-functions/_$pkgname"
install -Dm0644 "README.md" "$pkgdir/usr/share/doc/$pkgname/README.md"
install -Dm0644 "LICENSE" "$pkgdir/usr/share/doc/$pkgname/copyright"

dpkg-deb --build "$pkgdir" "bamrescue.deb"

test -n "$pkgdir" && rm -r "$pkgdir"
