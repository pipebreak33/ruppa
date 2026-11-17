# Ruppa

<p align="center">
  <img src="assets/ruppa.svg" alt="Ruppa: Hej! Jag är Ruppa." width="720">
</p>

[Wuppa](https://github.com/pipebreak33/wuppa) i Rust, som en riktig app. Ruppa samlar kusinerna
[Uppa](https://github.com/pipebreak33/uppa), [Puppa](https://github.com/pipebreak33/puppa) och
[Muppa](https://github.com/pipebreak33/muppa) i ett fönster, med knappar, listor och formulär i stället för kommandon.
Installera Ruppa så får du alla tre. Allt hamnar i en mapp: `~/.ruppa`.

<p align="center">
  <img src="assets/screens/over.png" alt="Översikten" width="860">
</p>

## Installera (eller uppdatera – kör samma rad igen)

**Windows** (PowerShell):

```powershell
irm https://raw.githubusercontent.com/pipebreak33/ruppa/main/install.ps1 | iex
```

**macOS / Linux:**

```bash
curl -fsSL https://raw.githubusercontent.com/pipebreak33/ruppa/main/install.sh | sh
```

Installern hämtar rätt binär för datorn från den senaste releasen, kontrollerar SHA-256, och installerar sedan
kusinerna. Ruppa dyker upp bland dina program med R-ikonen: `~/Applications/Ruppa.app` på macOS, Startmenyn på
Windows och programmenyn på Linux. Inga adminrättigheter behövs.

```
~/.ruppa/
  bin/      ruppa + startare för uppa, puppa och muppa (ligger i PATH)
  uppa/     Uppa
  puppa/    Puppa (med recepten)
  muppa/    Muppa
  share/    ikonen
```

Kusinerna är Python-program. Saknas Python 3.8+ installerar Ruppa det per användare med winget på Windows. På macOS
räcker `xcode-select --install`.

## Det här kan du göra

Växla mellan sidorna i menyn till vänster, eller med **⌘1–⌘4** (Ctrl+1–4 på Windows och Linux).

| Sida | |
|---|---|
| **Översikt** | Ruppa säger hej och berättar vad som saknas eller kan uppdateras. Installera, uppdatera eller öppna varje kusin med en knapp. |
| **Uppa** | Allt som kan uppdateras, grupperat per källa (Homebrew/winget, PowerShell-moduler, az, npm, .NET). Kryssa och klicka *Uppdatera valda*. Grundpaket (Bas, M365, Azure, Alla), *Logga ut från allt*, hälsokoll och städning. |
| **Puppa** | Välj tenant, sök bland recepten, fyll i fälten och klicka *Kör recept*. Resultatet visas som en tabell och sparas som CSV. Recept som ändrar något är orange och kräver att du bekräftar. |
| **Muppa** | Migreringsguiden som ett formulär: användare, mappar (chips), BitLocker, domän, webbläsare, lokalt admin-konto. *Skapa skript och checklista*, och checklistan visas direkt. |

<p align="center">
  <img src="assets/screens/puppa.png" alt="Puppa i Ruppa" width="430">
  <img src="assets/screens/muppa.png" alt="Muppa i Ruppa" width="430">
</p>

Behöver du terminalversionen av en kusin, till exempel för ett recept som frågar något, finns *Öppna i terminal*. Då
körs kusinen i en inbyggd terminal i fönstret. Den fortsätter köra när du byter sida, och *Tillbaka till appen* tar
dig tillbaka.

## Teknik

- **Rust** med **egui/eframe**: en enda binär utan WebView eller andra beroenden hos användaren. egui valdes framför
  Tauri för att slippa WebView2/WebKitGTK, och framför iced för att det är enkelt att rita egna saker, som de levande
  maskoterna.
- Systemets typsnitt (SF Pro på macOS, Segoe UI på Windows, Inter/Cantarell/Ubuntu på Linux).
- Kusinerna hämtas som arkiv från GitHub och packas upp i `~/.ruppa/<kusin>`. Den gamla versionen byts ut först när
  den nya har klarat en syntaxkontroll. `~/.wuppa/config.json` delas med Wuppa (`repos`, `local`), och
  `UPPA_REPO`/`PUPPA_BRANCH` osv. fungerar som vanligt.
- Sidorna anropar kusinernas egna kommandon (`uppa update`, `puppa kör`, `muppa skapa`) och läser deras data direkt
  (Uppas cache, Puppas recept och tenants).
- Den inbyggda terminalen: `portable-pty` (ConPTY på Windows) + `vt100`.
- Self-update från GitHub Releases med SHA-256-kontroll.

Ruppa har också ett litet kommandoradsgränssnitt för skript: `ruppa status`, `ruppa install`, `ruppa update`,
`ruppa uppa|puppa|muppa …` och `ruppa self-update`.

### Bygga själv

```bash
cargo build --release          # target/release/ruppa
cargo test && cargo clippy --all-targets -- -D warnings
python3 scripts/make_icon.py   # ritar om ikonen (assets/icon)
```

En tagg `v*` bygger macOS (universal), Linux (x86_64, arm64) och Windows (x86_64, arm64) och lägger dem i en release.

## Licens

MIT
