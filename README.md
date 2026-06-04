<!-- Developed by mahbub khan <mahbub.aumi@gmail.com> -->

# Postora

Postora is a native Rust GTK4/libadwaita desktop app for regular Fedora systems that use `dnf`.
It analyzes the host without elevation, then applies selected changes through a PolicyKit-authorized helper.

Supported v1 tasks:

- RPM Fusion free and nonfree release RPMs
- Fedora Cisco OpenH264 repository and OpenH264 packages
- RPM Fusion multimedia codecs
- NVIDIA driver packages when NVIDIA hardware is detected
- AMD and Intel media acceleration packages when matching hardware is detected
- Flathub remote setup
- Optional apps: Ghostty, VLC
- Optional shell setup: zsh as default shell and Starship with the Catppuccin Powerline preset
- Optional Nerd Fonts: FiraCode, JetBrainsMono, Hack, Meslo, CaskaydiaCove, SourceCodePro, UbuntuMono, RobotoMono, and Iosevka

Atomic Fedora variants such as Silverblue and Kinoite are detected and blocked because they require an rpm-ostree-specific workflow.

## Install

Normal users do not need Rust, Cargo, or development packages to run Postora.
Download the RPM from the GitHub release page, open it with GNOME Software or KDE Discover, and install it from there.

The RPM installs the app, desktop launcher, PolicyKit helper, icon, and metadata.
Runtime dependencies are handled by the package manager during installation.

## Build

These steps are only for developers who want to build Postora from source.
On Fedora, install build dependencies:

```sh
sudo dnf install cargo rust gtk4-devel libadwaita-devel desktop-file-utils libappstream-glib
```

Then run:

```sh
cargo fmt --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
```

## RPM

Create a source tarball named `postora-0.0.5.tar.gz`, then build the package with:

```sh
tar --exclude=target -czf ~/rpmbuild/SOURCES/postora-0.0.5.tar.gz --transform 's,^,postora-0.0.5/,' .
rpmbuild -ba packaging/rpm/postora.spec
```

The installed files include:

- `/usr/bin/postora`
- `/usr/libexec/postora-helper`
- `/usr/share/applications/io.github.mahbub_khan25.Postora.desktop`
- `/usr/share/polkit-1/actions/io.github.mahbub_khan25.Postora.policy`

For local development, set `POSTORA_HELPER` to a helper path before launching the GUI.
