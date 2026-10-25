# CI sur les runners standards GitHub

Depuis le passage du depot en public, `Rust Build` cible `windows-2025`
pour Windows et `ubuntu-24.04` pour les tests Linux, le style et la publication.
Chaque job utilise une VM GitHub jetable. Aucun job du workflow courant ne
cible MINIPC-CLEM, PC-PRO-CLEM ou PC-FIXE-CLEM. Le nom historique de ce guide
est conserve pour ne pas casser les liens.

Les declenchements restent identiques : push sur `master`, PR, tags `v[0-9]*`
et lancement manuel. Une PR de fork s'execute sans secret personnel sur une
VM jetable ; aucun `pull_request_target` n'execute son code. Les compilations
et tests gardent `contents: read`, les checkouts `persist-credentials: false`.
Seul `publish`, apres reussite Windows et uniquement sur tag, a `contents: write`.
La migration ne cree aucun tag ni release de demonstration.

## Preparation reproductible

Linux installe explicitement `libudev-dev`, `libssl-dev`, `build-essential`
et `pkg-config` avec apt dans sa VM. Rust stable, Clippy, rustfmt, les commandes
Cargo verrouillees et le controle du manifeste sont conserves.

Windows conserve sa preparation Rust GNU et MinGW, le cache Cargo et NSIS,
ainsi que les controles existants de compression, installation, mise a niveau
et desinstallation. Les chemins de l'ancienne reference scellee sont des
optimisations facultatives : ils sont absents sur GitHub, et la preparation
existante telecharge alors ses outils verifies. Aucun fichier applicatif ni
comportement de luminosite n'est modifie par cette migration.

`prepare_github_graphics.ps1` recree la fixture graphique attendue dans
`C:\RhadaCI\graphics`, uniquement sur un runner GitHub Windows. Il telecharge
les archives Chrome for Testing listees dans `github-graphics.json`, verifie
leur SHA-256 puis celui de chacun des six fichiers extraits. Les versions et
empreintes sont celles de la fixture MINIPC existante : ANGLE/GLES 137.0.7151.119
et Vulkan/SwiftShader 155.0.8059.39. Chrome lui-meme n'est ni installe ni execute.
Les deux ensembles sont verifies avant publication de la fixture ; aucune
installation existante n'est remplacee, et les fichiers temporaires sont nettoyes.

`test_github_graphics.ps1` utilise de vraies archives factices et remplace
seulement le telechargement. Il controle l'extraction complete, le refus d'une
destination existante, le refus hors GitHub Windows, les empreintes d'archive
et de fichier incorrectes, les fichiers absents et les chemins interdits.

## Tests conserves

Les deux passes GUI natives demandent explicitement Vulkan et OpenGL et
verifient le backend effectivement utilise, sans repli vers l'autre API.
Leurs delais et assertions sont inchanges. Le test rend toutes les pages et
le graphe solaire, cree le tray, ferme et rouvre la fenetre ; les captures
restent obligatoirement non uniformes. La configuration de test reste en
memoire, avec les moniteurs desactives et sans fichier applicatif personnel.

La fixture NSIS garde ses deux mises a niveau, la conservation de configuration,
les comparaisons binaires, le controle du demarrage automatique et des
raccourcis, puis la desinstallation. Elle n'installe pas l'application pour un
usage personnel. Les essais GUI et installateur exigent Windows : une
compilation croisee ou un seul `cargo test` ne les remplace pas.

Le script GUI peut employer une tache temporaire dans la session interactive
si le runner fonctionne en session 0. Une absence de bureau interactif ou de
backend reste un echec, pas un test ignore. La compilation est preparee avant
la fenetre de temps du test GUI, comme precedemment.

## Limite physique explicite

Le rendu logiciel prouve le fonctionnement des deux API graphiques, pas celui
d'un GPU physique ni d'un moniteur DDC/CI. `physical_monitor_check` reste
optionnel et desactive par defaut. L'activer exige toujours au moins un ecran
physique compatible : lire la luminosite, reecrire les memes valeurs et les
relire. Un ecran virtuel GitHub ne satisfait pas ce contrat et le test echoue
au lieu de pretendre le contraire. Pour ce controle materiel, utiliser le
script local sur une machine et un ecran autorises ; ne pas reconnecter un
runner personnel aux PR publiques.

## Lancement et publication

Dans Actions > Rust Build > Run workflow, `platform` choisit `all`, `windows`
ou `linux`, pas le runner. Sur tag, les trois executables Windows valides
sont transmis au job Linux de publication sous forme d'artefact de la meme
execution. La migration n'affaiblit pas cette dependance.

Les anciennes branches, anciens tags et anciennes executions peuvent encore
contenir le workflow MINIPC. Ne pas les relancer comme preuve de cette migration.
Revoir les inscriptions de runners auto-heberges autorisees pour ce depot public :
changer `runs-on` ne constitue pas une restriction administrative des runners.
Les anciens journaux et l'historique Git doivent etre controles separement
pour les informations sensibles ; ce changement de CI n'est pas un audit
exhaustif de publication.
