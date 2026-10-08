# Run CI on your own machines

The Linux test and style jobs already use the server runner labelled
`self-hosted`, `linux`, `x64`, `minipc`, `podman`.

The Windows job uses a Windows PC by default, including on pushes and pull
requests. It does not require a GitHub-hosted Windows runner or its billing
allowance. A manual run can select `github-hosted` instead.

## Register the Windows runner once

1. Open this repository's [runner settings](https://github.com/Rhadamanthe0/sunshift-brightness/settings/actions/runners),
   choose **New self-hosted runner**, then **Windows / x64**.
2. Follow GitHub's download and configuration commands on the Windows PC.
   Keep the runner and its work directory on a local disk, outside Google Drive.
   Keep the default labels `self-hosted`, `windows`, `x64`.
   The registration token stays on that PC; do not put it in the repository.
3. The runner account needs Git, Rust Cargo and Clippy, a working Rust linker
   (MSVC Build Tools or the GNU/MinGW setup described in `CLAUDE.md`), and NSIS 3.
   For GNU Rust, the workflow also checks `C:\Strawberry\c\bin` and
   `C:\msys64\mingw64\bin` if MinGW is not on PATH.
   Run `cargo clippy --version` and `cargo build --locked` from a checkout to
   confirm the build tools work. NSIS can be on PATH or installed in its standard
   `Program Files (x86)\NSIS` directory.
4. Start `.\run.cmd` from the runner directory and leave that terminal open.
   The PC must be awake. For later GUI or physical-monitor checks, run the runner
   in the logged-in user's session rather than as a background Windows service.

GitHub's runner configuration requires repository administration rights.
Registering a runner does not install the brightness application or change
monitor settings.

## Start a manual validation

With GitHub CLI authenticated to this repository:

```powershell
# Windows PC: tests, Clippy, release build, and installer compilation.
gh workflow run rust.yml --repo Rhadamanthe0/sunshift-brightness --ref audit/2026-10-08 -f platform=windows -f windows_runner=self-hosted

# Linux server: tests, Clippy, script syntax, format and manifest ordering.
gh workflow run rust.yml --repo Rhadamanthe0/sunshift-brightness --ref audit/2026-10-08 -f platform=linux

# Both machines.
gh workflow run rust.yml --repo Rhadamanthe0/sunshift-brightness --ref audit/2026-10-08 -f platform=all -f windows_runner=self-hosted
```

Use the branch containing the changes you want to validate. Once the workflow
is on the default branch, **Actions → Rust Build → Run workflow** provides the
same platform and runner selections in the GitHub UI. Keep both machines
available for automatic push/PR runs.

A Windows job stays queued until an online runner has all three labels:
`self-hosted`, `windows`, `x64`.

These jobs compile and test software. They do not launch the GUI, install or
uninstall the application, or change physical monitor brightness. Those checks
need an attached compatible monitor and a separate interactive test session.
