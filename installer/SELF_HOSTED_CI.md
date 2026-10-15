# CI sur MINIPC-CLEM

Le workflow Rust Build utilise uniquement MINIPC-CLEM. Les jobs Linux de tests,
style et publication exigent `self-hosted,linux,x64,minipc,podman`. Chaque job
Windows, y compris une PR de fork ou un tag, exige une VM KVM neuve et les labels
`self-hosted,windows,x64,minipc,ephemeral,sunshift`. PC-PRO-CLEM, PC-FIXE-CLEM et
les runners Windows hébergés GitHub ne sont pas des cibles de repli. Les tâches
Dependabot font exception et gardent leur service actuel ; cette migration ne
modifie ni leurs réglages ni leurs jobs dynamiques.

## Environnement Windows

La création de la VM et le cycle du runner relèvent de l'infrastructure MINIPC.
Ces changements de workflow ne les installent ni ne les enregistrent. Ne pas
attribuer ces labels à une machine persistante : ils ne garantissent pas
l'isolation. La VM doit être détruite après réussite, échec ou annulation, sans
secrets hôte, socket de conteneurs rootful, dossiers personnels ni accès au
réseau domestique. L'enveloppe CPU et mémoire CI reste partagée avec Linux.

Le guest requiert Git, Windows PowerShell et un bureau Windows interactif.
Le workflow prépare Rust stable GNU et Clippy dans son cache local, puis vérifie
ou télécharge MinGW portable (`gcc`, `dlltool`, `windres`) et NSIS (`makensis`).
Les archives sont figées et vérifiées par SHA-256. Cette chaîne GNU n'exige ni
MSVC Build Tools, ni CMake, ni Chocolatey. Les outils restent dans la VM jetable. Les dépendances Cargo téléchargées et
compilées (debug et release) sont sauvegardées dans le cache GitHub Actions par
`Swatinem/rust-cache`, puis restaurées dans chaque nouvelle VM. Aucun disque
écrivable de VM n'est partagé entre jobs. Le cache exclut les exécutables de
`CARGO_HOME/bin` ; Rust est toujours préparé et vérifié avant la restauration.

La clé distingue Windows GNU et la version MinGW, puis inclut automatiquement
la version de Rust, les manifestes et `Cargo.lock`. Le dossier cible situé dans
`RUNNER_TOOL_CACHE` est explicitement déclaré relativement au checkout. Les
règles de portée GitHub isolent les caches des PR ; une PR ne peut pas remplacer
le cache de la branche principale. Les dépendances sont sauvegardées uniquement après réussite des validations :
un échec doit publier ses diagnostics sans attendre la compression du cache.
Le premier passage reste une compilation à froid ;
les suivants peuvent réutiliser les dépendances, mais recompilent le projet et
refont l'édition de liens. La validation native doit confirmer la sauvegarde,
puis une restauration lors d'un job suivant.

La préparation approuvée fournit Vulkan SwiftShader et OpenGL ES via EGL/ANGLE logiciel
sur CPU. Les deux passes GUI natives demandent explicitement Vulkan et OpenGL,
vérifient le backend réellement sélectionné et refusent un repli vers l'autre
API. La capture Vulkan utilise la relecture GPU ; la capture OpenGL sous
egui-wgpu 0.23 lit les pixels de la fenêtre Windows via GDI. Chaque page et
la fenêtre rouverte doivent produire une image non uniforme. Le rendu logiciel
ne prouve pas le support d'un GPU ou moniteur physique.
La passe OpenGL conserve les DLL ANGLE/SwiftShader 137 ensemble et copie aussi
le chargeur vérifié `vulkan-1.dll` dans son dossier privé. Son manifeste
`installer/vk_swiftshader_icd.json` provient de Chrome for Testing
137.0.7151.119 (archive SHA-256
`c1ff403bfd791544fd880f67daa81bfaa022c2e3fdbc0ad68605458b5a2c0845`) ;
il désigne la DLL GLES locale, sans emprunter celle de la passe Vulkan.
Les essais GUI et installateur exigent toujours Windows ; une compilation
croisée ne les remplace pas.

Un runner démarré comme service utilise une tâche planifiée temporaire pour
lancer la fixture GUI dans la session console connectée. Cela exige le droit
d'enregistrer cette tâche ; elle est supprimée après l'essai et ne reçoit aucun
identifiant du runner. Le guest doit fournir cette session interactive jetable.
Aucun bureau personnel n'est utilisé.

## Validation manuelle

Après publication du workflow migré, choisir une branche qui le contient :

```powershell
$branch = 'master'
# Windows : tests, Clippy, build release, NSIS et essais GUI/installateur natifs.
gh workflow run rust.yml --repo Rhadamanthe0/sunshift-brightness --ref $branch -f platform=windows
# Linux : tests, Clippy, syntaxe du script, format et tri du manifeste.
gh workflow run rust.yml --repo Rhadamanthe0/sunshift-brightness --ref $branch -f platform=linux
# Les deux plateformes, exclusivement sur MINIPC-CLEM.
gh workflow run rust.yml --repo Rhadamanthe0/sunshift-brightness --ref $branch -f platform=all
# Seulement avec un moniteur physique compatible réellement accessible au guest.
gh workflow run rust.yml --repo Rhadamanthe0/sunshift-brightness --ref $branch -f platform=windows -f physical_monitor_check=true
```

**Actions → Rust Build → Run workflow** propose la plateforme et le contrôle
physique optionnel. Le choix du runner n'est plus un paramètre. Les jobs Windows
restent en attente tant qu'un runner ne possède pas les six labels et que le
cycle de VM n'est pas préparé ; cette attente ne prouve pas que la VM existe.

## Contrôles natifs et publication

La fixture GUI rend toutes les pages et le graphe solaire, crée le tray puis
ferme et rouvre sa fenêtre, pour chacune des deux API demandées. Elle garde sa
configuration en mémoire avec tous les moniteurs désactivés, ignore les clics
et ne charge ni n'enregistre les fichiers applicatifs. `cargo test` ordinaire
ignore cet essai ; le workflow l'appelle explicitement.

La fixture NSIS effectue deux mises à niveau, vérifie la conservation de sa
configuration, arrête son processus CLI verrouillé, compare les binaires, vérifie
démarrage automatique et raccourcis, puis désinstalle. Fichiers, processus, clés
de registre et raccourcis portent des identifiants uniques. Aucune application
n'est installée pour un usage personnel.

Sur tag uniquement, après réussite de ces contrôles Windows, les mêmes trois
exécutables (`ssb.exe`, `ssb-cli.exe`, `ssb-installer.exe`) sont envoyés comme
artefact `windows-release`. Un job Linux distinct télécharge l'artefact de cette
exécution et publie la release. Seul ce job possède `contents: write` ; les jobs
de compilation et tests gardent `contents: read`, avec
`persist-credentials: false` pour checkout. Ce guide n'autorise pas la création
d'un tag ou d'une release comme simple essai.

## Contrôle du moniteur physique

`physical_monitor_check=true` conserve le cycle natif existant : lire la
luminosité actuelle, réécrire ces mêmes valeurs, puis les relire. Les écrans
internes utilisent l'interface Windows IOCTL de l'application et conservent les
valeurs secteur et batterie ; les écrans externes exigent DDC/CI. Au moins un
moniteur physique compatible doit réussir, sinon le contrôle échoue. Un écran
virtuel ne prouve pas cette capacité. Les passages ordinaires ne font
qu'énumérer les moniteurs.

En cas d'échec, lire la description du moniteur physique et l'erreur Windows.
Pour un écran externe, DDC/CI doit être activé avec une connexion compatible ;
un moniteur détecté ne prouve pas le support DDC/CI. Connexion, verrouillage et
déverrouillage réels, ainsi que débranchement/rebranchement physique du moniteur,
exigent encore une séance interactive distincte.
