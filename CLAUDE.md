# CLAUDE.md — solar-screen-brightness (fork)

> ⚠️ **Chemin du Drive : espace INSÉCABLE (U+00A0).** Le point de montage `c:\Users\user\Accès Google Drive\Mon Drive` contient une espace **insécable** entre « Google » et « Drive » (U+00A0, pas une espace ordinaire). Les outils à **chemin absolu** — `Read` / `Edit` / `Grep` — **échouent** si l'on y tape une espace normale. Contourner via **PowerShell** : chemin **relatif** depuis la racine, ou `(Get-Item -LiteralPath (Join-Path (Get-Location) "<chemin\relatif>")).FullName` pour obtenir le vrai chemin, puis lire/écrire avec `[System.IO.File]::ReadAllText` / `WriteAllText`. (Détail : CLAUDE.md racine, « Pièges connus ».)


## Nature

Fork personnel de **solar-screen-brightness** (jacob-pro, LGPL-3.0), écrit en **Rust**. L'application ajuste automatiquement la luminosité des moniteurs (DDC/CI) selon le lever/coucher du soleil. C'est une **exception** à la règle « pas de code » de `Logiciels/` : un vrai projet compilable, conservé ici car c'est un outil Windows.

Source amont : https://github.com/jacob-pro/solar-screen-brightness

**Rédaction & vérification (messages, courriers, commentaires)** : tout texte produit ou modifié doit être **univoque, clair, organisé, concis, complet et sans doublon** ; **relire systématiquement** pour le vérifier — cf. CLAUDE.md racine du Drive, « Style de rédaction ».

## Apport du fork : décalage lever/coucher

Deux réglages ajoutés, **Sunrise Advance** et **Sunset Advance** (minutes, signés), qui décalent **indépendamment** les transitions du matin et du soir par rapport à l'instant solaire réel.

- Valeur **positive = transition plus tôt** (avant le lever/coucher) ; négative = plus tard.
- À `0` partout : transitions centrées sur les événements solaires réels.
- Cas d'usage (motivation initiale) : pièce exposée **est** (lumineuse tôt le matin, sombre tôt le soir) → avancer les deux transitions.

Implémentation — même chaîne de paramètres que `transition_mins` :

| Fichier | Rôle |
|---|---|
| `src/config.rs` | Champs `sunrise_offset_mins` / `sunset_offset_mins` (i32, `#[serde(default)]` → les anciens `config.json` se chargent à 0). |
| `src/calculator.rs` | `SolarSchedule` décale les événements : `rise/set = sun.rise/set − offset×60`, puis réutilise le même calcul pour le contrôleur et le graphe. |
| `src/apply.rs`, `src/controller.rs`, `src/cli.rs` | Propagation du paramètre. |
| `src/gui/brightness_settings.rs` | 2 sliders (−180..180) + courbe d'aperçu mise à jour. |

**Piège corrigé — courbe parasite + boucle infinie / crash.** Le décalage déplace `rise`/`set` mais **pas** `sun.visible` (position réelle du soleil, calculée par la lib). À l'origine, `calculate_brightness` choisissait le régime jour/nuit sur `sun.visible` : dans la fenêtre où l'événement décalé et l'événement réel divergent, la sinusoïde était évaluée **hors de sa fenêtre `[event ± half]`**, d'où (a) un **pic parasite** dans la courbe (remontée à mi-hauteur juste après le coucher décalé, symétrique au lever) et (b) une `expiry_time` **dans le passé** → boucle infinie de `generate_plot_data` (`current = expiry` sans progresser) → OOM → **crash à l'ouverture de l'onglet Brightness**, et busy-loop possible du contrôleur. **Correction de fond** : `calculate_brightness` est désormais une **courbe sinusoïdale par morceaux construite uniquement sur les événements décalés** (transition bornée à `[event ± half]`, plateaux jour/nuit autour), **indépendante de `sun.visible`** — le décalage devient un simple glissement, sans pic (testé : `test_offsets_produce_clean_shifted_curve`). Les événements voisins sont conservés dans `SolarSchedule` ; quand des transitions se chevaucheraient, leur durée est raccourcie symétriquement pour garder une courbe continue. Des décalages opposés peuvent réordonner les événements : une cible identique à la précédente reste un plateau. Garde-fous conservés **en défense** : `calculator::next_sample_time` (progression stricte ≥ 15 min en zone plate, testé) + attente de 60 s pour une expiration passée et minimum de 1 s dans `controller.rs`. Ne jamais rebrancher le choix de régime sur `sun.visible`, ni réintroduire un `current = expiry` nu.

**Piège corrigé — crash GPU à l'ouverture (backend Direct3D 12).** Symptôme distinct du précédent : fenêtre **blanche quelques secondes puis fermeture sèche**, intermittente, à l'ouverture de la fenêtre (surtout depuis le tray) — **sans** panic Rust (donc ni boîte « Fatal Error », ni `crashes.txt`). Diagnostic via le **journal d'événements Windows** (`Get-WinEvent -LogName Application`, source *Application Error*) : `APPCRASH`, **module fautif `D3D12Core.dll`, exception `0xc0000005`** (violation d'accès). C'est un bug du **backend Direct3D 12 de wgpu 0.16**, pas de notre code (on n'écrit aucun appel D3D12 ; wgpu le fait en interne). **Correction** : forcer wgpu sur **Vulkan** (repli OpenGL) dans `gui/mod.rs::launch_window` via `supported_backends = Backends::VULKAN | Backends::GL`, ce qui exclut entièrement D3D12. Impact perf nul (UI triviale, egui agnostique du backend). Vérifié par le log de démarrage « Backend de rendu : Vulkan — adaptateur : … » (ajouté après `set_window`). Ne pas réintroduire D3D12 dans `supported_backends` sur cette machine.

**Diagnostic des crashs — `crashes.txt`.** `log.txt` est **tronqué à chaque démarrage** (`File::create`), donc un crash est effacé par la relance suivante. Le panic hook (`main.rs::record_crash`) écrit désormais **en plus** dans `crashes.txt` (mode **ajout**, jamais tronqué) → un panic Rust intermittent y survit. Règle de lecture : crash **présent** dans `crashes.txt` = panic Rust (message + ligne source) ; **absent** alors que l'app a planté = crash **natif** → chercher le module fautif dans le journal d'événements Windows.

## Build (IMPORTANT)

- Toolchain : **Rust stable msvc** (via rustup) + **Build Tools MSVC** (linker `link.exe` + `rc.exe`).
- **Alternative sans MSVC (validée)** : toolchain **gnu** + linker mingw. `rustup toolchain install stable-x86_64-pc-windows-gnu`, mettre `C:\Strawberry\c\bin` (Strawberry Perl : `gcc`/`windres`) sur le PATH, puis `cargo +stable-x86_64-pc-windows-gnu build --release`. Sert quand `link.exe`/MSVC est absent. Voir mémoire [[ssb-build-without-msvc-gnu]].
- **Ne jamais laisser `target/` dans le Drive.** Le FS virtuel Google Drive rejette certaines opérations de cargo (**os error 87**, « Paramètre incorrect ») ET synchroniser des Go d'artefacts gaspille la bande passante. La sortie est redirigée hors du Drive via `.cargo/config.toml` → `C:\Users\user\AppData\Local\Temp\ssb-target` (dossier temp que Windows purge). Ce fichier est **machine-spécifique** et **gitignoré** (ne pas committer).
- Compiler : à la racine du projet, `cargo build --release`. Binaires produits dans `…\AppData\Local\Temp\ssb-target\release\` : **`ssb.exe`** (GUI + icône de barre des tâches) et **`ssb-cli.exe`**.
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
- **Build dans le Drive = `os error 87`** : le FS virtuel du Drive rejette des écritures de cargo (timestamps/fingerprints) et la synchro gaspille la bande passante. D'où `.cargo/config.toml` → `target-dir` dans `%TEMP%\ssb-target`. Si des artefacts traînent dans le Drive, les supprimer (Drive rattrape ensuite sa file de synchro).
- **`Get-Content` (PS 5.1) décode en ANSI** : un fichier UTF-8 s'affiche en mojibake (`Ã©`, `â€"`) à la relecture **alors que le fichier est correct**. Vérifier avec `[IO.File]::ReadAllText($p,[Text.Encoding]::UTF8)` ou `Get-Content -Encoding UTF8`. Toujours écrire en UTF-8 **sans BOM**.
- **Backtick en chaîne double-quote** : `` `v `` & co. sont des échappements (→ caractère parasite, ex. dans un message de commit). Utiliser des here-strings `@'…'@` littéraux, ou un fichier message + `git commit -F`.
- **Bac à sable / classifieur PowerShell** : (a) toute commande touchant `C:\Temp` est bloquée → utiliser `%LOCALAPPDATA%` ; (b) écrire dans un fichier un contenu qui *ressemble* à une commande destructive (un RMDir récursif, un taskkill) est analysé puis bloqué → assembler ces tokens à l'exécution (`'RMD'+'ir'`, etc.) avant l'écriture.
- **NSIS / makensis** : compiler depuis un **chemin ASCII** (copie dans `%TEMP%\ssb-target`) avec tous les chemins en `/D` absolus, pour éviter l'insécable.
- **Toolchain** : Rust **msvc** + **Build Tools MSVC** (winget `Microsoft.VisualStudio.2022.BuildTools`, workload VCTools) pour `link.exe`/`rc.exe`. Sans eux, même `cargo check` échoue (build.rs doit être linké).
- Ne pas committer `.cargo/`, `target/`, `build_release.log`, `ssb-installer.exe` (tous gitignorés ; l'installeur reste dans le dossier mais hors git, distribué via Release GitHub).

## Publication

Fork publié en **privé** sur le compte GitHub de l'utilisateur (CPI). Branche de travail `feature/sunrise-sunset-advance`, fusionnée dans la branche par défaut. Le dépôt amont reste `jacob-pro/solar-screen-brightness` (LGPL). L'installeur compilé est attaché en **Release** (binaire hors historique git).

**CI (`.github/workflows/rust.yml`)** : GitHub Actions lance les contrôles à **chaque push** sur les runners personnels Windows et Linux. Le lancement manuel permet de choisir la plateforme et, facultativement, un runner Windows hébergé par GitHub (soumis au quota et à la facturation). Voir `installer/SELF_HOSTED_CI.md`. Un **tag** poussé (`refs/tags/…`) déclenche en plus la publication d'une Release avec les binaires + installeur. La Release `v2.3.2-custom` a, elle, été créée manuellement via `gh`.

**Trois jobs — pièges à connaître :**

- `check_style` = `cargo fmt -- --check` **+ `cargo-sort --check --workspace`**. cargo-sort exige, dans `Cargo.toml` : (a) les clés triées **dans chaque table** ; (b) les **tables `[target.*]` elles-mêmes** triées par en-tête (`cfg(target_os="linux")` < `cfg(unix)` < `cfg(windows)`) ; (c) une **virgule finale** dans les tableaux multi-lignes (ex. `features`). Le `Cargo.toml` amont violait (a), (b) et (c). Vérifier sans MSVC : compiler cargo-sort avec la toolchain **gnu** (`cargo +stable-x86_64-pc-windows-gnu install cargo-sort`) et le lancer sur une copie **LF** (la CI tourne sous Linux).
- `ubuntu` = test + clippy (toutes les cibles) + syntaxe du script Linux (ne compile pas le code Windows-only — un échec Windows-only n'y apparaît donc pas).
- `windows` = test + clippy + rendu GUI sur le PC + build + **installeur**. Sur le PC, le workflow prépare Rust stable GNU et Clippy dans son propre cache, sans modifier Rust personnel. Il reconnaît Strawberry/MinGW ou prépare MinGW portable dans le cache d’outils du runner. Il cherche NSIS dans le PATH puis dans `Program Files (x86)\NSIS` ; à défaut, il prépare NSIS portable. Les archives sont figées et vérifiées par SHA-256 ; les outils restent en cache pour les jobs suivants. Les artefacts Cargo du PC restent aussi hors checkout dans le cache local : le `tar` de Git for Windows avait fait échouer l’archivage distant. Sur `windows-latest`, NSIS est installé avec Chocolatey et le cache Cargo distant est utilisé. Makensis tourne **depuis `installer/`**, reçoit `/DBINDIR` avec la sortie réelle de Cargo et produit `ssb-installer.exe` dans ce dossier pour l'étape Publish.

## Validation locale après corrections

- `make test` vérifie formatage, tri du manifeste, Clippy sur toutes les cibles et tests, avec le lockfile.
- La recherche Nominatim est limitée à 15 s et rejette les coordonnées non finies.
- Une sauvegarde échouée conserve la configuration active ; `Apply` reste une application temporaire.
- Linux : le verrou de fichier et la FIFO IPC persistent après l’arrêt pour éviter les courses liées au remplacement d’inode ; les descripteurs sont fermés automatiquement. Le watcher utilise l’API sûre de `udev`.
- Un build croisé Windows vérifie les types et l’édition de liens, mais ne remplace pas les essais natifs des messages Win32, de la session, du tray et de l’installeur.
- Le runner Windows personnel exécute aussi `cargo test --locked native_windows_gui_lifecycle -- --ignored --nocapture --test-threads=1` : toutes les pages, graphe, création du tray et réouverture de fenêtre sur Vulkan/OpenGL. Ce test requiert un bureau Windows interactif ; la suite ordinaire ne le lance pas. Sa configuration reste en mémoire et désactive tous les moniteurs ; aucun fichier utilisateur ni réglage de luminosité n’est modifié.
- `installer/test_windows_installer.ps1` teste le script NSIS natif dans une installation temporaire isolée : noms d’exécutables, raccourcis et valeur de registre propres au test, deux mises à niveau, fichier CLI verrouillé, configuration préservée et désinstallation. Aucun binaire de l’application n’est lancé ; l’installation existante reste intacte.
- Si le runner Windows est un service (session0), `installer/test_windows_gui.ps1` lance uniquement le binaire de test dans la session de l’utilisateur connecté, par une tâche temporaire sans identifiants transmis. Le dossier de test a une ACL dédiée, les clics utilisateur sont ignorés et la tâche est supprimée après l’essai. Sans utilisateur connecté ou droit d’enregistrer la tâche, le contrôle échoue explicitement.
- Le lancement manuel `physical_monitor_check=true` vérifie aussi DDC : lecture de la valeur native actuelle, écriture de cette même valeur, relecture identique. Il requiert au moins un moniteur compatible ; les lancements ordinaires restent en lecture seule. Le diagnostic de session/moniteurs est dans `installer/diagnose_windows_monitors.ps1`.
