use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;
use uuid::Uuid;

pub const MIN_SUPPORTED_FEDORA: u16 = 40;

#[derive(Debug, Error)]
pub enum PlannerError {
    #[error("this app supports Fedora only")]
    NonFedora,
    #[error("Fedora {0} is not supported; Fedora {MIN_SUPPORTED_FEDORA} or newer is required")]
    UnsupportedFedora(u16),
    #[error("rpm-ostree/Atomic Fedora systems are not supported by this version")]
    AtomicSystem,
    #[error("dnf is required but was not found")]
    MissingDnf,
    #[error("internet connectivity check failed")]
    Offline,
    #[error("selected action {0:?} is not available for this system")]
    UnavailableAction(ActionId),
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SystemInfo {
    pub os_id: String,
    pub os_name: String,
    pub fedora_version: Option<u16>,
    pub arch: String,
    pub is_atomic: bool,
    pub has_dnf: bool,
    pub has_internet: bool,
    pub secure_boot: SecureBootState,
    pub gpu_vendors: BTreeSet<GpuVendor>,
    pub installed_packages: BTreeSet<String>,
    pub enabled_repos: BTreeSet<String>,
    pub flatpak_remotes: BTreeSet<String>,
    pub flatpak_apps: BTreeSet<String>,
}

impl SystemInfo {
    pub fn validate_supported(&self) -> Result<u16, PlannerError> {
        if self.os_id != "fedora" {
            return Err(PlannerError::NonFedora);
        }
        if self.is_atomic {
            return Err(PlannerError::AtomicSystem);
        }
        if !self.has_dnf {
            return Err(PlannerError::MissingDnf);
        }
        if !self.has_internet {
            return Err(PlannerError::Offline);
        }
        let version = self.fedora_version.ok_or(PlannerError::NonFedora)?;
        if version < MIN_SUPPORTED_FEDORA {
            return Err(PlannerError::UnsupportedFedora(version));
        }
        Ok(version)
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SecureBootState {
    Enabled,
    Disabled,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

impl GpuVendor {
    pub fn from_pci_vendor_id(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "0x10de" | "10de" => Self::Nvidia,
            "0x1002" | "1002" | "0x1022" | "1022" => Self::Amd,
            "0x8086" | "8086" => Self::Intel,
            _ => Self::Other,
        }
    }

    pub fn from_lspci_line(line: &str) -> Option<Self> {
        let lower = line.to_ascii_lowercase();
        if !(lower.contains("vga") || lower.contains("3d controller") || lower.contains("display")) {
            return None;
        }
        if lower.contains("nvidia") {
            Some(Self::Nvidia)
        } else if lower.contains("amd") || lower.contains("ati") || lower.contains("advanced micro devices") {
            Some(Self::Amd)
        } else if lower.contains("intel") {
            Some(Self::Intel)
        } else {
            Some(Self::Other)
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum ActionId {
    SystemUpdate,
    RpmFusionFree,
    RpmFusionNonfree,
    CiscoOpenh264Repo,
    Openh264Packages,
    MultimediaCodecs,
    NvidiaDriver,
    AmdAcceleration,
    IntelAcceleration,
    Flathub,
    Ghostty,
    Zed,
    Vlc,
    ZshDefault,
    Starship,
    FontFiraCode,
    FontZedMono,
    FontJetBrainsMono,
    FontHack,
    FontMeslo,
    FontCaskaydiaCove,
    FontSourceCodePro,
    FontUbuntuMono,
    FontRobotoMono,
    FontIosevka,
    FlatpakChrome,
    FlatpakFirefox,
    FlatpakBrave,
    FlatpakZed,
    FlatpakPodmanDesktop,
    FlatpakDbeaver,
    FlatpakPostman,
    FlatpakOnlyOffice,
    FlatpakObsidian,
    FlatpakBitwarden,
    FlatpakVlc,
    FlatpakObsStudio,
    FlatpakGimp,
    FlatpakKdenlive,
    FlatpakLocalSend,
    FlatpakFlameshot,
    FlatpakFlatseal,
    FlatpakBottles,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionCategory {
    FedoraSetup,
    ExtraApps,
    NerdFonts,
    WebBrowsers,
    DevDatabase,
    OfficeProductivity,
    MediaCreative,
    UtilitiesTools,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Action {
    pub id: ActionId,
    pub category: ActionCategory,
    pub title: String,
    pub description: String,
    pub recommended: bool,
    pub selected_by_default: bool,
    pub already_complete: bool,
    pub warning: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Plan {
    pub plan_id: Uuid,
    pub fedora_version: u16,
    pub actions: Vec<Action>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplyRequest {
    pub plan_id: Uuid,
    pub selected_actions: BTreeSet<ActionId>,
    pub detected_fedora_version: u16,
    pub detected_gpu_vendors: BTreeSet<GpuVendor>,
    pub target_user: Option<String>,
    pub target_home: Option<String>,
    #[serde(default)]
    pub run_update: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

impl CommandSpec {
    pub fn new(program: impl Into<String>, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    pub fn display(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

pub fn build_plan(info: &SystemInfo) -> Result<Plan, PlannerError> {
    let version = info.validate_supported()?;
    let mut actions = Vec::new();

    actions.push(Action {
        id: ActionId::RpmFusionFree,
        category: ActionCategory::FedoraSetup,
        title: "RPM Fusion Free".into(),
        description: "Enable the RPM Fusion Free repository for Fedora-compatible packages.".into(),
        recommended: true,
        selected_by_default: !repo_enabled(info, "rpmfusion-free"),
        already_complete: repo_enabled(info, "rpmfusion-free"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::RpmFusionNonfree,
        category: ActionCategory::FedoraSetup,
        title: "RPM Fusion Nonfree".into(),
        description: "Enable the RPM Fusion Nonfree repository for codecs and vendor drivers.".into(),
        recommended: true,
        selected_by_default: !repo_enabled(info, "rpmfusion-nonfree"),
        already_complete: repo_enabled(info, "rpmfusion-nonfree"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::CiscoOpenh264Repo,
        category: ActionCategory::FedoraSetup,
        title: "Cisco OpenH264 repository".into(),
        description: "Enable Fedora's Cisco OpenH264 repository.".into(),
        recommended: true,
        selected_by_default: !repo_enabled(info, "fedora-cisco-openh264"),
        already_complete: repo_enabled(info, "fedora-cisco-openh264"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::Openh264Packages,
        category: ActionCategory::FedoraSetup,
        title: "OpenH264 packages".into(),
        description: "Install GStreamer and Firefox OpenH264 integration packages.".into(),
        recommended: true,
        selected_by_default: !packages_installed(info, &["gstreamer1-plugin-openh264", "mozilla-openh264"]),
        already_complete: packages_installed(info, &["gstreamer1-plugin-openh264", "mozilla-openh264"]),
        warning: None,
    });
    let multimedia_complete = info.installed_packages.contains("ffmpeg")
        && info.installed_packages.contains("gstreamer1-plugins-ugly");
    actions.push(Action {
        id: ActionId::MultimediaCodecs,
        category: ActionCategory::FedoraSetup,
        title: "Multimedia codecs".into(),
        description: "Install RPM Fusion multimedia packages and replace ffmpeg-free when needed.".into(),
        recommended: true,
        selected_by_default: !multimedia_complete,
        already_complete: multimedia_complete,
        warning: None,
    });

    if info.gpu_vendors.contains(&GpuVendor::Nvidia) {
        actions.push(Action {
            id: ActionId::NvidiaDriver,
            category: ActionCategory::FedoraSetup,
            title: "NVIDIA driver".into(),
            description: "Install RPM Fusion NVIDIA akmod driver and CUDA support package.".into(),
            recommended: true,
            selected_by_default: info.secure_boot != SecureBootState::Enabled
                && !info.installed_packages.contains("akmod-nvidia"),
            already_complete: info.installed_packages.contains("akmod-nvidia"),
            warning: (info.secure_boot == SecureBootState::Enabled).then(|| {
                "Secure Boot is enabled. NVIDIA kernel modules may require MOK enrollment and a reboot before they load.".into()
            }),
        });
    }
    if info.gpu_vendors.contains(&GpuVendor::Amd) {
        actions.push(Action {
            id: ActionId::AmdAcceleration,
            category: ActionCategory::FedoraSetup,
            title: "AMD media acceleration".into(),
            description: "Install RPM Fusion Mesa VA-API and VDPAU freeworld drivers.".into(),
            recommended: true,
            selected_by_default: !packages_installed(
                info,
                &["mesa-va-drivers-freeworld", "mesa-vdpau-drivers-freeworld"],
            ),
            already_complete: packages_installed(
                info,
                &["mesa-va-drivers-freeworld", "mesa-vdpau-drivers-freeworld"],
            ),
            warning: None,
        });
    }
    if info.gpu_vendors.contains(&GpuVendor::Intel) {
        actions.push(Action {
            id: ActionId::IntelAcceleration,
            category: ActionCategory::FedoraSetup,
            title: "Intel media acceleration".into(),
            description: "Install Intel media driver packages when available.".into(),
            recommended: true,
            selected_by_default: !info.installed_packages.contains("intel-media-driver"),
            already_complete: info.installed_packages.contains("intel-media-driver"),
            warning: None,
        });
    }
    actions.push(Action {
        id: ActionId::Flathub,
        category: ActionCategory::FedoraSetup,
        title: "Flathub".into(),
        description: "Install Flatpak if needed and add the Flathub remote.".into(),
        recommended: true,
        selected_by_default: !info.flatpak_remotes.contains("flathub"),
        already_complete: info.flatpak_remotes.contains("flathub"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::Ghostty,
        category: ActionCategory::ExtraApps,
        title: "Ghostty terminal".into(),
        description: "Enable the scottames/ghostty COPR repository and install Ghostty.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.installed_packages.contains("ghostty"),
        warning: Some("This enables a third-party COPR repository.".into()),
    });
    actions.push(Action {
        id: ActionId::Zed,
        category: ActionCategory::ExtraApps,
        title: "Zed editor".into(),
        description: "Install Zed for the current user using the official zed.dev install script.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: zed_installed(&current_home_dir()),
        warning: Some("This runs the official installer from zed.dev as your user.".into()),
    });
    actions.push(Action {
        id: ActionId::Vlc,
        category: ActionCategory::ExtraApps,
        title: "VLC media player".into(),
        description: "Install VLC. This usually requires RPM Fusion to be enabled first.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.installed_packages.contains("vlc"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::ZshDefault,
        category: ActionCategory::ExtraApps,
        title: "Zsh as default shell".into(),
        description: "Install zsh and set it as the default shell for the current user.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: default_shell_is_zsh(),
        warning: Some("You may need to log out and back in for the default shell change to apply.".into()),
    });
    actions.push(Action {
        id: ActionId::Starship,
        category: ActionCategory::ExtraApps,
        title: "Starship prompt".into(),
        description: "Install Starship, enable it for bash and zsh, and apply the Catppuccin Powerline preset.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: starship_configured(&current_home_dir()),
        warning: Some("This runs the official installer from starship.rs.".into()),
    });

    // Web Browsers
    actions.push(Action {
        id: ActionId::FlatpakChrome,
        category: ActionCategory::WebBrowsers,
        title: "Google Chrome".into(),
        description: "Install Google Chrome web browser.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("com.google.Chrome"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::FlatpakFirefox,
        category: ActionCategory::WebBrowsers,
        title: "Firefox (Flatpak)".into(),
        description: "Install Mozilla Firefox web browser.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("org.mozilla.firefox"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::FlatpakBrave,
        category: ActionCategory::WebBrowsers,
        title: "Brave Browser".into(),
        description: "Install Brave web browser.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("com.brave.Browser"),
        warning: None,
    });

    // Development & Database
    actions.push(Action {
        id: ActionId::FlatpakZed,
        category: ActionCategory::DevDatabase,
        title: "Zed IDE (Flatpak)".into(),
        description: "Install Zed developer IDE.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("dev.zed.Zed"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::FlatpakPodmanDesktop,
        category: ActionCategory::DevDatabase,
        title: "Podman Desktop".into(),
        description: "Install Podman Desktop container tool.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("io.podman_desktop.PodmanDesktop"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::FlatpakDbeaver,
        category: ActionCategory::DevDatabase,
        title: "DBeaver Community".into(),
        description: "Install DBeaver Community database manager.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("io.dbeaver.DBeaverCommunity"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::FlatpakPostman,
        category: ActionCategory::DevDatabase,
        title: "Postman".into(),
        description: "Install Postman API platform client.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("com.getpostman.Postman"),
        warning: None,
    });

    // Office & Productivity
    actions.push(Action {
        id: ActionId::FlatpakOnlyOffice,
        category: ActionCategory::OfficeProductivity,
        title: "OnlyOffice".into(),
        description: "Install OnlyOffice desktop editors.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("org.onlyoffice.desktopeditors"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::FlatpakObsidian,
        category: ActionCategory::OfficeProductivity,
        title: "Obsidian".into(),
        description: "Install Obsidian knowledge base app.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("md.obsidian.Obsidian"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::FlatpakBitwarden,
        category: ActionCategory::OfficeProductivity,
        title: "Bitwarden (Flatpak)".into(),
        description: "Install Bitwarden password manager desktop client.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("com.bitwarden.desktop"),
        warning: None,
    });

    // Media & Creative
    actions.push(Action {
        id: ActionId::FlatpakVlc,
        category: ActionCategory::MediaCreative,
        title: "VLC Media Player (Flatpak)".into(),
        description: "Install VLC media player.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("org.videolan.VLC"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::FlatpakObsStudio,
        category: ActionCategory::MediaCreative,
        title: "OBS Studio".into(),
        description: "Install OBS Studio for live streaming and recording.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("com.obsproject.Studio"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::FlatpakGimp,
        category: ActionCategory::MediaCreative,
        title: "GIMP (Flatpak)".into(),
        description: "Install GNU Image Manipulation Program.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("org.gimp.GIMP"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::FlatpakKdenlive,
        category: ActionCategory::MediaCreative,
        title: "Kdenlive (Flatpak)".into(),
        description: "Install Kdenlive video editor.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("org.kde.kdenlive"),
        warning: None,
    });

    // Utilities & Tools
    actions.push(Action {
        id: ActionId::FlatpakLocalSend,
        category: ActionCategory::UtilitiesTools,
        title: "LocalSend".into(),
        description: "Install LocalSend file sharing tool.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("org.localsend.localsend_app"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::FlatpakFlameshot,
        category: ActionCategory::UtilitiesTools,
        title: "Flameshot (Flatpak)".into(),
        description: "Install Flameshot screenshot tool.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("org.flameshot.Flameshot"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::FlatpakFlatseal,
        category: ActionCategory::UtilitiesTools,
        title: "Flatseal".into(),
        description: "Install Flatseal Flatpak permission manager.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("com.github.tchx84.Flatseal"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::FlatpakBottles,
        category: ActionCategory::UtilitiesTools,
        title: "Bottles".into(),
        description: "Install Bottles for running Windows software.".into(),
        recommended: false,
        selected_by_default: false,
        already_complete: info.flatpak_apps.contains("com.usebottles.bottles"),
        warning: None,
    });

    for font in nerd_fonts() {
        actions.push(Action {
            id: font.id,
            category: ActionCategory::NerdFonts,
            title: font.title.into(),
            description: format!("Install the {} Nerd Font from the Nerd Fonts v3.4.0 release.", font.title),
            recommended: false,
            selected_by_default: false,
            already_complete: Path::new(&format!("/usr/local/share/fonts/postora/{}", font.asset_slug)).exists(),
            warning: None,
        });
    }

    Ok(Plan {
        plan_id: Uuid::new_v4(),
        fedora_version: version,
        actions,
    })
}

pub fn commands_for_action(
    action: ActionId,
    version: u16,
    info: &SystemInfo,
) -> Result<Vec<CommandSpec>, PlannerError> {
    let mut commands = Vec::new();
    match action {
        ActionId::RpmFusionFree if !repo_enabled(info, "rpmfusion-free") => commands.push(CommandSpec::new(
            "dnf",
            [
                "install".into(),
                "-y".into(),
                rpmfusion_release_url("free", version),
            ],
        )),
        ActionId::RpmFusionNonfree if !repo_enabled(info, "rpmfusion-nonfree") => commands.push(CommandSpec::new(
            "dnf",
            [
                "install".into(),
                "-y".into(),
                rpmfusion_release_url("nonfree", version),
            ],
        )),
        ActionId::CiscoOpenh264Repo if !repo_enabled(info, "fedora-cisco-openh264") => {
            if version <= 40 {
                commands.push(CommandSpec::new(
                    "dnf",
                    ["config-manager", "--enable", "fedora-cisco-openh264"],
                ));
            } else {
                commands.push(CommandSpec::new(
                    "dnf",
                    ["config-manager", "setopt", "fedora-cisco-openh264.enabled=1"],
                ));
            }
        }
        ActionId::Openh264Packages
            if !packages_installed(info, &["gstreamer1-plugin-openh264", "mozilla-openh264"]) =>
        {
            commands.push(CommandSpec::new(
                "dnf",
                ["install", "-y", "gstreamer1-plugin-openh264", "mozilla-openh264"],
            ));
        }
        ActionId::MultimediaCodecs => {
            if !info.installed_packages.contains("ffmpeg") {
                if info.installed_packages.contains("ffmpeg-free") {
                    commands.push(CommandSpec::new(
                        "dnf",
                        ["swap", "-y", "ffmpeg-free", "ffmpeg", "--allowerasing"],
                    ));
                } else {
                    commands.push(CommandSpec::new("dnf", ["install", "-y", "ffmpeg", "--allowerasing"]));
                }
            }
            commands.push(CommandSpec::new(
                "dnf",
                [
                    "update",
                    "-y",
                    "@multimedia",
                    "--setopt=install_weak_deps=False",
                    "--exclude=PackageKit-gstreamer-plugin",
                ],
            ));
        }
        ActionId::NvidiaDriver if !info.installed_packages.contains("akmod-nvidia") => {
            if !info.gpu_vendors.contains(&GpuVendor::Nvidia) {
                return Err(PlannerError::UnavailableAction(action));
            }
            commands.push(CommandSpec::new(
                "dnf",
                ["install", "-y", "akmod-nvidia", "xorg-x11-drv-nvidia-cuda"],
            ));
        }
        ActionId::AmdAcceleration
            if !packages_installed(
                info,
                &["mesa-va-drivers-freeworld", "mesa-vdpau-drivers-freeworld"],
            ) =>
        {
            if !info.gpu_vendors.contains(&GpuVendor::Amd) {
                return Err(PlannerError::UnavailableAction(action));
            }
            commands.push(CommandSpec::new(
                "dnf",
                ["install", "-y", "mesa-va-drivers-freeworld", "mesa-vdpau-drivers-freeworld"],
            ));
        }
        ActionId::IntelAcceleration if !info.installed_packages.contains("intel-media-driver") => {
            if !info.gpu_vendors.contains(&GpuVendor::Intel) {
                return Err(PlannerError::UnavailableAction(action));
            }
            commands.push(CommandSpec::new(
                "dnf",
                ["install", "-y", "intel-media-driver", "libva-intel-driver"],
            ));
        }
        ActionId::Flathub => {
            if !info.installed_packages.contains("flatpak") {
                commands.push(CommandSpec::new("dnf", ["install", "-y", "flatpak"]));
            }
            commands.push(CommandSpec::new(
                "flatpak",
                [
                    "remote-add",
                    "--if-not-exists",
                    "flathub",
                    "https://dl.flathub.org/repo/flathub.flatpakrepo",
                ],
            ));
            commands.push(CommandSpec::new(
                "flatpak",
                ["remote-modify", "--enable", "flathub"],
            ));
        }
        ActionId::Ghostty if !info.installed_packages.contains("ghostty") => {
            commands.push(CommandSpec::new("dnf", ["install", "-y", "dnf-plugins-core"]));
            commands.push(CommandSpec::new("dnf", ["copr", "enable", "-y", "scottames/ghostty"]));
            commands.push(CommandSpec::new("dnf", ["install", "-y", "ghostty"]));
        }
        ActionId::Zed if !zed_installed(&current_home_dir()) => {
            commands.extend(user_shell_commands(
                info,
                "Install Zed",
                "curl -f https://zed.dev/install.sh | sh",
            ));
        }
        ActionId::Vlc if !info.installed_packages.contains("vlc") => {
            commands.push(CommandSpec::new("dnf", ["install", "-y", "vlc"]));
        }
        ActionId::ZshDefault if !default_shell_is_zsh() => {
            if !info.installed_packages.contains("zsh") {
                commands.push(CommandSpec::new("dnf", ["install", "-y", "zsh"]));
            }
            commands.push(CommandSpec::new(
                "sh",
                [
                    "-c",
                    r#"user="${POSTORA_TARGET_USER:-${SUDO_USER:-}}"; if [ -n "$user" ]; then chsh -s /usr/bin/zsh "$user"; else echo "No target user was provided for chsh" >&2; exit 1; fi"#,
                ],
            ));
        }
        ActionId::Starship if !starship_configured(&current_home_dir()) => {
            commands.push(CommandSpec::new(
                "sh",
                ["-c", "curl -sS https://starship.rs/install.sh | sh -s -- -y"],
            ));
            commands.extend(user_shell_commands(
                info,
                "Configure Starship",
                r##"mkdir -p "$HOME/.config"; /usr/local/bin/starship preset catppuccin-powerline -o "$HOME/.config/starship.toml"; touch "$HOME/.bashrc" "$HOME/.zshrc"; grep -q "starship init bash" "$HOME/.bashrc" 2>/dev/null || echo 'eval "$(starship init bash)"' >> "$HOME/.bashrc"; grep -q "starship init zsh" "$HOME/.zshrc" 2>/dev/null || echo 'eval "$(starship init zsh)"' >> "$HOME/.zshrc""##,
            ));
        }
        ActionId::FlatpakChrome => commands.extend(flatpak_install_commands("com.google.Chrome", info)),
        ActionId::FlatpakFirefox => commands.extend(flatpak_install_commands("org.mozilla.firefox", info)),
        ActionId::FlatpakBrave => commands.extend(flatpak_install_commands("com.brave.Browser", info)),
        ActionId::FlatpakZed => commands.extend(flatpak_install_commands("dev.zed.Zed", info)),
        ActionId::FlatpakPodmanDesktop => commands.extend(flatpak_install_commands("io.podman_desktop.PodmanDesktop", info)),
        ActionId::FlatpakDbeaver => commands.extend(flatpak_install_commands("io.dbeaver.DBeaverCommunity", info)),
        ActionId::FlatpakPostman => commands.extend(flatpak_install_commands("com.getpostman.Postman", info)),
        ActionId::FlatpakOnlyOffice => commands.extend(flatpak_install_commands("org.onlyoffice.desktopeditors", info)),
        ActionId::FlatpakObsidian => commands.extend(flatpak_install_commands("md.obsidian.Obsidian", info)),
        ActionId::FlatpakBitwarden => commands.extend(flatpak_install_commands("com.bitwarden.desktop", info)),
        ActionId::FlatpakVlc => commands.extend(flatpak_install_commands("org.videolan.VLC", info)),
        ActionId::FlatpakObsStudio => commands.extend(flatpak_install_commands("com.obsproject.Studio", info)),
        ActionId::FlatpakGimp => commands.extend(flatpak_install_commands("org.gimp.GIMP", info)),
        ActionId::FlatpakKdenlive => commands.extend(flatpak_install_commands("org.kde.kdenlive", info)),
        ActionId::FlatpakLocalSend => commands.extend(flatpak_install_commands("org.localsend.localsend_app", info)),
        ActionId::FlatpakFlameshot => commands.extend(flatpak_install_commands("org.flameshot.Flameshot", info)),
        ActionId::FlatpakFlatseal => commands.extend(flatpak_install_commands("com.github.tchx84.Flatseal", info)),
        ActionId::FlatpakBottles => commands.extend(flatpak_install_commands("com.usebottles.bottles", info)),
        action if nerd_font(action).is_some() => {
            let font = nerd_font(action).expect("font action exists");
            let destination = format!("/usr/local/share/fonts/postora/{}", font.asset_slug);
            let archive = format!("/tmp/postora-{}.zip", font.asset_slug);
            commands.push(CommandSpec::new("dnf", ["install", "-y", "curl", "unzip", "fontconfig"]));
            commands.push(CommandSpec::new(
                "curl",
                ["-fL", "-o", archive.as_str(), font.url],
            ));
            commands.push(CommandSpec::new("install", ["-d", "-m", "0755", destination.as_str()]));
            commands.push(CommandSpec::new("unzip", ["-o", archive.as_str(), "-d", destination.as_str()]));
            commands.push(CommandSpec::new("fc-cache", ["-f", destination.as_str()]));
        }
        _ => {}
    }
    Ok(commands)
}

#[derive(Clone, Copy, Debug)]
pub struct NerdFont {
    pub id: ActionId,
    pub title: &'static str,
    pub asset_slug: &'static str,
    pub url: &'static str,
}

pub fn nerd_fonts() -> &'static [NerdFont] {
    &[
        NerdFont { id: ActionId::FontFiraCode, title: "FiraCode", asset_slug: "FiraCode", url: "https://github.com/ryanoasis/nerd-fonts/releases/download/v3.4.0/FiraCode.zip" },
        NerdFont { id: ActionId::FontZedMono, title: "ZedMono", asset_slug: "ZedMono", url: "https://github.com/ryanoasis/nerd-fonts/releases/download/v3.4.0/ZedMono.zip" },
        NerdFont { id: ActionId::FontJetBrainsMono, title: "JetBrainsMono", asset_slug: "JetBrainsMono", url: "https://github.com/ryanoasis/nerd-fonts/releases/download/v3.4.0/JetBrainsMono.zip" },
        NerdFont { id: ActionId::FontHack, title: "Hack", asset_slug: "Hack", url: "https://github.com/ryanoasis/nerd-fonts/releases/download/v3.4.0/Hack.zip" },
        NerdFont { id: ActionId::FontMeslo, title: "Meslo", asset_slug: "Meslo", url: "https://github.com/ryanoasis/nerd-fonts/releases/download/v3.4.0/Meslo.zip" },
        NerdFont { id: ActionId::FontCaskaydiaCove, title: "CaskaydiaCove", asset_slug: "CaskaydiaCove", url: "https://github.com/ryanoasis/nerd-fonts/releases/download/v3.4.0/CaskaydiaCove.zip" },
        NerdFont { id: ActionId::FontSourceCodePro, title: "SourceCodePro", asset_slug: "SourceCodePro", url: "https://github.com/ryanoasis/nerd-fonts/releases/download/v3.4.0/SourceCodePro.zip" },
        NerdFont { id: ActionId::FontUbuntuMono, title: "UbuntuMono", asset_slug: "UbuntuMono", url: "https://github.com/ryanoasis/nerd-fonts/releases/download/v3.4.0/UbuntuMono.zip" },
        NerdFont { id: ActionId::FontRobotoMono, title: "RobotoMono", asset_slug: "RobotoMono", url: "https://github.com/ryanoasis/nerd-fonts/releases/download/v3.4.0/RobotoMono.zip" },
        NerdFont { id: ActionId::FontIosevka, title: "Iosevka", asset_slug: "Iosevka", url: "https://github.com/ryanoasis/nerd-fonts/releases/download/v3.4.0/Iosevka.zip" },
    ]
}

fn nerd_font(action: ActionId) -> Option<NerdFont> {
    nerd_fonts().iter().copied().find(|font| font.id == action)
}

fn current_home_dir() -> PathBuf {
    std::env::var_os("POSTORA_TARGET_HOME")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn current_user_name() -> Option<String> {
    std::env::var("POSTORA_TARGET_USER")
        .ok()
        .filter(|user| !user.is_empty())
        .or_else(|| std::env::var("USER").ok().filter(|user| !user.is_empty()))
        .or_else(|| std::env::var("SUDO_USER").ok().filter(|user| !user.is_empty()))
}

fn zed_installed(home: &Path) -> bool {
    [
        home.join(".local/bin/zed"),
        home.join(".local/zed.app/bin/zed"),
        home.join(".local/share/applications/dev.zed.Zed.desktop"),
        home.join(".local/share/zed"),
    ]
    .iter()
    .any(|path| path.exists())
}

fn starship_configured(home: &Path) -> bool {
    let binary = home.join(".cargo/bin/starship");
    let local_binary = home.join(".local/bin/starship");
    let usr_binary = Path::new("/usr/local/bin/starship");
    let config = home.join(".config/starship.toml");
    let bashrc = home.join(".bashrc");
    let zshrc = home.join(".zshrc");

    let configured_bash = file_contains(&bashrc, r#"eval "$(starship init bash)""#);
    let configured_zsh = file_contains(&zshrc, r#"eval "$(starship init zsh)""#);

    (binary.exists() || local_binary.exists() || usr_binary.exists())
        && config.exists()
        && configured_bash
        && configured_zsh
}

fn default_shell_is_zsh() -> bool {
    let Some(user) = current_user_name() else {
        return false;
    };
    let passwd_entry = command_output("getent", ["passwd", user.as_str()])
        .or_else(|| fs::read_to_string("/etc/passwd").ok().and_then(|contents| {
            contents
                .lines()
                .find(|line| line.split(':').next() == Some(user.as_str()))
                .map(ToOwned::to_owned)
        }));
    let Some(entry) = passwd_entry else {
        return false;
    };
    entry.split(':').nth(6).map(|shell| shell.ends_with("/zsh")).unwrap_or(false)
}

fn file_contains(path: &Path, needle: &str) -> bool {
    fs::read_to_string(path).map(|content| content.contains(needle)).unwrap_or(false)
}

fn user_shell_commands(_info: &SystemInfo, label: &str, script: &str) -> Vec<CommandSpec> {
    let Some(user) = info_target_user() else {
        return vec![CommandSpec::new(
            "sh",
            ["-c", &format!("echo 'No target user was provided for {label}' >&2; exit 1")],
        )];
    };
    let home = std::env::var("POSTORA_TARGET_HOME")
        .ok()
        .filter(|home| !home.is_empty())
        .unwrap_or_else(|| format!("/home/{user}"));
    vec![CommandSpec::new(
        "runuser",
        ["-u", user.as_str(), "--", "sh", "-lc", &format!("export HOME='{}'; {}", home.replace('\'', "'\\''"), script)],
    )]
}

fn info_target_user() -> Option<String> {
    current_user_name()
}

pub fn commands_for_request(
    request: &ApplyRequest,
    info: &SystemInfo,
) -> Result<Vec<(ActionId, CommandSpec)>, PlannerError> {
    let version = info.validate_supported()?;
    if !request.run_update && request.detected_fedora_version != version {
        return Err(PlannerError::UnsupportedFedora(request.detected_fedora_version));
    }
    let available: HashSet<ActionId> = build_plan(info)?.actions.into_iter().map(|a| a.id).collect();
    let mut out = Vec::new();
    if request.run_update {
        out.push((
            ActionId::SystemUpdate,
            CommandSpec::new("dnf", ["upgrade", "-y", "--refresh"]),
        ));
    }
    let mut repo_configured = false;
    for action in &request.selected_actions {
        if !available.contains(action) {
            return Err(PlannerError::UnavailableAction(*action));
        }
        let cmds = commands_for_action(*action, version, info)?;
        if !cmds.is_empty() {
            if *action == ActionId::RpmFusionFree
                || *action == ActionId::RpmFusionNonfree
                || *action == ActionId::CiscoOpenh264Repo
            {
                repo_configured = true;
            }
            for command in cmds {
                out.push((*action, command));
            }
        }
    }

    if repo_configured {
        let last_repo_idx = out.iter().rposition(|(action, _)| {
            *action == ActionId::RpmFusionFree
                || *action == ActionId::RpmFusionNonfree
                || *action == ActionId::CiscoOpenh264Repo
        });
        if let Some(idx) = last_repo_idx {
            let action_id = out[idx].0;
            out.insert(
                idx + 1,
                (action_id, CommandSpec::new("dnf", ["update", "-y", "--refresh"])),
            );
        }
    }

    Ok(out)
}

pub fn rpmfusion_release_url(kind: &str, version: u16) -> String {
    format!(
        "https://mirrors.rpmfusion.org/{kind}/fedora/rpmfusion-{kind}-release-{version}.noarch.rpm"
    )
}

pub fn openh264_command(version: u16) -> CommandSpec {
    if version <= 40 {
        CommandSpec::new("dnf", ["config-manager", "--enable", "fedora-cisco-openh264"])
    } else {
        CommandSpec::new(
            "dnf",
            ["config-manager", "setopt", "fedora-cisco-openh264.enabled=1"],
        )
    }
}

pub fn detect_system() -> SystemInfo {
    let os_release = parse_os_release("/etc/os-release");
    let os_id = os_release.get("ID").cloned().unwrap_or_default();
    let os_name = os_release.get("NAME").cloned().unwrap_or_else(|| os_id.clone());
    let fedora_version = command_output("rpm", ["-E", "%fedora"])
        .and_then(|s| s.trim().parse::<u16>().ok())
        .or_else(|| os_release.get("VERSION_ID").and_then(|s| s.parse::<u16>().ok()));

    SystemInfo {
        os_id,
        os_name,
        fedora_version,
        arch: command_output("uname", ["-m"]).unwrap_or_default().trim().into(),
        is_atomic: command_exists("rpm-ostree") || Path::new("/run/ostree-booted").exists(),
        has_dnf: command_exists("dnf"),
        has_internet: command_status("curl", ["-fsI", "--connect-timeout", "3", "https://mirrors.fedoraproject.org"])
            || command_status("ping", ["-c", "1", "-W", "3", "mirrors.fedoraproject.org"]),
        secure_boot: detect_secure_boot(),
        gpu_vendors: detect_gpu_vendors(),
        installed_packages: installed_packages(),
        enabled_repos: enabled_repos(),
        flatpak_remotes: flatpak_remotes(),
        flatpak_apps: flatpak_apps(),
    }
}

pub fn parse_os_release(path: impl AsRef<Path>) -> std::collections::BTreeMap<String, String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((key.to_string(), value.trim_matches('"').to_string()))
        })
        .collect()
}

fn repo_enabled(info: &SystemInfo, repo: &str) -> bool {
    info.enabled_repos.contains(repo)
}

fn packages_installed(info: &SystemInfo, packages: &[&str]) -> bool {
    packages.iter().all(|package| info.installed_packages.contains(*package))
}

fn detect_secure_boot() -> SecureBootState {
    let Some(output) = command_output("mokutil", ["--sb-state"]) else {
        return SecureBootState::Unknown;
    };
    let lower = output.to_ascii_lowercase();
    if lower.contains("enabled") {
        SecureBootState::Enabled
    } else if lower.contains("disabled") {
        SecureBootState::Disabled
    } else {
        SecureBootState::Unknown
    }
}

fn detect_gpu_vendors() -> BTreeSet<GpuVendor> {
    let mut vendors = BTreeSet::new();
    if let Ok(entries) = fs::read_dir("/sys/class/drm") {
        for entry in entries.flatten() {
            let path = entry.path().join("device/vendor");
            if let Ok(raw) = fs::read_to_string(path) {
                vendors.insert(GpuVendor::from_pci_vendor_id(&raw));
            }
        }
    }
    if vendors.is_empty() {
        if let Some(output) = command_output("lspci", std::iter::empty::<&str>()) {
            for line in output.lines() {
                if let Some(vendor) = GpuVendor::from_lspci_line(line) {
                    vendors.insert(vendor);
                }
            }
        }
    }
    vendors
}

fn installed_packages() -> BTreeSet<String> {
    command_output("rpm", ["-qa", "--qf", "%{NAME}\n"])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn enabled_repos() -> BTreeSet<String> {
    let mut repos = BTreeSet::new();
    let outputs = [
        command_output("dnf", ["repoquery", "--repoid", "*", "--qf", "%{repoid}", "fedora-release"])
            .unwrap_or_default(),
        command_output("dnf", ["repolist", "--enabled"]).unwrap_or_default(),
    ];
    for output in outputs {
        for repo in output
            .lines()
            .filter_map(|line| line.split_whitespace().next())
            .filter(|repo| !repo.eq_ignore_ascii_case("repo") && !repo.eq_ignore_ascii_case("id"))
        {
            repos.insert(repo.to_string());
        }
    }
    repos
}

fn flatpak_remotes() -> BTreeSet<String> {
    command_output("flatpak", ["remotes", "--columns=name"])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "Name")
        .map(ToOwned::to_owned)
        .collect()
}

fn flatpak_apps() -> BTreeSet<String> {
    command_output("flatpak", ["list", "--columns=application"])
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "Application")
        .map(ToOwned::to_owned)
        .collect()
}

fn flatpak_install_commands(app_id: &str, info: &SystemInfo) -> Vec<CommandSpec> {
    let mut commands = Vec::new();
    if !info.flatpak_remotes.contains("flathub") {
        if !info.installed_packages.contains("flatpak") {
            commands.push(CommandSpec::new("dnf", ["install", "-y", "flatpak"]));
        }
        commands.push(CommandSpec::new(
            "flatpak",
            [
                "remote-add",
                "--if-not-exists",
                "flathub",
                "https://dl.flathub.org/repo/flathub.flatpakrepo",
            ],
        ));
    }
    commands.push(CommandSpec::new(
        "flatpak",
        ["remote-modify", "--enable", "flathub"],
    ));
    commands.push(CommandSpec::new("flatpak", ["install", "-y", "flathub", app_id]));
    commands
}

fn command_exists(program: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {program} >/dev/null 2>&1"))
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn command_status<I, S>(program: &str, args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    Command::new(program)
        .args(args)
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn command_output<I, S>(program: &str, args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn base_info(version: u16) -> SystemInfo {
        SystemInfo {
            os_id: "fedora".into(),
            os_name: "Fedora Linux".into(),
            fedora_version: Some(version),
            arch: "x86_64".into(),
            is_atomic: false,
            has_dnf: true,
            has_internet: true,
            secure_boot: SecureBootState::Disabled,
            gpu_vendors: BTreeSet::new(),
            installed_packages: BTreeSet::new(),
            enabled_repos: BTreeSet::new(),
            flatpak_remotes: BTreeSet::new(),
            flatpak_apps: BTreeSet::new(),
        }
    }

    fn unique_temp_home() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("postora-test-{suffix}"))
    }

    #[test]
    fn openh264_uses_legacy_enable_on_fedora_40() {
        assert_eq!(
            openh264_command(40),
            CommandSpec::new("dnf", ["config-manager", "--enable", "fedora-cisco-openh264"])
        );
    }

    #[test]
    fn openh264_uses_setopt_on_fedora_41_and_newer() {
        for version in [41, 42, 43, 44] {
            assert_eq!(
                openh264_command(version),
                CommandSpec::new(
                    "dnf",
                    ["config-manager", "setopt", "fedora-cisco-openh264.enabled=1"]
                )
            );
        }
    }

    #[test]
    fn gpu_vendor_mapping_handles_common_ids_and_lspci() {
        assert_eq!(GpuVendor::from_pci_vendor_id("0x10de"), GpuVendor::Nvidia);
        assert_eq!(GpuVendor::from_pci_vendor_id("1002"), GpuVendor::Amd);
        assert_eq!(GpuVendor::from_pci_vendor_id("8086"), GpuVendor::Intel);
        assert_eq!(
            GpuVendor::from_lspci_line("01:00.0 VGA compatible controller: NVIDIA Corporation AD104"),
            Some(GpuVendor::Nvidia)
        );
    }

    #[test]
    fn rpm_ostree_detection_blocks_mutation() {
        let mut info = base_info(42);
        info.is_atomic = true;
        assert!(matches!(build_plan(&info), Err(PlannerError::AtomicSystem)));
    }

    #[test]
    fn completed_repos_are_not_reinstalled() {
        let mut info = base_info(42);
        info.enabled_repos.insert("rpmfusion-free".into());
        let commands = commands_for_action(ActionId::RpmFusionFree, 42, &info).unwrap();
        assert!(commands.is_empty());
    }

    #[test]
    fn planner_snapshots_cover_supported_versions() {
        for version in [40, 41, 42, 43, 44] {
            let info = base_info(version);
            let commands = commands_for_action(ActionId::CiscoOpenh264Repo, version, &info).unwrap();
            assert_eq!(commands.len(), 1);
            assert_eq!(commands[0], openh264_command(version));
        }
    }

    #[test]
    fn zed_detection_uses_install_directory() {
        let home = unique_temp_home();
        fs::create_dir_all(home.join(".local/bin")).unwrap();
        fs::write(home.join(".local/bin/zed"), "binary").unwrap();
        assert!(zed_installed(&home));
        assert!(!zed_installed(&home.join("missing")));
    }

    #[test]
    fn starship_detection_requires_config_and_shell_hooks() {
        let home = unique_temp_home();
        fs::create_dir_all(home.join(".cargo/bin")).unwrap();
        fs::create_dir_all(home.join(".config")).unwrap();
        fs::write(home.join(".cargo/bin/starship"), "binary").unwrap();
        fs::write(home.join(".config/starship.toml"), "config").unwrap();
        fs::write(home.join(".bashrc"), r#"eval "$(starship init bash)""#).unwrap();
        fs::write(home.join(".zshrc"), r#"eval "$(starship init zsh)""#).unwrap();
        assert!(starship_configured(&home));
        fs::write(home.join(".zshrc"), "echo hello").unwrap();
        assert!(!starship_configured(&home));
    }
}
