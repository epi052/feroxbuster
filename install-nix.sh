#!/usr/bin/env bash

BASE_URL=https://github.com/epi052/feroxbuster/releases/latest/download

MAC_ZIP=x86_64-macos-feroxbuster.zip
MAC_URL="$BASE_URL/$MAC_ZIP"

LIN32_ZIP=x86-linux-feroxbuster.zip
LIN32_URL="$BASE_URL/$LIN32_ZIP"

LIN64_ZIP=x86_64-linux-feroxbuster.zip
LIN64_URL="$BASE_URL/$LIN64_ZIP"

EMOJI_URL=https://gist.github.com/epi052/8196b550ea51d0907ad4b93751b1b57d/raw/6112c9f32ae07922983fdc549c54fd3fb9a38e4c/NotoColorEmoji.ttf

INSTALL_DIR="${1:-$(pwd)}"

echo "[+] Installing feroxbuster to ${INSTALL_DIR}!"

which unzip &>/dev/null
if [ "$?" != "0" ]; then
  echo "[!] unzip not found, exiting. "
  exit -1
fi

if [[ "$(uname)" == "Darwin" ]]; then
  echo "[=] Found MacOS, downloading from $MAC_URL"

  curl -sLO "$MAC_URL"
  unzip -o "$MAC_ZIP" -d "${INSTALL_DIR}" >/dev/null
  rm "$MAC_ZIP"
elif [[ "$(expr substr $(uname -s) 1 5)" == "Linux" ]]; then
  if [[ $(getconf LONG_BIT) == 32 ]]; then
    echo "[=] Found 32-bit Linux, downloading from $LIN32_URL"

    curl -sLO "$LIN32_URL"
    unzip -o "$LIN32_ZIP" -d "${INSTALL_DIR}" >/dev/null
    rm "$LIN32_ZIP"
  else
    echo "[=] Found 64-bit Linux, downloading from $LIN64_URL"

    curl -sLO "$LIN64_URL"
    unzip -o "$LIN64_ZIP" -d "${INSTALL_DIR}" >/dev/null
    rm "$LIN64_ZIP"
  fi

  if [[ "$(fc-list NotoColorEmoji | wc -l)" -gt 0 ]]; then
    echo "[=] Found Noto Emoji Font, skipping install"
  else
    echo "[=] Installing Noto Emoji Font"
    mkdir -p ~/.fonts
    pushd ~/.fonts 2>&1 >/dev/null

    curl -sLO "$EMOJI_URL"

    fc-cache -f -v >/dev/null

    popd 2>&1 >/dev/null
    echo "[+] Noto Emoji Font installed"
  fi
fi

chmod +x "${INSTALL_DIR}/feroxbuster"

echo "[+] Installed feroxbuster"
echo "  [-] path: ${INSTALL_DIR}/feroxbuster"
echo "  [-] version: $(${INSTALL_DIR}/feroxbuster -V | awk '{print $2}')"
