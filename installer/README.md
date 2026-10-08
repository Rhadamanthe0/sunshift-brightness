# Windows NSIS Installer

Requires NSIS 3 and the built `ssb.exe` and `ssb-cli.exe` binaries. From this
directory, run `makensis windows.nsi`, or run `make installer` from the repository
root. The script uses NSIS's bundled `nsExec`; no external process plugin is required.

Override the build inputs with `/DBINDIR=<binary directory>`, `/DICON=<icon.ico>`,
and `/DOUTFILE=<installer.exe>` on Windows (`-D` on Linux). Paths containing
spaces must be quoted. The installer runs without administrator rights, preserves
the current configuration on upgrade, and registers autostart for the current
user. Uninstalling removes application files, configuration, and logs from
LocalAppData. Legacy configurations in Roaming are left intact.

The Windows PC CI also runs `test_windows_installer.ps1`. It compiles the same
NSIS script with isolated names and tests upgrades, locked CLI replacement,
configuration preservation, the autostart command, shortcuts and uninstallation
in a temporary fixture. Its registry key and shortcut paths are isolated; it
never launches the installed application.
