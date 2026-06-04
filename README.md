<!-- Developed by mahbub khan <mahbub.aumi@gmail.com> -->

# Postora

Postora is a native Fedora desktop utility for common post-install setup tasks.
It checks the system first, then lets you apply selected repository, codec,
driver, Flatpak, shell, font, and application changes through PolicyKit.

![Postora system setup screen](Screenshots/1.png)

## Download

The latest release package is included in this repository:

[postora-0.1.6-1.fc44.x86_64.rpm](postora-0.1.6-1.fc44.x86_64.rpm)

Install it on Fedora with:

```sh
sudo dnf install ./postora-0.1.6-1.fc44.x86_64.rpm
```

## Screenshots

![Postora authorization and logs](Screenshots/2.png)

![Postora result dialog](Screenshots/3.png)

![Postora tools and extras](Screenshots/4.png)

![Postora applications](Screenshots/5.png)

![Postora nerd fonts](Screenshots/6.png)

![Postora completed actions](Screenshots/7.png)

## Details

- Fedora-focused GTK4/libadwaita desktop app.
- System analysis runs before privileged changes.
- PolicyKit is used for installation tasks.
- Includes the latest RPM release file for direct installation.
