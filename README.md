# Solar Screen Brightness

[![Build status](https://github.com/Rhadamanthe0/sunshift-brightness/actions/workflows/rust.yml/badge.svg)](https://github.com/Rhadamanthe0/sunshift-brightness/actions)

Automatically and smoothly adjusts monitor brightness based on the sunrise/sunset times at your location.

> #### Fork — Sunrise/Sunset Advance
>
> This fork adds two settings, **Sunrise Advance** and **Sunset Advance** (minutes), that shift the
> morning and evening transitions independently relative to the actual solar event. A positive value
> starts the transition *earlier* than sunrise/sunset, a negative value *later*. Useful when your room
> gets light or dark before the sun actually rises or sets (e.g. an east-facing room that brightens
> early in the morning and dims early in the evening). When both are `0`, transitions remain centred
> on the actual solar events. If long transitions would overlap, their duration is shortened
> symmetrically to keep the brightness curve continuous.
>
> Sunshift Brightness is an **unofficial fork** of [Solar Screen Brightness](https://github.com/jacob-pro/solar-screen-brightness)
> by **Jacob Halsey** ([jacob-pro](https://github.com/jacob-pro)), licensed under **LGPL-3.0**. All credit for the
> original application goes to the original author; this fork only adds the Sunrise/Sunset Advance feature.

> #### New 2.0 Release! 
> 
> I am pleased to announce a new 2.0 version with a modernised UI, new features, and bug fixes!

## About

### What is this for?

Supports Windows and Linux computers. Recommended for desktop PCs where you don't have an ambient light sensor to
automatically adjust screen brightness.

### How is this different to [f.lux](https://justgetflux.com/) or similar Night Mode programs?

This changes the screen brightness via monitor control APIs, whereas those utilities vary the colour temperature.

### How to Install

For Windows, you can download pre-compiled binaries from this fork's
[Releases page](https://github.com/Rhadamanthe0/sunshift-brightness/releases).

If you are using Linux, please read the [Linux Guide](linux/README.md)

There is also a CLI only version of the application available.

### How to Use

1. An icon will appear in your tray when it is running.
2. Click on the icon to open the settings menu.
3. Use the menus to set:
    - Daytime and Nighttime brightness percentages.
    - Transition time (the time it takes to switch between the two brightness values at either sunset or sunrise).
    - Sunrise / Sunset Advance (minutes to shift each transition earlier (+) or later (-) than the actual solar event).
    - Your location (either manually enter coordinates, or using the search tool).
4. Click save and this configuration will be applied and persisted to disk.
5. You can close the window, and it will continue to update your brightness in the background.

## Installation (this fork)

A pre-built Windows installer (`ssb-installer.exe`, NSIS, no admin rights) is produced from `installer/windows.nsi`. Running it:

- copies `ssb.exe` + `ssb-cli.exe` into `%LOCALAPPDATA%\solar-screen-brightness\`;
- adds Start Menu shortcuts and an uninstaller;
- registers autostart at logon, minimised to tray, via `HKCU\…\CurrentVersion\Run` (`ssb.exe --minimised`).

First run: open the settings from the tray icon and set your **Location**, then Day/Night brightness, Transition, and the new **Sunrise/Sunset Advance**. Nothing is adjusted until a location is set.

For external monitors on Windows, enable **DDC/CI** in the monitor's own menu.
The monitor and connection must support DDC/CI brightness control; detecting a
physical monitor alone does not confirm this support. If brightness commands
fail, test a direct HDMI/DisplayPort connection: some docks, adapters and KVMs
do not pass DDC/CI correctly. The Status page reports the Windows error.

To uninstall: Start Menu → *Uninstall Custom Solar Screen Brightness*.

### Build from source

`cargo build --locked --release` produces `ssb` (tray GUI) and `ssb-cli` in
`target/release` (`ssb.exe` and `ssb-cli.exe` on Windows). For Linux build
dependencies, see the [Linux guide](linux/README.md). The app
icon is embedded into the executables at build time: with the **MSVC** toolchain
via `rc.exe`, and with the **GNU** toolchain via `windres` (the build script picks
the right one). If no resource compiler is available the build still succeeds —
only the file icon shown in Explorer is skipped (the tray icon is loaded at
runtime).

To rebuild the installer from source (requires `makensis`):

```
makensis /DBINDIR=<dir with the .exe> /DICON=<icon.ico> /DOUTFILE=<output.exe> installer\windows.nsi
```

Le workflow Rust Build s'exécute uniquement sur MINIPC-CLEM. Chaque job
Windows exige une VM neuve avec les labels
`self-hosted,windows,x64,minipc,ephemeral,sunshift` ; les jobs Linux utilisent
`self-hosted,linux,x64,minipc,podman`. Aucun de ces jobs ne se replie sur
PC-PRO-CLEM, PC-FIXE-CLEM ou un runner hébergé GitHub. Les tâches Dependabot
font exception à cette destination et gardent leur service actuel.

La VM doit être détruite après chaque job, y compris après échec ou annulation ;
les labels ne prouvent pas cette isolation. Le workflow prépare Rust/MinGW et
NSIS portables avec vérification des archives, puis lance `makensis` **depuis
le dossier `installer/`**, en passant la sortie réelle de Cargo par `/DBINDIR`.
Les changements de workflow n'installent ni la VM ni son runner.

Sur tag, après réussite des contrôles Windows, les mêmes `ssb.exe`,
`ssb-cli.exe` et `ssb-installer.exe` sont transmis à un job Linux de publication.
Seul ce job possède `contents: write` ; compilations et tests gardent
`contents: read`, et checkout ne conserve pas les identifiants GitHub.
Le lancement manuel choisit la plateforme et le contrôle physique optionnel ;
le choix du runner n'est plus un paramètre.

### Validation

Install the Rust `rustfmt` and `clippy` components and `cargo-sort`, then run
`make test` for formatting, manifest ordering, Clippy (including test targets),
and unit/doc tests. Run `cargo build --locked --release` to check the release build.
The CI validates Windows and Linux separately; a Linux build does not check
Windows event handling or rendering. With `cargo-audit` installed, `cargo audit`
checks the locked dependencies against RustSec. Informational dependency warnings
must be reviewed separately from confirmed vulnerabilities.

Le job Windows conserve compilation native, tests, Clippy, cycle NSIS
installation/mise à niveau/désinstallation, et cycle GUI/tray/fenêtre. Un bureau
Windows interactif est requis. La préparation approuvée utilise le rendu CPU
Vulkan SwiftShader et OpenGL ES via EGL/ANGLE logiciel ; elle ne prouve pas le support d'un
GPU ou d'un moniteur physique. Le workflow demande Vulkan et OpenGL dans deux
passes natives distinctes. Chaque test restreint l'API autorisée et vérifie le
backend réellement retenu ; il refuse un repli silencieux vers l'autre API.

La configuration du test GUI reste en mémoire avec tous les moniteurs
désactivés ; aucun fichier applicatif utilisateur n'est enregistré. La fixture
NSIS utilise des noms uniques de processus, clés de registre et raccourcis dans
un répertoire temporaire. Les détails sont dans le
[guide CI MINIPC](installer/SELF_HOSTED_CI.md).

Le paramètre optionnel `physical_monitor_check` exige un moniteur physique
compatible accessible au job Windows. Un écran virtuel ne prouve ni DDC/CI ni
le contrôle natif de luminosité. Sans ce matériel, ce contrôle reste non
vérifié ; il ne doit pas être remplacé ou annoncé réussi.

### CLI

Configure a location in the GUI first, then run `ssb-cli --once` to apply
brightness once and print the solar times, monitor results, and device errors as
JSON. `ssb-cli` without `--once` runs until Ctrl-C. Use `--config <path>` to load a
different configuration, or `--debug` for verbose logs. Only one GUI or CLI
instance can control brightness at a time. A configuration error exits with a
nonzero status; per-monitor failures are reported in the JSON and log.

## Original project & licence

This repository is a fork of **[Solar Screen Brightness](https://github.com/jacob-pro/solar-screen-brightness)**, created by **Jacob Halsey** (<https://github.com/jacob-pro>). The original work — and the vast majority of this codebase — is his. This fork is distributed under the same **GNU LGPL-3.0** licence (see [LICENSE](LICENSE)); the only functional addition is the configurable **Sunrise/Sunset Advance** offsets. It is not affiliated with, nor endorsed by, the original author.

## Screenshots

![](./screenshots/status.png)

![](./screenshots/brightness.png)

![](./screenshots/location.png)
