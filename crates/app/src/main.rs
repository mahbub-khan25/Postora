use adw::prelude::*;
use postora_planner::{
    build_plan, detect_system, Action, ActionId, ApplyRequest, Plan, SecureBootState, SystemInfo,
};
use gtk::glib;
use gtk::{Align, Orientation};
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
    };

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Postora")
        .default_width(860)
        .default_height(680)
        .build();

    let header = adw::HeaderBar::new();
    let analyze_button = gtk::Button::with_label("Analyze System");
    let apply_button = gtk::Button::with_label("Apply Selected Changes");
    apply_button.set_sensitive(false);
    header.pack_start(&analyze_button);
    header.pack_end(&apply_button);

    let status_group = adw::PreferencesGroup::builder().title("System").build();
    let status_row = adw::ActionRow::builder()
        .title("Ready to analyze")
        .subtitle("No privileged changes are made during analysis.")
        .build();
    status_group.add(&status_row);

    let action_group = adw::PreferencesGroup::builder().title("Optional Changes").build();
    let empty_row = adw::ActionRow::builder()
        .title("Analyze system to see available actions")
        .build();
    action_group.add(&empty_row);

    let progress = gtk::ProgressBar::new();
    progress.set_show_text(true);
    progress.set_text(Some("Idle"));

    let log_view = gtk::TextView::new();
    log_view.set_editable(false);
    log_view.set_monospace(true);
    log_view.set_vexpand(true);
    let log_scroller = gtk::ScrolledWindow::builder()
        .min_content_height(180)
        .vexpand(true)
        .child(&log_view)
        .build();
    let log_expander = gtk::Expander::builder()
        .label("Logs")
        .expanded(true)
        .child(&log_scroller)
        .build();

    let page = gtk::Box::new(Orientation::Vertical, 12);
    page.set_margin_top(18);
    page.set_margin_bottom(18);
    page.set_margin_start(18);
    page.set_margin_end(18);
    page.append(&status_group);
    page.append(&action_group);
    page.append(&progress);
    page.append(&log_expander);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&page));
    window.set_content(Some(&toolbar_view));

    let (sender, receiver) = mpsc::channel::<WorkerMessage>();

    {
        let state = state.clone();
        let status_row = status_row.clone();
        let action_group = action_group.clone();
        let empty_row = empty_row.clone();
        let progress = progress.clone();
        let apply_button = apply_button.clone();
        let log_view = log_view.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || {
            for message in receiver.try_iter() {
                match message {
                    WorkerMessage::Analyzed(Ok((system, plan))) => {
                        *state.system.borrow_mut() = Some(system.clone());
                        *state.plan.borrow_mut() = Some(plan.clone());
                        state.selected.borrow_mut().clear();
                        action_group.remove(&empty_row);
                        render_actions(&action_group, &state, &plan);
                        status_row.set_title(&format_system_title(&system));
                        status_row.set_subtitle(&format_system_subtitle(&system));
                        progress.set_fraction(0.0);
                        progress.set_text(Some("Analysis complete"));
                        apply_button.set_sensitive(plan.actions.iter().any(|action| !action.already_complete));
                        append_log(&log_view, "Analysis complete.");
                    }
                    WorkerMessage::Analyzed(Err(error)) => {
                        status_row.set_title("Unsupported or unavailable system");
                        status_row.set_subtitle(&error);
                        progress.set_fraction(0.0);
                        progress.set_text(Some("Analysis failed"));
                        apply_button.set_sensitive(false);
                        append_log(&log_view, &format!("Analysis failed: {error}"));
                    }
                    WorkerMessage::HelperLine(line) => {
                        progress.pulse();
                        progress.set_text(Some("Applying changes"));
                        append_log(&log_view, &line);
                    }
                    WorkerMessage::ApplyFinished(Ok(())) => {
                        progress.set_fraction(1.0);
                        progress.set_text(Some("Finished"));
                        apply_button.set_sensitive(false);
                        append_log(&log_view, "Apply finished.");
                    }
                    WorkerMessage::ApplyFinished(Err(error)) => {
                        progress.set_fraction(0.0);
                        progress.set_text(Some("Apply failed"));
                        apply_button.set_sensitive(true);
                        append_log(&log_view, &format!("Apply failed: {error}"));
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
        analyze_button.connect_clicked(move |_| {
            progress.pulse();
            progress.set_text(Some("Analyzing"));
            append_log(&log_view, "Analyzing system...");
            let sender = sender.clone();
            std::thread::spawn(move || {
                let system = detect_system();
                let result = build_plan(&system)
                    .map(|plan| (system, plan))
                    .map_err(|error| error.to_string());
                let _ = sender.send(WorkerMessage::Analyzed(result));
            });
        });
    }

    {
        let state = state.clone();
        let sender = sender.clone();
        let progress = progress.clone();
        let log_view = log_view.clone();
        apply_button.connect_clicked(move |button| {
            let Some(system) = state.system.borrow().clone() else {
                return;
            };
            let Some(plan) = state.plan.borrow().clone() else {
                return;
            };
            let selected_actions = state.selected.borrow().clone();
            if selected_actions.is_empty() {
                append_log(&log_view, "No actions selected.");
                return;
            }
            button.set_sensitive(false);
            progress.pulse();
            progress.set_text(Some("Waiting for authorization"));
            append_log(&log_view, "Requesting PolicyKit authorization...");
            let sender = sender.clone();
            std::thread::spawn(move || {
                let request = ApplyRequest {
                    plan_id: plan.plan_id,
                    selected_actions,
                    detected_fedora_version: plan.fedora_version,
                    detected_gpu_vendors: system.gpu_vendors,
                };
                let result = run_helper(request, sender.clone());
                let _ = sender.send(WorkerMessage::ApplyFinished(result));
            });
        });
    }

    window.present();
}

fn render_actions(group: &adw::PreferencesGroup, state: &UiState, plan: &Plan) {
    for action in &plan.actions {
        let row = adw::ActionRow::builder()
            .title(&action.title)
            .subtitle(&action_subtitle(action))
            .activatable(true)
            .build();
        let check = gtk::CheckButton::new();
        check.set_valign(Align::Center);
        check.set_sensitive(!action.already_complete);
        check.set_active(action.selected_by_default && !action.already_complete);
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
        group.add(&row);
    }
}

fn action_subtitle(action: &Action) -> String {
    if action.already_complete {
        return "Already complete".into();
    }
    match &action.warning {
        Some(warning) => format!("{} {}", action.description, warning),
        None => action.description.clone(),
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

fn append_log(view: &gtk::TextView, line: &str) {
    let buffer = view.buffer();
    let mut end = buffer.end_iter();
    buffer.insert(&mut end, line);
    buffer.insert(&mut end, "\n");
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
