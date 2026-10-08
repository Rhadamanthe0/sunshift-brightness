# Linux Guide

## Installation

Clone this fork (authenticate with GitHub if the repository is private), install
Rust from <https://rustup.rs>, and run the [installation script](./ubuntu_install.sh)
from the checkout on Ubuntu:

```bash
git clone https://github.com/Rhadamanthe0/sunshift-brightness.git
cd sunshift-brightness
bash linux/ubuntu_install.sh install
```

The script builds this checkout with its lockfile and installs both binaries in
`${CARGO_HOME:-$HOME/.cargo}/bin`. It creates desktop and autostart entries and
copies the icon locally. It needs sudo to install missing build packages:
`build-essential`, `pkg-config`, `libssl-dev`, and `libudev-dev`. GTK and libxdo
are not required by the StatusNotifier tray backend. The desktop session must
provide a D-Bus session bus and a StatusNotifier-compatible tray.

For a manual build, install these packages and run `cargo build --locked --release`.
The GUI and CLI are `target/release/ssb` and `target/release/ssb-cli`.

To uninstall, stop the application, then run `bash linux/ubuntu_install.sh uninstall`.
This removes the binaries, desktop entries, configuration, and application data.
`XDG_CONFIG_HOME` and `XDG_DATA_HOME` are respected when set.

## External Monitor Support

Internally `solar-screen-brightness` uses the [`brightness` crate](https://github.com/stephaneyfx/brightness).

On Linux, the `brightness` crate interacts with devices found at `/sys/class/backlight`.
The application watches backlight and DDC/CI device events. It also retries
within one minute when no devices are found or a device reports an error.

The [ddcci-backlight](https://gitlab.com/ddcci-driver-linux/ddcci-driver-linux) 
kernel driver is required to expose external monitors as backlight devices (via DDC/CI).

### Installing the Driver

On Ubuntu-like distributions you should be able to use APT to install:

```
sudo apt install ddcci-dkms
```

On RHEL-family distributions:

```
sudo yum install kernel-devel      # You need matching kernel headers installed
git clone https://gitlab.com/ddcci-driver-linux/ddcci-driver-linux.git
cd ddcci-driver-linux
sudo make install
sudo make load
```

If the driver was installed successfully and is working for your monitors, you should now 
be able to see the devices listed in both `/sys/bus/ddcci/devices` and `/sys/class/backlight`.

### Debugging the Driver

In one terminal run: `dmesg -wT | grep ddcci` to follow the logs.

Then reload the driver in debug mode:
```
cd ddcci-driver-linux
sudo make unload
sudo modprobe ddcci-backlight dyndbg
```

### Backlight Permissions

If you have `systemd`
[version 243 or later](https://github.com/systemd/systemd/blob/877aa0bdcc2900712b02dac90856f181b93c4e40/NEWS#L262), 
then the `brightness` crate will attempt to set the device brightness
using the `DBus` `SetBrightness()` call, which manages all the permissions for you.

However, on older versions which don't have this function, then `brightness` must write directly to the backlight file,
which will require you to set appropriate permissions. You can do this using `udev` rules, for example:

`/etc/udev/rules.d/backlight.rules`
```
RUN+="/bin/bash -c '/bin/chgrp video /sys/class/backlight/*/brightness'"
RUN+="/bin/bash -c '/bin/chmod g+w /sys/class/backlight/*/brightness'"
```

`sudo usermod -a -G video "$USER"` (requires logging out to take effect)

### Known Issues

- [Monitors connected via a USB-C dock, on Intel devices, require updating to the Linux Kernel 5.10 for DDC/CI to work](https://gitlab.freedesktop.org/drm/intel/-/issues/37).
- [Hot swapping monitors is not yet supported, you need to reload the kernel module](https://gitlab.com/ddcci-driver-linux/ddcci-driver-linux/-/issues/5)
