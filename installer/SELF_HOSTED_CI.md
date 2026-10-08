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
3. The runner account needs Git. The workflow prepares its own Rust stable GNU
   toolchain and Clippy under the runner tool cache, using the official Rustup
   download with its SHA-256 checksum. Existing personal Rust installations and
   the system PATH are left unchanged. For the GNU linker, it checks `C:\Strawberry\c\bin` and
   `C:\msys64\mingw64\bin` if MinGW is not on PATH.
   If absent, it downloads a pinned portable MinGW build into the runner's
   tool cache, verifies its SHA-256 checksum and adds it to the job's
   PATH. It does not change the system PATH or require administrator rights.
   NSIS can be on PATH or installed in its
   standard `Program Files (x86)\NSIS` directory. Otherwise the installer step
   downloads and verifies a pinned portable NSIS build in the runner's tool
   cache. Completed tool extractions are reused by later jobs. Only the installer
   compiler builds the normal installer; a separate temporary fixture tests
   installation and uninstallation as described below.
   Cargo build artifacts also stay in the local tool cache, outside the checkout,
   so later jobs reuse them without archiving or uploading them to GitHub.
4. Start `.\run.cmd` from the runner directory and leave that terminal open.
   The PC must be awake and a user logged into its desktop. The Windows job
   opens a temporary GUI and tray icon to check rendering and window reopening.
   Vulkan or OpenGL is required. If the runner is a service, the GUI helper
   uses a temporary scheduled task to run the test in the logged-in console
   user's session. This requires task registration rights; otherwise start the
   runner directly in that user's session. The temporary task is removed after
   the test, and no credentials are passed to it.

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

# Also validate DDC on a connected compatible monitor, keeping its current value.
gh workflow run rust.yml --repo Rhadamanthe0/sunshift-brightness --ref audit/2026-10-08 -f platform=all -f windows_runner=self-hosted -f physical_monitor_check=true
```

Use the branch containing the changes you want to validate. Once the workflow
is on the default branch, **Actions → Rust Build → Run workflow** provides the
same platform and runner selections in the GitHub UI. Keep both machines
available for automatic push/PR runs.

A Windows job stays queued until an online runner has all three labels:
`self-hosted`, `windows`, `x64`.

The Windows PC job also renders all GUI pages, including the solar preview,
and closes/reopens its temporary window. It uses an in-memory configuration
with every monitor disabled and does not load or save your application files.
Desktop input is discarded during rendering so a click cannot activate Save.
The GUI test is selected explicitly by the workflow; ordinary `cargo test`
skips it because it needs a Windows desktop with Vulkan or OpenGL.

The PC job also executes an isolated NSIS fixture: two upgrades preserve a test
configuration, replace a locked CLI file after stopping its test process, and
verify payloads, autostart command and shortcuts. It then uninstalls the fixture.
Its files and shortcuts are under `RUNNER_TEMP`, and its registry key is under
`HKCU\Software\SunshiftBrightness\CI\<GUID>` rather than the real autostart key.
Executable names also include the GUID. The fixture never starts the application and
does not touch your existing installation or its configuration.

The GUI helper also records desktop session IDs and the results of the GDI/DXVA2
monitor APIs. With `physical_monitor_check=true` on a self-hosted Windows run,
it reads a monitor's native brightness value, sends that exact value via DDC,
and reads it back. At least one compatible monitor must pass; an unavailable
monitor fails the check. Ordinary runs only enumerate monitors and do not send
brightness commands. Neither mode changes the application configuration.

If this check fails, inspect the physical monitor description and Windows error
in the job log. Enable DDC/CI in the monitor menu and check the connection;
detecting a physical monitor does not establish DDC/CI support. A failure such as
`0xC0262589` means Windows received an invalid DDC/CI response command. Test a
direct HDMI/DisplayPort connection if a dock, adapter or KVM is involved.

Real login/lock/unlock and physically disconnecting/reconnecting a monitor need
a separate interactive test session. The native tests exercise the Win32 message
handlers and registration, and the installer fixture verifies the autostart command.
