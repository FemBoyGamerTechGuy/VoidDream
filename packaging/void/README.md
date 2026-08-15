# VoidDream — Void Linux (xbps) packaging

This is an [`xbps-src`](https://github.com/void-linux/void-packages) template
for building VoidDream as a native `.xbps` package.

Unlike the Arch/`deb`/`rpm` scripts elsewhere in `packaging/`, xbps-src
templates must live inside a clone of `void-packages` itself — they can't be
built standalone. This directory holds the template so it's easy to drop into
that tree or submit upstream.

## Building locally

```bash
git clone --depth=1 https://github.com/void-linux/void-packages.git
cd void-packages
./xbps-src binary-bootstrap

mkdir -p srcpkgs/voiddream
cp /path/to/VoidDream/packaging/void/template srcpkgs/voiddream/template

./xbps-src pkg voiddream
sudo xbps-install --repository=hostdir/binpkgs voiddream
```

## Before building

`checksum` in the template is set to `SKIP` as a placeholder. Fill it in with
the real hash of the release tarball before building for real:

```bash
curl -LO https://github.com/FemBoyGamerTechGuy/VoidDream/archive/refs/tags/v0.1.8-2.tar.gz
sha256sum v0.1.8-2.tar.gz
```

Paste that hash into `checksum=` in the template. `xbps-src` will refuse to
build (fetch fails loudly) if the tag doesn't exist yet or the hash doesn't
match, so this only works once the corresponding release tag is pushed.

## What gets installed

Same layout as the other package formats — see the table in
[`packaging/README.md`](../README.md#what-gets-installed).

## Uninstalling

```bash
sudo xbps-remove voiddream
```
