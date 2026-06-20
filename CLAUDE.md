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

## Build (IMPORTANT)

- Toolchain : **Rust stable msvc** (via rustup) + **Build Tools MSVC** (linker `link.exe` + `rc.exe`).
- **Ne jamais laisser `target/` dans le Drive.** Le FS virtuel Google Drive rejette certaines opérations de cargo (**os error 87**, « Paramètre incorrect ») ET synchroniser des Go d'artefacts gaspille la bande passante. La sortie est redirigée hors du Drive via `.cargo/config.toml` → `C:\Users\user\AppData\Local\ssb-target`. Ce fichier est **machine-spécifique** et **gitignoré** (ne pas committer).
- Compiler : à la racine du projet, `cargo build --release`. Binaires produits dans `…\AppData\Local\ssb-target\release\` : **`ssb.exe`** (GUI + icône de barre des tâches) et **`ssb-cli.exe`**.
- `build.rs` intègre l'icône via `winres` ; rendu **tolérant** : avertit au lieu d'échouer si `rc.exe` manque (l'icône de l'exe est cosmétique).

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

Désinstaller : menu Démarrer → « Uninstall Custom Solar Screen Brightness » (ou `%LOCALAPPDATA%\solar-screen-brightness\uninstall.exe`) ; retire fichiers, raccourcis et clé de démarrage.
## Pièges

- Chemin du Drive avec **espace insécable** (U+00A0) : les outils à chemin absolu (Read/Edit) échouent ; passer par PowerShell + chemins relatifs (cf. CLAUDE.md racine du Drive).
- Pas de nom court 8.3 sur ce volume (impossible de contourner l'espace insécable ainsi).
- Ne pas committer `.cargo/`, `build_release.log`, ni `target/` (tous gitignorés).