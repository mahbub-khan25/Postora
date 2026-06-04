// Developed by mahbub khan <mahbub.aumi@gmail.com>

use postora_planner::{
    build_plan, commands_for_request, ActionId, ApplyRequest, GpuVendor, PlannerError,
    SecureBootState, SystemInfo,
};
use std::collections::BTreeSet;
use uuid::Uuid;

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

#[test]
fn dnf_update_is_inserted_after_repos() {
    let info = base_info(42);
    let request = ApplyRequest {
        plan_id: Uuid::new_v4(),
        selected_actions: BTreeSet::from([ActionId::RpmFusionFree, ActionId::Flathub]),
        uninstall_actions: BTreeSet::new(),
        detected_fedora_version: 42,
        detected_gpu_vendors: BTreeSet::new(),
        target_user: Some("testuser".into()),
        target_home: Some("/home/testuser".into()),
        run_update: false,
    };
    let commands = commands_for_request(&request, &info).unwrap();
    let rendered = commands
        .iter()
        .map(|(_, command)| command.display())
        .collect::<Vec<_>>();

    assert_eq!(rendered.len(), 5);
    assert!(rendered[0].contains("mirrors.rpmfusion.org"));
    assert_eq!(rendered[1], "dnf update -y --refresh");
    assert!(rendered[2].contains("dnf install -y flatpak"));
    assert!(rendered[3].contains("flatpak remote-add"));
    assert_eq!(
        rendered[4],
        "flatpak remote-modify --system --enable flathub"
    );
}

#[test]
fn command_planner_snapshots_for_fedora_40_to_44() {
    for version in [40, 41, 42, 43, 44] {
        let mut info = base_info(version);
        info.gpu_vendors.extend([GpuVendor::Intel, GpuVendor::Amd]);
        info.installed_packages.insert("ffmpeg-free".into());
        let plan = build_plan(&info).unwrap();
        let selected_actions = plan.actions.iter().map(|action| action.id).collect();
        let request = ApplyRequest {
            plan_id: plan.plan_id,
            selected_actions,
            uninstall_actions: BTreeSet::new(),
            detected_fedora_version: version,
            detected_gpu_vendors: info.gpu_vendors.clone(),
            target_user: Some("testuser".into()),
            target_home: Some("/home/testuser".into()),
            run_update: false,
        };
        let commands = commands_for_request(&request, &info).unwrap();
        let rendered = commands
            .iter()
            .map(|(_, command)| command.display())
            .collect::<Vec<_>>();
        assert!(rendered
            .iter()
            .any(|cmd| cmd.contains(&format!("rpmfusion-free-release-{version}.noarch.rpm"))));
        assert!(rendered
            .iter()
            .any(|cmd| cmd.contains("flatpak remote-add --system --if-not-exists")));
        assert!(rendered
            .iter()
            .any(|cmd| cmd.contains("dnf swap -y ffmpeg-free ffmpeg")));
    }
}

#[test]
fn unavailable_gpu_action_is_rejected() {
    let info = base_info(42);
    let request = ApplyRequest {
        plan_id: Uuid::new_v4(),
        selected_actions: BTreeSet::from([ActionId::NvidiaDriver]),
        uninstall_actions: BTreeSet::new(),
        detected_fedora_version: 42,
        detected_gpu_vendors: BTreeSet::new(),
        target_user: Some("testuser".into()),
        target_home: Some("/home/testuser".into()),
        run_update: false,
    };
    assert!(matches!(
        commands_for_request(&request, &info),
        Err(PlannerError::UnavailableAction(ActionId::NvidiaDriver))
    ));
}

#[test]
fn helper_planning_refuses_non_fedora_systems() {
    let mut info = base_info(42);
    info.os_id = "ubuntu".into();
    let request = ApplyRequest {
        plan_id: Uuid::new_v4(),
        selected_actions: BTreeSet::from([ActionId::Flathub]),
        uninstall_actions: BTreeSet::new(),
        detected_fedora_version: 42,
        detected_gpu_vendors: BTreeSet::new(),
        target_user: Some("testuser".into()),
        target_home: Some("/home/testuser".into()),
        run_update: false,
    };
    assert!(matches!(
        commands_for_request(&request, &info),
        Err(PlannerError::NonFedora)
    ));
}

#[test]
fn system_update_planned_correctly() {
    let info = base_info(42);
    let request = ApplyRequest {
        plan_id: Uuid::new_v4(),
        selected_actions: BTreeSet::new(),
        uninstall_actions: BTreeSet::new(),
        detected_fedora_version: 0,
        detected_gpu_vendors: BTreeSet::new(),
        target_user: Some("testuser".into()),
        target_home: Some("/home/testuser".into()),
        run_update: true,
    };
    let commands = commands_for_request(&request, &info).unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].0, ActionId::SystemUpdate);
    assert_eq!(commands[0].1.display(), "dnf upgrade -y --refresh");
}
