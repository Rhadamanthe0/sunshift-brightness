# Solar Screen Brightness

[![Build status](https://github.com/Rhadamanthe0/custom-solar-screen-brightness/actions/workflows/rust.yml/badge.svg)](https://github.com/Rhadamanthe0/custom-solar-screen-brightness/actions)

Automatically and smoothly adjusts monitor brightness based on the sunrise/sunset times at your location.

> #### Fork — Sunrise/Sunset Advance
>
> This fork adds two settings, **Sunrise Advance** and **Sunset Advance** (minutes), that shift the
> morning and evening transitions independently relative to the actual solar event. A positive value
> starts the transition *earlier* than sunrise/sunset, a negative value *later*. Useful when your room
> gets light or dark before the sun actually rises or sets (e.g. an east-facing room that brightens
> early in the morning and dims early in the evening). When both are `0` the behaviour is identical to
> upstream.
>
> This is an **unofficial fork** of [Solar Screen Brightness](https://github.com/jacob-pro/solar-screen-brightness)
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

For Windows, you can download pre-compiled binaries from
[Releases](https://github.com/jacob-pro/solar-screen-brightness/releases).

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

To uninstall: Start Menu → *Uninstall Custom Solar Screen Brightness*.

To rebuild the installer from source (requires `makensis`):

```
makensis /DBINDIR=<dir with the .exe> /DICON=<icon.ico> /DOUTFILE=<output.exe> installer\windows.nsi
```

CI builds the installer on every push: it runs `choco install nsis`, then invokes `makensis` **from the `installer/` directory**, where the default relative paths (`..\target\release`, `..\assets`) resolve correctly and `ssb-installer.exe` is produced.

## Original project & licence

This repository is a fork of **[Solar Screen Brightness](https://github.com/jacob-pro/solar-screen-brightness)**, created by **Jacob Halsey** (<https://github.com/jacob-pro>). The original work — and the vast majority of this codebase — is his. This fork is distributed under the same **GNU LGPL-3.0** licence (see [LICENSE](LICENSE)); the only functional addition is the configurable **Sunrise/Sunset Advance** offsets. It is not affiliated with, nor endorsed by, the original author.

## Screenshots

![](./screenshots/status.png)

![](./screenshots/brightness.png)

![](./screenshots/location.png)
