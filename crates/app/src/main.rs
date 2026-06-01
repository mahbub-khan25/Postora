use adw::prelude::*;
use postora_planner::{
    build_plan, detect_system, Action, ActionCategory, ActionId, ApplyRequest, Plan, SecureBootState, SystemInfo,
};
use gtk::glib;
use gtk::{Align, Orientation, PolicyType, WrapMode};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

const APP_ID: &str = "io.github.mahbub_khan25.Postora";
const HELPER_PATH: &str = "/usr/libexec/postora-helper";

#[derive(Clone)]
struct UiState {
    system: Rc<RefCell<Option<SystemInfo>>>,
    plan: Rc<RefCell<Option<Plan>>>,
    selected: Rc<RefCell<BTreeSet<ActionId>>>,
    completed: Rc<RefCell<BTreeSet<ActionId>>>,
}

#[derive(Clone)]
struct ActionWidgets {
    row: adw::ActionRow,
    check: gtk::CheckButton,
    category: ActionCategory,
}

#[derive(Debug)]
enum WorkerMessage {
    Analyzed(Result<(SystemInfo, Plan), String>),
    HelperLine(String),
    ApplyFinished(Result<(), String>),
}

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    let state = UiState {
        system: Rc::new(RefCell::new(None)),
        plan: Rc::new(RefCell::new(None)),
        selected: Rc::new(RefCell::new(BTreeSet::new())),
        completed: Rc::new(RefCell::new(BTreeSet::new())),
    };

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Postora")
        .default_width(860)
        .default_height(680)
        .build();

    let header = adw::HeaderBar::new();

    let status_group = adw::PreferencesGroup::builder().title("System").build();
    let status_row = adw::ActionRow::builder()
        .title("Ready to analyze")
        .subtitle("No privileged changes are made during analysis.")
        .build();
    status_group.add(&status_row);

    let action_group = adw::PreferencesGroup::builder().title("Fedora Setup").build();
    let empty_row = adw::ActionRow::builder()
        .title("Analyze system to see available actions")
        .build();
    action_group.add(&empty_row);
    let browsers_group = adw::PreferencesGroup::builder()
        .title("Web Browsers")
        .description("Install Flatpak web browsers.")
        .build();
    let dev_group = adw::PreferencesGroup::builder()
        .title("Development & Database")
        .description("Install development and database tools.")
        .build();
    let office_group = adw::PreferencesGroup::builder()
        .title("Office & Productivity")
        .description("Install office suite and note-taking apps.")
        .build();
    let creative_group = adw::PreferencesGroup::builder()
        .title("Media & Creative")
        .description("Install multimedia players, recording, and editor tools.")
        .build();
    let utilities_group = adw::PreferencesGroup::builder()
        .title("Utilities & Tools")
        .description("Install system utilities and helper tools.")
        .build();
    let extra_group = adw::PreferencesGroup::builder()
        .title("Extra Apps")
        .description("Optional application and shell setup.")
        .build();
    let fonts_group = adw::PreferencesGroup::builder()
        .title("Nerd Fonts")
        .description("Select one or more developer fonts to install system-wide.")
        .build();

    let progress = gtk::ProgressBar::new();
    progress.set_hexpand(true);
    progress.set_show_text(true);
    progress.set_text(Some("Idle"));

    let log_view = gtk::TextView::new();
    log_view.set_editable(false);
    log_view.set_left_margin(8);
    log_view.set_monospace(true);
    log_view.set_right_margin(8);
    log_view.set_top_margin(8);
    log_view.set_bottom_margin(8);
    log_view.set_vexpand(true);
    log_view.set_wrap_mode(WrapMode::Char);
    let log_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .min_content_height(170)
        .max_content_height(240)
        .propagate_natural_height(true)
        .vexpand(true)
        .child(&log_view)
        .build();
    let log_expander = gtk::Expander::builder()
        .label("Logs")
        .expanded(true)
        .child(&log_scroller)
        .build();

    let setup_page = gtk::Box::new(Orientation::Vertical, 12);
    setup_page.set_margin_top(18);
    setup_page.set_margin_bottom(18);
    setup_page.set_margin_start(18);
    setup_page.set_margin_end(18);
    setup_page.append(&status_group);
    setup_page.append(&action_group);
    setup_page.append(&extra_group);

    let setup_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .child(&setup_page)
        .build();

    let apps_page = gtk::Box::new(Orientation::Vertical, 12);
    apps_page.set_margin_top(18);
    apps_page.set_margin_bottom(18);
    apps_page.set_margin_start(18);
    apps_page.set_margin_end(18);
    apps_page.append(&browsers_group);
    apps_page.append(&dev_group);
    apps_page.append(&office_group);
    apps_page.append(&creative_group);
    apps_page.append(&utilities_group);
    apps_page.append(&fonts_group);

    let apps_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .child(&apps_page)
        .build();

    let view_stack = adw::ViewStack::builder()
        .vexpand(true)
        .hexpand(true)
        .build();

    let stack_page_setup = view_stack.add_titled(&setup_scroller, Some("setup"), "System Setup");
    stack_page_setup.set_icon_name(Some("preferences-system-symbolic"));

    let stack_page_apps = view_stack.add_titled(&apps_scroller, Some("apps"), "Applications");
    stack_page_apps.set_icon_name(Some("application-x-executable-symbolic"));

    let switcher = adw::ViewSwitcher::builder()
        .stack(&view_stack)
        .halign(Align::Center)
        .build();

    header.set_title_widget(Some(&switcher));

    let about_button = gtk::Button::builder()
        .icon_name("help-about-symbolic")
        .tooltip_text("About Postora")
        .build();
    header.pack_end(&about_button);

    {
        let window_clone = window.clone();
        about_button.connect_clicked(move |_| {
            let about = adw::AboutWindow::builder()
                .transient_for(&window_clone)
                .application_name("Postora")
                .application_icon("io.github.mahbub_khan25.Postora")
                .version("0.1.8")
                .developer_name("Mahbub Afzal Khan")
                .support_url("mailto:mahbub.aumi@gmail.com")
                .website("https://github.com/mahbub-khan25/Postora")
                .issue_url("https://github.com/mahbub-khan25/Postora/issues")
                .license_type(gtk::License::MitX11)
                .build();
            about.present();
        });
    }

    let analyze_button = gtk::Button::with_label("Analyze System");
    let apply_button = gtk::Button::with_label("Apply Selected Changes");
    apply_button.set_sensitive(false);

    let button_box = gtk::Box::new(Orientation::Horizontal, 8);
    button_box.set_halign(Align::End);
    button_box.append(&analyze_button);
    button_box.append(&apply_button);

    let footer = gtk::Box::new(Orientation::Vertical, 8);
    footer.set_margin_top(10);
    footer.set_margin_bottom(10);
    footer.set_margin_start(18);
    footer.set_margin_end(18);
    footer.append(&progress);
    footer.append(&button_box);

    let root = gtk::Box::new(Orientation::Vertical, 0);
    
    log_expander.set_margin_start(18);
    log_expander.set_margin_end(18);
    log_expander.set_margin_bottom(12);

    root.append(&view_stack);
    root.append(&log_expander);
    root.append(&footer);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&root));
    window.set_content(Some(&toolbar_view));

    let (sender, receiver) = mpsc::channel::<WorkerMessage>();
    let rendered_action_rows = Rc::new(RefCell::new(Vec::<ActionWidgets>::new()));
    let analysis_sender = sender.clone();

    {
        let state = state.clone();
        let status_row = status_row.clone();
        let action_group = action_group.clone();
        let extra_group = extra_group.clone();
        let fonts_group = fonts_group.clone();
        let empty_row = empty_row.clone();
        let progress = progress.clone();
        let apply_button = apply_button.clone();
        let log_view = log_view.clone();
        let log_scroller = log_scroller.clone();
        let rendered_action_rows = rendered_action_rows.clone();
        let window_clone = window.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || {
            for message in receiver.try_iter() {
                match message {
                    WorkerMessage::Analyzed(Ok((system, plan))) => {
                        *state.system.borrow_mut() = Some(system.clone());
                        *state.plan.borrow_mut() = Some(plan.clone());
                        state.selected.borrow_mut().clear();
                        {
                            let mut completed = state.completed.borrow_mut();
                            for action in &plan.actions {
                                if action.already_complete {
                                    completed.insert(action.id);
                                }
                            }
                        }
                        for widget in rendered_action_rows.borrow_mut().drain(..) {
                            match widget.category {
                                ActionCategory::FedoraSetup => action_group.remove(&widget.row),
                                ActionCategory::ExtraApps => extra_group.remove(&widget.row),
                                ActionCategory::NerdFonts => fonts_group.remove(&widget.row),
                                ActionCategory::WebBrowsers => browsers_group.remove(&widget.row),
                                ActionCategory::DevDatabase => dev_group.remove(&widget.row),
                                ActionCategory::OfficeProductivity => office_group.remove(&widget.row),
                                ActionCategory::MediaCreative => creative_group.remove(&widget.row),
                                ActionCategory::UtilitiesTools => utilities_group.remove(&widget.row),
                            }
                        }
                        if empty_row.parent().is_some() {
                            action_group.remove(&empty_row);
                        }
                        render_actions(
                            &action_group,
                            &extra_group,
                            &fonts_group,
                            &browsers_group,
                            &dev_group,
                            &office_group,
                            &creative_group,
                            &utilities_group,
                            &state,
                            &plan,
                            &rendered_action_rows,
                        );
                        status_row.set_title(&format_system_title(&system));
                        status_row.set_subtitle(&format_system_subtitle(&system));
                        progress.set_fraction(0.0);
                        progress.set_text(Some("Analysis complete"));
                        apply_button.set_sensitive(plan.actions.iter().any(|action| !action.already_complete));
                        append_log(&log_view, &log_scroller, "Analysis complete.");
                    }
                    WorkerMessage::Analyzed(Err(error)) => {
                        for widget in rendered_action_rows.borrow_mut().drain(..) {
                            match widget.category {
                                ActionCategory::FedoraSetup => action_group.remove(&widget.row),
                                ActionCategory::ExtraApps => extra_group.remove(&widget.row),
                                ActionCategory::NerdFonts => fonts_group.remove(&widget.row),
                                ActionCategory::WebBrowsers => browsers_group.remove(&widget.row),
                                ActionCategory::DevDatabase => dev_group.remove(&widget.row),
                                ActionCategory::OfficeProductivity => office_group.remove(&widget.row),
                                ActionCategory::MediaCreative => creative_group.remove(&widget.row),
                                ActionCategory::UtilitiesTools => utilities_group.remove(&widget.row),
                            }
                        }
                        if empty_row.parent().is_none() {
                            action_group.add(&empty_row);
                        }
                        state.selected.borrow_mut().clear();
                        status_row.set_title("Unsupported or unavailable system");
                        status_row.set_subtitle(&error);
                        progress.set_fraction(0.0);
                        progress.set_text(Some("Analysis failed"));
                        apply_button.set_sensitive(false);
                        append_log(&log_view, &log_scroller, &format!("Analysis failed: {error}"));
                    }
                    WorkerMessage::HelperLine(line) => {
                        progress.pulse();
                        progress.set_text(Some("Applying changes"));
                        append_log(&log_view, &log_scroller, &line);
                    }
                    WorkerMessage::ApplyFinished(Ok(())) => {
                        let selected_actions = state.selected.borrow().clone();
                        state.selected.borrow_mut().clear();
                        state.completed.borrow_mut().extend(selected_actions);
                        for widget in rendered_action_rows.borrow().iter() {
                            widget.check.set_active(false);
                        }
                        progress.pulse();
                        progress.set_text(Some("Refreshing status"));
                        apply_button.set_sensitive(false);
                        status_row.set_title("Refreshing status");
                        status_row.set_subtitle("Re-analyzing system state after apply.");
                        append_log(&log_view, &log_scroller, "Apply finished. Refreshing status...");
                        
                        let dialog = adw::MessageDialog::builder()
                            .transient_for(&window_clone)
                            .heading("Restart or Log Out Recommended")
                            .body("A system update or repository configuration change has been successfully applied. Please restart or log out to ensure all changes take effect properly before performing further operations.")
                            .build();
                        dialog.add_response("ok", "OK");
                        dialog.set_default_response(Some("ok"));
                        dialog.connect_response(None, move |d, _| {
                            d.close();
                        });
                        dialog.present();

                        spawn_analysis(analysis_sender.clone());
                    }
                    WorkerMessage::ApplyFinished(Err(error)) => {
                        progress.set_fraction(0.0);
                        progress.set_text(Some("Apply failed"));
                        apply_button.set_sensitive(true);
                        append_log(&log_view, &log_scroller, &format!("Apply failed: {error}"));
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    {
        let sender = sender.clone();
        let progress = progress.clone();
        let log_view = log_view.clone();
        let log_scroller = log_scroller.clone();
        analyze_button.connect_clicked(move |_| {
            progress.pulse();
            progress.set_text(Some("Waiting for authorization"));
            append_log(&log_view, &log_scroller, "Starting system update before analysis...");
            append_log(&log_view, &log_scroller, "Requesting PolicyKit authorization...");
            let sender = sender.clone();
            std::thread::spawn(move || {
                let request = ApplyRequest {
                    plan_id: uuid::Uuid::new_v4(),
                    selected_actions: BTreeSet::new(),
                    detected_fedora_version: 0,
                    detected_gpu_vendors: BTreeSet::new(),
                    target_user: std::env::var("USER").ok(),
                    target_home: std::env::var("HOME").ok(),
                    run_update: true,
                };
                let result = run_helper(request, sender.clone());
                match result {
                    Ok(()) => {
                        let _ = sender.send(WorkerMessage::ApplyFinished(Ok(())));
                    }
                    Err(error) => {
                        let _ = sender.send(WorkerMessage::HelperLine(format!("System update failed or skipped: {error}")));
                        let _ = sender.send(WorkerMessage::HelperLine("Proceeding with system analysis...".into()));
                        let system = detect_system();
                        let plan_result = build_plan(&system)
                            .map(|plan| (system, plan))
                            .map_err(|error| error.to_string());
                        let _ = sender.send(WorkerMessage::Analyzed(plan_result));
                    }
                }
            });
        });
    }

    {
        let state = state.clone();
        let sender = sender.clone();
        let progress = progress.clone();
        let log_view = log_view.clone();
        let log_scroller = log_scroller.clone();
        apply_button.connect_clicked(move |button| {
            let Some(system) = state.system.borrow().clone() else {
                return;
            };
            let Some(plan) = state.plan.borrow().clone() else {
                return;
            };
            let selected_actions = state.selected.borrow().clone();
            if selected_actions.is_empty() {
                append_log(&log_view, &log_scroller, "No actions selected.");
                return;
            }
            button.set_sensitive(false);
            progress.pulse();
            progress.set_text(Some("Waiting for authorization"));
            append_log(&log_view, &log_scroller, "Requesting PolicyKit authorization...");
            let sender = sender.clone();
            std::thread::spawn(move || {
                let request = ApplyRequest {
                    plan_id: plan.plan_id,
                    selected_actions,
                    detected_fedora_version: plan.fedora_version,
                    detected_gpu_vendors: system.gpu_vendors,
                    target_user: std::env::var("USER").ok(),
                    target_home: std::env::var("HOME").ok(),
                    run_update: false,
                };
                let result = run_helper(request, sender.clone());
                let _ = sender.send(WorkerMessage::ApplyFinished(result));
            });
        });
    }

    window.present();
}

fn render_actions(
    setup_group: &adw::PreferencesGroup,
    extra_group: &adw::PreferencesGroup,
    fonts_group: &adw::PreferencesGroup,
    browsers_group: &adw::PreferencesGroup,
    dev_group: &adw::PreferencesGroup,
    office_group: &adw::PreferencesGroup,
    creative_group: &adw::PreferencesGroup,
    utilities_group: &adw::PreferencesGroup,
    state: &UiState,
    plan: &Plan,
    rendered_rows: &Rc<RefCell<Vec<ActionWidgets>>>,
) {
    let completed_actions = state.completed.borrow().clone();
    for action in &plan.actions {
        let completed = action.already_complete || completed_actions.contains(&action.id);
        let row = adw::ActionRow::builder()
            .title(&action.title)
            .subtitle(&action_subtitle(action, completed))
            .activatable(true)
            .build();
        row.set_subtitle_lines(4);
        row.set_title_lines(2);
        if completed {
            let status_label = gtk::Label::new(Some(action_status_label(action)));
            status_label.add_css_class("dim-label");
            status_label.set_halign(Align::End);
            row.add_suffix(&status_label);
        }
        let check = gtk::CheckButton::new();
        check.set_valign(Align::Center);
        check.set_sensitive(!completed);
        check.set_active(action.selected_by_default && !completed);
        if check.is_active() {
            state.selected.borrow_mut().insert(action.id);
        }
        let selected = state.selected.clone();
        let id = action.id;
        check.connect_toggled(move |check| {
            if check.is_active() {
                selected.borrow_mut().insert(id);
            } else {
                selected.borrow_mut().remove(&id);
            }
        });
        row.add_prefix(&check);
        match action.category {
            ActionCategory::FedoraSetup => setup_group.add(&row),
            ActionCategory::ExtraApps => extra_group.add(&row),
            ActionCategory::NerdFonts => fonts_group.add(&row),
            ActionCategory::WebBrowsers => browsers_group.add(&row),
            ActionCategory::DevDatabase => dev_group.add(&row),
            ActionCategory::OfficeProductivity => office_group.add(&row),
            ActionCategory::MediaCreative => creative_group.add(&row),
            ActionCategory::UtilitiesTools => utilities_group.add(&row),
        }
        rendered_rows.borrow_mut().push(ActionWidgets {
            row,
            check,
            category: action.category,
        });
    }
}

fn action_subtitle(action: &Action, completed: bool) -> String {
    if completed {
        return action_status_label(action).into();
    }
    match &action.warning {
        Some(warning) => format!("{} {}", action.description, warning),
        None => action.description.clone(),
    }
}

fn action_status_label(action: &Action) -> &'static str {
    match action.category {
        ActionCategory::FedoraSetup => "Enabled",
        ActionCategory::ExtraApps => match action.id {
            ActionId::ZshDefault | ActionId::Starship => "Configured",
            _ => "Installed",
        },
        ActionCategory::NerdFonts => "Installed",
        ActionCategory::WebBrowsers
        | ActionCategory::DevDatabase
        | ActionCategory::OfficeProductivity
        | ActionCategory::MediaCreative
        | ActionCategory::UtilitiesTools => "Installed",
    }
}

fn format_system_title(system: &SystemInfo) -> String {
    let version = system
        .fedora_version
        .map(|v| v.to_string())
        .unwrap_or_else(|| "unknown".into());
    format!("{} {} ({})", system.os_name, version, system.arch)
}

fn format_system_subtitle(system: &SystemInfo) -> String {
    let gpus = if system.gpu_vendors.is_empty() {
        "GPU: unknown".into()
    } else {
        format!("GPU: {:?}", system.gpu_vendors)
    };
    let secure_boot = match system.secure_boot {
        SecureBootState::Enabled => "Secure Boot: enabled",
        SecureBootState::Disabled => "Secure Boot: disabled",
        SecureBootState::Unknown => "Secure Boot: unknown",
    };
    format!("{gpus} | {secure_boot}")
}

fn append_log(view: &gtk::TextView, scroller: &gtk::ScrolledWindow, line: &str) {
    let buffer = view.buffer();
    let mut end = buffer.end_iter();
    buffer.insert(&mut end, line);
    buffer.insert(&mut end, "\n");
    let adjustment = scroller.vadjustment();
    glib::idle_add_local_once(move || {
        adjustment.set_value(adjustment.upper() - adjustment.page_size());
    });
}

fn spawn_analysis(sender: mpsc::Sender<WorkerMessage>) {
    std::thread::spawn(move || {
        let system = detect_system();
        let result = build_plan(&system)
            .map(|plan| (system, plan))
            .map_err(|error| error.to_string());
        let _ = sender.send(WorkerMessage::Analyzed(result));
    });
}

fn run_helper(request: ApplyRequest, sender: mpsc::Sender<WorkerMessage>) -> Result<(), String> {
    let helper = std::env::var("POSTORA_HELPER").unwrap_or_else(|_| HELPER_PATH.into());
    let mut child = Command::new("pkexec")
        .arg(helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to start helper through pkexec: {error}"))?;

    let request_json = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
    child
        .stdin
        .as_mut()
        .ok_or_else(|| "failed to open helper stdin".to_string())?
        .write_all(&request_json)
        .map_err(|error| format!("failed to write helper request: {error}"))?;
    drop(child.stdin.take());

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to read helper stdout".to_string())?;
    let reader = BufReader::new(stdout);
    for line in reader.lines() {
        let line = line.map_err(|error| format!("failed to read helper output: {error}"))?;
        let _ = sender.send(WorkerMessage::HelperLine(line));
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed to wait for helper: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("helper exited with {}; {}", output.status, stderr.trim()))
    }
}
