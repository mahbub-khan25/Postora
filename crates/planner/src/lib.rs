use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
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
    RpmFusionFree,
    RpmFusionNonfree,
    CiscoOpenh264Repo,
    Openh264Packages,
    MultimediaCodecs,
    NvidiaDriver,
    AmdAcceleration,
    IntelAcceleration,
    Flathub,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Action {
    pub id: ActionId,
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
        title: "RPM Fusion Free".into(),
        description: "Enable the RPM Fusion Free repository for Fedora-compatible packages.".into(),
        recommended: true,
        selected_by_default: !repo_enabled(info, "rpmfusion-free"),
        already_complete: repo_enabled(info, "rpmfusion-free"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::RpmFusionNonfree,
        title: "RPM Fusion Nonfree".into(),
        description: "Enable the RPM Fusion Nonfree repository for codecs and vendor drivers.".into(),
        recommended: true,
        selected_by_default: !repo_enabled(info, "rpmfusion-nonfree"),
        already_complete: repo_enabled(info, "rpmfusion-nonfree"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::CiscoOpenh264Repo,
        title: "Cisco OpenH264 repository".into(),
        description: "Enable Fedora's Cisco OpenH264 repository.".into(),
        recommended: true,
        selected_by_default: !repo_enabled(info, "fedora-cisco-openh264"),
        already_complete: repo_enabled(info, "fedora-cisco-openh264"),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::Openh264Packages,
        title: "OpenH264 packages".into(),
        description: "Install GStreamer and Firefox OpenH264 integration packages.".into(),
        recommended: true,
        selected_by_default: !packages_installed(info, &["gstreamer1-plugin-openh264", "mozilla-openh264"]),
        already_complete: packages_installed(info, &["gstreamer1-plugin-openh264", "mozilla-openh264"]),
        warning: None,
    });
    actions.push(Action {
        id: ActionId::MultimediaCodecs,
        title: "Multimedia codecs".into(),
        description: "Install RPM Fusion multimedia packages and replace ffmpeg-free when needed.".into(),
        recommended: true,
        selected_by_default: !info.installed_packages.contains("ffmpeg"),
        already_complete: info.installed_packages.contains("ffmpeg"),
        warning: None,
    });

    if info.gpu_vendors.contains(&GpuVendor::Nvidia) {
        actions.push(Action {
            id: ActionId::NvidiaDriver,
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
        title: "Flathub".into(),
        description: "Install Flatpak if needed and add the Flathub remote.".into(),
        recommended: true,
        selected_by_default: !info.flatpak_remotes.contains("flathub"),
        already_complete: info.flatpak_remotes.contains("flathub"),
        warning: None,
    });

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
        ActionId::MultimediaCodecs if !info.installed_packages.contains("ffmpeg") => {
            if info.installed_packages.contains("ffmpeg-free") {
                commands.push(CommandSpec::new(
                    "dnf",
                    ["swap", "-y", "ffmpeg-free", "ffmpeg", "--allowerasing"],
                ));
            } else {
                commands.push(CommandSpec::new("dnf", ["install", "-y", "ffmpeg", "--allowerasing"]));
            }
            commands.push(CommandSpec::new(
                "dnf",
                [
                    "group",
                    "upgrade",
                    "-y",
                    "multimedia",
                    "--setop=install_weak_deps=False",
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
        ActionId::Flathub if !info.flatpak_remotes.contains("flathub") => {
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
        _ => {}
    }
    Ok(commands)
}

pub fn commands_for_request(
    request: &ApplyRequest,
    info: &SystemInfo,
) -> Result<Vec<(ActionId, CommandSpec)>, PlannerError> {
    let version = info.validate_supported()?;
    if request.detected_fedora_version != version {
        return Err(PlannerError::UnsupportedFedora(request.detected_fedora_version));
    }
    let available: HashSet<ActionId> = build_plan(info)?.actions.into_iter().map(|a| a.id).collect();
    let mut out = Vec::new();
    for action in &request.selected_actions {
        if !available.contains(action) {
            return Err(PlannerError::UnavailableAction(*action));
        }
        for command in commands_for_action(*action, version, info)? {
            out.push((*action, command));
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
        }
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
}
