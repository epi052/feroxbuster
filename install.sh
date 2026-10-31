#!/data/data/com.termux/files/usr/bin/bash

# تحديث الحزم
pkg update -y && pkg upgrade -y

# تثبيت rust لو مش موجود
if ! command -v cargo &> /dev/null
then
    echo "[*] Installing Rust..."
    pkg install rust -y
fi

# بناء الأداة
echo "[*] Building feroxbuster..."
cargo build --release

# نسخ الملف التنفيذي للمسار
echo "[*] Copying binary to \$PREFIX/bin"
cp target/release/feroxbuster $PREFIX/bin/

echo "[✔] Installation complete! Now you can run: feroxbuster"
