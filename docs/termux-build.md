# Building Feroxbuster on Termux (armeabi-v7a / ARM32)

This guide explains how to build Feroxbuster from source on Termux for
32-bit ARM devices (armeabi-v7a), where prebuilt binaries are not available.

---

## ✅ Tested Device
- Architecture: armeabi-v7a (ARM32)
- OS: Android + Termux
- Rust version: latest from Termux repo

---

## ✅ Install Dependencies

```bash
pkg update && pkg upgrade -y
pkg install git clang make cmake pkg-config python rust -y

## clone repository
git clone https://github.com/epi052/feroxbuster.git
cd feroxbuster

## Build from source

cargo build --release

## install to path
cp target/release/feroxbuster $PREFIX/bin/
chmod +x $PREFIX/bin/feroxbuster

## verify installation

feroxbuster --version
## expected output
feroxbuster 2.13.0


## ✅ Contributor

Built & tested by:

GitHub: @pg9051

Platform: Android Termux ARM32


