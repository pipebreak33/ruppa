#!/usr/bin/env sh
# Avinstallera Ruppa på macOS och Linux:
#   curl -fsSL https://raw.githubusercontent.com/pipebreak33/ruppa/main/uninstall.sh | sh
#
# Tar bort ~/.ruppa (Ruppa, kusinerna och deras startare), Ruppa.app / programmenyposten och PATH-raden.
# Behåller dina data: Puppas tenants (~/.puppa), Muppas mallar (~/.muppa), Euppas tenants (~/.euppa), Uppas cache (~/.uppa)
# och skapade skript (~/Documents/Muppa, ~/Documents/Puppa).
# RUPPA_YES=1 hoppar över frågan. RUPPA_HOME om Ruppa installerades någon annanstans.
set -eu
# Language: TRUPPA_LANG, else ~/.truppa/config.json (shared by the whole troupe), else English
TL="${TRUPPA_LANG:-}"
if [ -z "$TL" ] && [ -f "$HOME/.truppa/config.json" ]; then
  TL="$(sed -n 's/.*"lang"[^"]*"\([a-z]*\)".*/\1/p' "$HOME/.truppa/config.json" 2>/dev/null | head -n 1)"
fi
t() { case "$TL" in sv*) printf '%s' "$2" ;; *) printf '%s' "$1" ;; esac; }
START="$(date +%s)"

DIR="${RUPPA_HOME:-$HOME/.ruppa}"
APP="$HOME/Applications/Ruppa.app"
DESKTOP="$HOME/.local/share/applications/ruppa.desktop"

say()  { printf '  \033[38;5;202mR›\033[0m %s\n' "$1"; }
die()  { printf '  \033[31m✘\033[0m %s\n' "$1" >&2; exit 1; }

# säkerhetsspärr: radera bara en mapp som ser ut som Ruppas
if [ -e "$DIR" ] && [ ! -e "$DIR/bin/ruppa" ] && [ "$(basename "$DIR")" != ".ruppa" ]; then
  die "$(t "$DIR does not look like a Ruppa folder. Nothing has been removed." "$DIR ser inte ut som en Ruppa-mapp. Ingenting har tagits bort.")"
fi

FOUND=""
[ -e "$DIR" ] && FOUND="$FOUND\n    $DIR"
[ -e "$APP" ] && FOUND="$FOUND\n    $APP"
[ -e "$DESKTOP" ] && FOUND="$FOUND\n    $DESKTOP"
RCS=""
for rc in "$HOME/.zshrc" "$HOME/.bashrc" "$HOME/.bash_profile" "$HOME/.profile"; do
  if [ -f "$rc" ] && grep -qsF "$DIR/bin" "$rc"; then RCS="$RCS $rc"; FOUND="$FOUND\n    PATH-raden i $rc"; fi
done
if [ -z "$FOUND" ]; then
  say "$(t "Ruppa is not installed here. Nothing to remove." "Ruppa är inte installerad här. Inget att ta bort.")"
  exit 0
fi

printf "$(t '\n  This is removed:' '\n  Det här tas bort:')"
printf "$FOUND\n"
printf "$(t '  Your data in ~/.puppa, ~/.muppa, ~/.euppa, ~/.uppa and ~/Documents is kept.\n\n' '  Dina data i ~/.puppa, ~/.muppa, ~/.euppa, ~/.uppa och ~/Documents behålls.\n\n')"
if [ "${RUPPA_YES:-}" != "1" ]; then
  if [ -r /dev/tty ]; then
    printf "$(t '  Uninstall Ruppa? [y/N] ' '  Avinstallera Ruppa? [j/N] ')"
    { read -r SVAR < /dev/tty; } 2>/dev/null || SVAR=""
  else
    die "$(t "Run with RUPPA_YES=1 to uninstall without asking." "Kör med RUPPA_YES=1 för att avinstallera utan fråga.")"
  fi
  case "$SVAR" in j|J|ja|Ja|y|Y|yes) ;; *) say "$(t "Cancelled. Nothing has been removed." "Avbrutet. Ingenting har tagits bort.")"; exit 0 ;; esac
fi

# stäng ett öppet Ruppa-fönster
pkill -f "$DIR/bin/ruppa" 2>/dev/null || true

rm -rf "$DIR"
[ -e "$APP" ] && rm -rf "$APP"
[ -e "$DESKTOP" ] && rm -f "$DESKTOP"
for rc in $RCS; do
  # ta bort bara våra två rader: "# Ruppa" och export-raden som pekar på ~/.ruppa/bin (en kopia sparas som .bak)
  cp "$rc" "$rc.ruppa.bak"
  tmp="$(mktemp)"
  grep -vF "$DIR/bin" "$rc" | grep -vx '# Ruppa' > "$tmp" || true
  cat "$tmp" > "$rc"
  rm -f "$tmp"
done

SECS=$(( $(date +%s) - START ))
printf "$(t '\n  \033[38;5;42m✔\033[0m Ruppa is uninstalled (%s s). Open a new terminal and the PATH line is gone.\n\n' '\n  \033[38;5;42m✔\033[0m Ruppa är avinstallerad (%s s). Öppna en ny terminal så försvinner PATH-raden.\n\n')" "$SECS"
