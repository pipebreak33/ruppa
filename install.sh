#!/usr/bin/env sh
# Ruppa för macOS och Linux – installera eller uppdatera (kör igen = senaste versionen):
#   curl -fsSL https://raw.githubusercontent.com/pipebreak33/ruppa/main/install.sh | sh
#
# Allt hamnar i ~/.ruppa:
#   ~/.ruppa/bin/ruppa            programmet (plus uppa, puppa, muppa)
#   ~/.ruppa/uppa, puppa, muppa   kusinerna
#   ~/.ruppa/share/               ikonen
# Dessutom en startare så att Ruppa syns bland programmen:
#   macOS: ~/Applications/Ruppa.app     Linux: ~/.local/share/applications/ruppa.desktop
#
# RUPPA_REPO=ägare/repo och RUPPA_VERSION=v1.2.3 styr källan. RUPPA_BINARY=/sökväg/ruppa installerar en lokal binär.
# RUPPA_HOME flyttar mappen.
set -eu

REPO="${RUPPA_REPO:-pipebreak33/ruppa}"
DIR="${RUPPA_HOME:-$HOME/.ruppa}"
BIN="$DIR/bin"
SHARE="$DIR/share"

say()  { printf '  \033[38;5;202mR›\033[0m %s\n' "$1"; }
warn() { printf '  \033[38;5;214m!\033[0m %s\n' "$1"; }
die()  { printf '  \033[31m✘\033[0m %s\n' "$1" >&2; exit 1; }

OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
  Darwin) ASSET="ruppa-macos-universal" ;;
  Linux)
    case "$ARCH" in
      x86_64|amd64) ASSET="ruppa-linux-x86_64" ;;
      aarch64|arm64) ASSET="ruppa-linux-aarch64" ;;
      *) die "Ingen färdig Ruppa för $ARCH. Bygg själv: cargo build --release" ;;
    esac ;;
  *) die "Okänt system: $OS. På Windows: irm https://raw.githubusercontent.com/$REPO/main/install.ps1 | iex" ;;
esac

mkdir -p "$BIN" "$SHARE"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# 1. Programmet
if [ -n "${RUPPA_BINARY:-}" ]; then
  say "Installerar $RUPPA_BINARY"
  cp "$RUPPA_BINARY" "$TMP/ruppa"
else
  if [ -n "${RUPPA_VERSION:-}" ]; then
    BASE="https://github.com/$REPO/releases/download/$RUPPA_VERSION"
  else
    BASE="https://github.com/$REPO/releases/latest/download"
  fi
  say "Hämtar $ASSET från github.com/$REPO"
  curl -fsSL "$BASE/$ASSET" -o "$TMP/ruppa" || die "nedladdningen misslyckades – finns det en release?"
  curl -fsSL "$BASE/SHA256SUMS" -o "$TMP/SHA256SUMS" || die "kunde inte hämta SHA256SUMS"
  WANT="$(awk -v n="$ASSET" '$2==n || $2=="*"n {print $1}' "$TMP/SHA256SUMS")"
  if command -v shasum >/dev/null 2>&1; then GOT="$(shasum -a 256 "$TMP/ruppa" | awk '{print $1}')"
  else GOT="$(sha256sum "$TMP/ruppa" | awk '{print $1}')"; fi
  [ -n "$WANT" ] && [ "$WANT" = "$GOT" ] || die "kontrollsumman stämmer inte. Inget har ändrats."
fi
chmod +x "$TMP/ruppa"
"$TMP/ruppa" --version >/dev/null 2>&1 || die "den nedladdade binären går inte att köra"
rm -f "$BIN/ruppa"
mv "$TMP/ruppa" "$BIN/ruppa"
say "$("$BIN/ruppa" --version) i $BIN"

# 2. Ikonen
ICON_BASE="https://raw.githubusercontent.com/$REPO/main/assets/icon"
HERE="$(cd "$(dirname "$0")" 2>/dev/null && pwd || true)"
for f in ruppa.png ruppa.icns; do
  if [ -n "$HERE" ] && [ -f "$HERE/assets/icon/$f" ]; then cp "$HERE/assets/icon/$f" "$SHARE/$f"
  else curl -fsSL "$ICON_BASE/$f" -o "$SHARE/$f" 2>/dev/null || true; fi
done

# 3. Startare bland programmen
if [ "$OS" = Darwin ]; then
  APP="$HOME/Applications/Ruppa.app"
  rm -rf "$APP"
  mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
  [ -f "$SHARE/ruppa.icns" ] && cp "$SHARE/ruppa.icns" "$APP/Contents/Resources/ruppa.icns"
  cat > "$APP/Contents/MacOS/Ruppa" <<EOF
#!/bin/sh
exec "$BIN/ruppa" gui
EOF
  chmod +x "$APP/Contents/MacOS/Ruppa"
  VER="$("$BIN/ruppa" --version | awk '{print $2}')"
  cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Ruppa</string>
  <key>CFBundleDisplayName</key><string>Ruppa</string>
  <key>CFBundleIdentifier</key><string>se.ruppa.app</string>
  <key>CFBundleExecutable</key><string>Ruppa</string>
  <key>CFBundleIconFile</key><string>ruppa</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VER</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
EOF
  touch "$APP"
  say "Ruppa.app finns i ~/Applications (Launchpad, Spotlight)"
else
  APPS="$HOME/.local/share/applications"
  mkdir -p "$APPS"
  cat > "$APPS/ruppa.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Ruppa
Comment=Uppa, Puppa och Muppa i ett fönster
Exec=$BIN/ruppa gui
Icon=$SHARE/ruppa.png
Terminal=false
Categories=System;Utility;
EOF
  say "Ruppa finns bland dina program"
fi

# 4. ~/.ruppa/bin i PATH (för uppa, puppa och muppa i terminalen)
case ":$PATH:" in
  *":$BIN:"*) ;;
  *)
    case "${SHELL:-}" in
      */zsh)  RC="$HOME/.zshrc" ;;
      */bash) RC="$HOME/.bashrc"; [ "$OS" = Darwin ] && RC="$HOME/.bash_profile" ;;
      *)      RC="$HOME/.profile" ;;
    esac
    LINE="export PATH=\"$BIN:\$PATH\""
    grep -qsF "$BIN" "$RC" 2>/dev/null || printf '\n# Ruppa\n%s\n' "$LINE" >> "$RC"
    say "La till $BIN i PATH ($RC)"
    ;;
esac

# 5. Kusinerna
printf '\n'
"$BIN/ruppa" install --no-anim || warn "Någon kusin gick inte att installera. Öppna Ruppa och försök igen."

printf '\n  \033[38;5;42m✔\033[0m Klart! Öppna \033[1mRuppa\033[0m bland dina program.\n\n'
if [ "$OS" = Darwin ] && [ -z "${RUPPA_NO_OPEN:-}" ]; then open "$HOME/Applications/Ruppa.app" 2>/dev/null || true; fi
