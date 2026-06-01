Name:           postora
Version:        0.2.0
Release:        1%{?dist}
Summary:        GUI post-install setup assistant for Fedora

License:        MIT
URL:            https://github.com/mahbub-khan25/Postora
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust
BuildRequires:  gtk4-devel
BuildRequires:  libadwaita-devel
BuildRequires:  desktop-file-utils
BuildRequires:  libappstream-glib

Requires:       dnf
Requires:       polkit
Requires:       rpm
Requires:       curl

%description
Postora is a GTK4/libadwaita desktop app that analyzes a regular
Fedora system and offers optional setup tasks for RPM Fusion, Cisco OpenH264,
multimedia codecs, GPU acceleration packages, and Flathub. Privileged changes
are performed by a small helper through PolicyKit.

%prep
%autosetup

%build
cargo build --release --offline

%install
install -Dm0755 target/release/postora %{buildroot}%{_bindir}/postora
install -Dm0755 target/release/postora-helper %{buildroot}%{_libexecdir}/postora-helper
install -Dm0644 data/applications/io.github.mahbub_khan25.Postora.desktop %{buildroot}%{_datadir}/applications/io.github.mahbub_khan25.Postora.desktop
install -Dm0644 data/icons/hicolor/scalable/apps/io.github.mahbub_khan25.Postora.svg %{buildroot}%{_datadir}/icons/hicolor/scalable/apps/io.github.mahbub_khan25.Postora.svg
install -Dm0644 data/polkit-1/actions/io.github.mahbub_khan25.Postora.policy %{buildroot}%{_datadir}/polkit-1/actions/io.github.mahbub_khan25.Postora.policy
install -Dm0644 data/metainfo/io.github.mahbub_khan25.Postora.metainfo.xml %{buildroot}%{_metainfodir}/io.github.mahbub_khan25.Postora.metainfo.xml

desktop-file-validate %{buildroot}%{_datadir}/applications/io.github.mahbub_khan25.Postora.desktop
appstream-util validate-relax --nonet %{buildroot}%{_metainfodir}/io.github.mahbub_khan25.Postora.metainfo.xml

%check
cargo test --workspace --offline

%files
%license LICENSE
%doc README.md
%{_bindir}/postora
%{_libexecdir}/postora-helper
%{_datadir}/applications/io.github.mahbub_khan25.Postora.desktop
%{_datadir}/icons/hicolor/scalable/apps/io.github.mahbub_khan25.Postora.svg
%{_datadir}/polkit-1/actions/io.github.mahbub_khan25.Postora.policy
%{_metainfodir}/io.github.mahbub_khan25.Postora.metainfo.xml

%changelog
* Mon Jun 01 2026 Postora contributors <noreply@example.invalid> - 0.2.0-1
- Implement resizable Paned logs partition, horizontal demarcation line, full-UI sensitivity locking, and busy mouse cursor indicator

* Mon Jun 01 2026 Postora contributors <noreply@example.invalid> - 0.1.9-1
- Collapse logs by default, expand on apply/progress, and only show restart warning if updates or drivers were installed

* Mon Jun 01 2026 Postora contributors <noreply@example.invalid> - 0.1.8-1
- Fix Starship command syntax error and add restart/logout prompt

* Sun May 31 2026 Postora contributors <noreply@example.invalid> - 0.1.5-1
- Ensure Flathub remote is always enabled and not left disabled

* Sun May 31 2026 Postora contributors <noreply@example.invalid> - 0.1.4-1
- Include 19 Flatpak apps in different categories: Web Browsers, Development, Office, Media, Utilities

* Sun May 31 2026 Postora contributors <noreply@example.invalid> - 0.1.3-1
- Refresh installed state after apply and improve completion detection

* Sun May 31 2026 Postora contributors <noreply@example.invalid> - 0.1.2-1
- Add optional extra app, shell, prompt, and Nerd Font installs

* Sun May 31 2026 Postora contributors <noreply@example.invalid> - 0.1.1-1
- Improve scrolling and log layout

* Sun May 31 2026 Postora contributors <noreply@example.invalid> - 0.1.0-1
- Initial package
