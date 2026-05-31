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

Atomic Fedora variants such as Silverblue and Kinoite are detected and blocked because they require an rpm-ostree-specific workflow.

## Build

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

Create a source tarball named `postora-0.1.0.tar.gz`, then build the package with:

```sh
tar --exclude=target -czf ~/rpmbuild/SOURCES/postora-0.1.0.tar.gz --transform 's,^,postora-0.1.0/,' .
rpmbuild -ba packaging/rpm/postora.spec
```

The installed files include:

- `/usr/bin/postora`
- `/usr/libexec/postora-helper`
- `/usr/share/applications/io.github.mahbub_khan25.Postora.desktop`
- `/usr/share/polkit-1/actions/io.github.mahbub_khan25.Postora.policy`

For local development, set `FEDORA_POST_SETUP_HELPER` to a helper path before launching the GUI.
