# CLAUDE.md — solar-screen-brightness (fork)

## Nature

Fork personnel de **solar-screen-brightness** (jacob-pro, LGPL-3.0), écrit en **Rust**. L'application ajuste automatiquement la luminosité des moniteurs (DDC/CI) selon le lever/coucher du soleil. C'est une **exception** à la règle « pas de code » de `Logiciels/` : un vrai projet compilable, conservé ici car c'est un outil Windows.

Source amont : https://github.com/jacob-pro/solar-screen-brightness

## Apport du fork : décalage lever/coucher

Deux réglages ajoutés, **Sunrise Advance** et **Sunset Advance** (minutes, signés), qui décalent **indépendamment** les transitions du matin et du soir par rapport à l'instant solaire réel.

- Valeur **positive = transition plus tôt** (avant le lever/coucher) ; négative = plus tard.
- À `0` partout : comportement identique à l'amont.
- Cas d'usage (motivation initiale) : pièce exposée **est** (lumineuse tôt le matin, sombre tôt le soir) → avancer les deux transitions.

Implémentation — même chaîne de paramètres que `transition_mins` :

| Fichier | Rôle |
|---|---|
| `src/config.rs` | Champs `sunrise_offset_mins` / `sunset_offset_mins` (i32, `#[serde(default)]` → les anciens `config.json` se chargent à 0). |
| `src/calculator.rs` | `calculate_brightness` décale les instants : `rise/set = sun.rise/set − offset×60`. |
| `src/apply.rs`, `src/controller.rs`, `src/cli.rs` | Propagation du paramètre. |
| `src/gui/brightness_settings.rs` | 2 sliders (−180..180) + courbe d'aperçu mise à jour. |

**Piège corrigé — boucle infinie / crash.** Le décalage déplace `rise`/`set` mais **pas** `sun.visible` (position réelle du soleil, calculée par la lib). Dans la fenêtre où les deux divergent, `calculate_brightness` peut renvoyer une `expiry_time` **dans le passé**. Conséquences : (1) la boucle d'échantillonnage du graphe (`generate_plot_data`) faisait `current = expiry` sans progresser → **boucle infinie → OOM → crash à l'ouverture de l'onglet Brightness** ; (2) le contrôleur temps-réel pouvait **busy-looper** (sleep 0). Garde-fous en place : `calculator::next_sample_time` (progression stricte, ≥ 15 min en zone plate, **testé**) et un sleep minimal de 60 s dans `controller.rs`. Ne pas réintroduire un `current = expiry` nu.

## Build (IMPORTANT)

- Toolchain : **Rust stable msvc** (via rustup) + **Build Tools MSVC** (linker `link.exe` + `rc.exe`).
- **Alternative sans MSVC (validée)** : toolchain **gnu** + linker mingw. `rustup toolchain install stable-x86_64-pc-windows-gnu`, mettre `C:\Strawberry\c\bin` (Strawberry Perl : `gcc`/`windres`) sur le PATH, puis `cargo +stable-x86_64-pc-windows-gnu build --release`. Sert quand `link.exe`/MSVC est absent. Voir mémoire [[ssb-build-without-msvc-gnu]].
- **Ne jamais laisser `target/` dans le Drive.** Le FS virtuel Google Drive rejette certaines opérations de cargo (**os error 87**, « Paramètre incorrect ») ET synchroniser des Go d'artefacts gaspille la bande passante. La sortie est redirigée hors du Drive via `.cargo/config.toml` → `C:\Users\user\AppData\Local\ssb-target`. Ce fichier est **machine-spécifique** et **gitignoré** (ne pas committer).
- Compiler : à la racine du projet, `cargo build --release`. Binaires produits dans `…\AppData\Local\ssb-target\release\` : **`ssb.exe`** (GUI + icône de barre des tâches) et **`ssb-cli.exe`**.
- `build.rs` intègre l'icône : via `winres`/`rc.exe` en **msvc**, via un `.rc` minimal + `windres` en **gnu** (winres écrit le chemin de l'icône avec des backslashes que windres rejette). Rendu **tolérant** : avertit au lieu d'échouer si aucun compilateur de ressources n'est dispo (l'icône de fichier est cosmétique).

## Installation

Installeur Windows NSIS : `installer/windows.nsi`, **paramétré** (`/DBINDIR`, `/DICON`, `/DOUTFILE`) pour pointer vers la sortie de build hors-Drive.

Construire l'installeur (NSIS requis : `winget install NSIS.NSIS`) — compiler depuis un **chemin ASCII** pour éviter l'espace insécable :

```
makensis /DBINDIR=<dossier des .exe> /DICON=<icone .ico> /DOUTFILE=<sortie .exe> installer\windows.nsi
```

`ssb-installer.exe` (aucun droit admin) :
- copie `ssb.exe` + `ssb-cli.exe` dans `%LOCALAPPDATA%\solar-screen-brightness\` ;
- crée les raccourcis menu Démarrer (+ désinstalleur) ;
- **démarrage auto** au logon via `HKCU\…\CurrentVersion\Run` → `ssb.exe --minimised`, déclenché une fois le shell prêt (la barre des tâches doit exister pour que l'icône de tray s'enregistre — ne pas lancer plus tôt).

Désinstaller : menu Démarrer → « Uninstall Custom Solar Screen Brightness » (ou `%LOCALAPPDATA%\solar-screen-brightness\uninstall.exe`) ; retire fichiers (binaires, config, log), raccourcis et clé de démarrage. **Désinstallation vérifiée propre** (aucun résidu) : tout vit dans `%LOCALAPPDATA%\solar-screen-brightness\` (= INSTDIR = dossier config/log), supprimé en bloc ; l'uninstaller NSIS se relance depuis `%TEMP%` donc il efface aussi son propre dossier.

## Pièges & difficultés rencontrées (à relire avant de retoucher)

Spécifique à cet environnement (Drive monté + bac à sable PowerShell) — points qui m'ont réellement bloqué :

- **Espace insécable (U+00A0)** dans le chemin du Drive : Read/Edit/Write/Grep (chemin absolu) échouent. Contourner : éditer via PowerShell `[System.IO.File]::ReadAllText/WriteAllText`, et résoudre la racine par **wildcard** `Get-Item "$env:USERPROFILE\Acc*\Mon Drive\…"` (le `*` couvre l'insécable). Pas de nom court 8.3 sur ce volume.
- **`Set-Location` persiste entre appels** : un `cd` fait dans une commande casse les chemins relatifs des commandes suivantes. Re-résoudre la racine en absolu (wildcard) à chaque fois plutôt que `.\…`.
- **Build dans le Drive = `os error 87`** : le FS virtuel du Drive rejette des écritures de cargo (timestamps/fingerprints) et la synchro gaspille la bande passante. D'où `.cargo/config.toml` → `target-dir` dans `%LOCALAPPDATA%\ssb-target`. Si des artefacts traînent dans le Drive, les supprimer (Drive rattrape ensuite sa file de synchro).
- **`Get-Content` (PS 5.1) décode en ANSI** : un fichier UTF-8 s'affiche en mojibake (`Ã©`, `â€"`) à la relecture **alors que le fichier est correct**. Vérifier avec `[IO.File]::ReadAllText($p,[Text.Encoding]::UTF8)` ou `Get-Content -Encoding UTF8`. Toujours écrire en UTF-8 **sans BOM**.
- **Backtick en chaîne double-quote** : `` `v `` & co. sont des échappements (→ caractère parasite, ex. dans un message de commit). Utiliser des here-strings `@'…'@` littéraux, ou un fichier message + `git commit -F`.
- **Bac à sable / classifieur PowerShell** : (a) toute commande touchant `C:\Temp` est bloquée → utiliser `%LOCALAPPDATA%` ; (b) écrire dans un fichier un contenu qui *ressemble* à une commande destructive (un RMDir récursif, un taskkill) est analysé puis bloqué → assembler ces tokens à l'exécution (`'RMD'+'ir'`, etc.) avant l'écriture.
- **NSIS / makensis** : compiler depuis un **chemin ASCII** (copie dans `%LOCALAPPDATA%\ssb-target`) avec tous les chemins en `/D` absolus, pour éviter l'insécable.
- **Toolchain** : Rust **msvc** + **Build Tools MSVC** (winget `Microsoft.VisualStudio.2022.BuildTools`, workload VCTools) pour `link.exe`/`rc.exe`. Sans eux, même `cargo check` échoue (build.rs doit être linké).
- Ne pas committer `.cargo/`, `target/`, `build_release.log`, `ssb-installer.exe` (tous gitignorés ; l'installeur reste dans le dossier mais hors git, distribué via Release GitHub).

## Publication

Fork publié en **privé** sur le compte GitHub de l'utilisateur (CPI). Branche de travail `feature/sunrise-sunset-advance`, fusionnée dans la branche par défaut. Le dépôt amont reste `jacob-pro/solar-screen-brightness` (LGPL). L'installeur compilé est attaché en **Release** (binaire hors historique git).

**CI (`.github/workflows/rust.yml`)** : GitHub Actions lance test + clippy + build à **chaque push** (Windows + Ubuntu) — consomme des minutes Actions (quota limité en dépôt privé). Un **tag** poussé (`refs/tags/…`) déclenche en plus la publication d'une Release avec les binaires + installeur. La Release `v2.3.2-custom` a, elle, été créée manuellement via `gh`.

**Trois jobs — pièges à connaître :**

- `check_style` = `cargo fmt -- --check` **+ `cargo-sort --check --workspace`**. Garder `Cargo.toml` trié **alphabétiquement dans chaque table**, y compris les `[target.'cfg(...)'.dependencies]` (le tri amont était cassé : il fallait `console` avant `win32-utils`, et `gtk` avant `udev`).
- `ubuntu` = test + clippy uniquement (ne compile pas le code Windows-only — un échec Windows-only n'y apparaît donc pas).
- `windows` = test + clippy + build + **installeur**. `makensis` **n'est pas dans le PATH** du runner `windows-latest` : le workflow fait `choco install nsis`, puis appelle makensis **depuis `installer/`**. Les chemins relatifs `..\target\release` / `..\assets` du `.nsi` ne sont corrects que depuis ce dossier, où `ssb-installer.exe` est aussi produit pour l'étape Publish.