use adw::prelude::*;
use gtk::glib;
use gtk::{Align, Orientation, PolicyType, WrapMode};
use postora_planner::{
    build_plan, detect_system, Action, ActionCategory, ActionId, ApplyRequest, Plan,
    SecureBootState, SystemInfo,
};
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
    ApplyFinished {
        result: Result<bool, String>,
        is_update: bool,
        applied_actions: BTreeSet<ActionId>,
    },
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

    let action_group = adw::PreferencesGroup::builder()
        .title("Fedora Setup")
        .build();
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
        .title("Command Line & Editors")
        .description("Install optional developer tools, editors, shells, and prompts.")
        .build();
    let kde_group = adw::PreferencesGroup::builder()
        .title("KDE")
        .description("Install KDE-focused appearance and desktop tools.")
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
    let log_panel = gtk::Box::new(Orientation::Vertical, 6);
    log_panel.set_margin_start(18);
    log_panel.set_margin_end(18);
    log_panel.set_margin_bottom(12);
    log_panel.set_visible(false);

    let separator = gtk::Separator::new(Orientation::Horizontal);
    separator.set_margin_bottom(6);

    let log_title = gtk::Label::builder()
        .label("Logs")
        .halign(Align::Start)
        .build();
    log_title.add_css_class("heading");

    log_panel.append(&separator);
    log_panel.append(&log_title);
    log_panel.append(&log_scroller);

    let setup_page = gtk::Box::new(Orientation::Vertical, 12);
    setup_page.set_margin_top(18);
    setup_page.set_margin_bottom(18);
    setup_page.set_margin_start(18);
    setup_page.set_margin_end(18);
    setup_page.append(&status_group);
    setup_page.append(&action_group);

    let setup_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .child(&setup_page)
        .build();

    let extra_page = gtk::Box::new(Orientation::Vertical, 12);
    extra_page.set_margin_top(18);
    extra_page.set_margin_bottom(18);
    extra_page.set_margin_start(18);
    extra_page.set_margin_end(18);
    extra_page.append(&extra_group);
    extra_page.append(&kde_group);

    let extra_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .child(&extra_page)
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

    let apps_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .child(&apps_page)
        .build();

    let fonts_page = gtk::Box::new(Orientation::Vertical, 12);
    fonts_page.set_margin_top(18);
    fonts_page.set_margin_bottom(18);
    fonts_page.set_margin_start(18);
    fonts_page.set_margin_end(18);
    fonts_page.append(&fonts_group);

    let fonts_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(PolicyType::Never)
        .child(&fonts_page)
        .build();

    let view_stack = adw::ViewStack::builder()
        .vexpand(true)
        .hexpand(true)
        .build();

    let stack_page_setup = view_stack.add_titled(&setup_scroller, Some("setup"), "System Setup");
    stack_page_setup.set_icon_name(Some("preferences-system-symbolic"));

    let stack_page_extra = view_stack.add_titled(&extra_scroller, Some("extras"), "Tools & Extras");
    stack_page_extra.set_icon_name(Some("applications-utilities-symbolic"));

    let stack_page_apps = view_stack.add_titled(&apps_scroller, Some("apps"), "Applications");
    stack_page_apps.set_icon_name(Some("application-x-executable-symbolic"));

    let stack_page_fonts = view_stack.add_titled(&fonts_scroller, Some("fonts"), "Nerd Fonts");
    stack_page_fonts.set_icon_name(Some("font-x-generic-symbolic"));

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
                .version("0.0.5")
                .developer_name("Mahbub Afzal Khan")
                .support_url("mailto:mahbub.aumi@gmail.com")
                .website("https://github.com/mahbub-khan25/Postora")
                .issue_url("https://github.com/mahbub-khan25/Postora/issues")
                .license_type(gtk::License::MitX11)
                .build();
            about.present();
        });
    }

    let paned = gtk::Paned::new(Orientation::Vertical);
    paned.set_vexpand(true);
    paned.set_start_child(Some(&view_stack));
    paned.set_resize_start_child(true);
    paned.set_shrink_start_child(false);
    paned.set_end_child(Some(&log_panel));
    paned.set_resize_end_child(true);
    paned.set_shrink_end_child(true);
    paned.set_position(1000);

    let toggle_logs_btn = gtk::Button::with_label("Show Logs");
    {
        let log_panel = log_panel.clone();
        let paned = paned.clone();
        let toggle_logs_btn = toggle_logs_btn.clone();
        toggle_logs_btn.connect_clicked(move |btn| {
            if log_panel.is_visible() {
                log_panel.set_visible(false);
                btn.set_label("Show Logs");
            } else {
                log_panel.set_visible(true);
                paned.set_position(440);
                btn.set_label("Hide Logs");
            }
        });
    }

    {
        let log_panel = log_panel.clone();
        let toggle_logs_btn = toggle_logs_btn.clone();
        paned.connect_position_notify(move |p| {
            let pos = p.position();
            if pos >= 540 {
                if log_panel.is_visible() {
                    log_panel.set_visible(false);
                    toggle_logs_btn.set_label("Show Logs");
                }
            } else {
                if !log_panel.is_visible() {
                    log_panel.set_visible(true);
                    toggle_logs_btn.set_label("Hide Logs");
                }
            }
        });
    }

    let analyze_button = gtk::Button::with_label("Analyze System");
    let apply_button = gtk::Button::with_label("Apply Selected Changes");
    apply_button.set_sensitive(false);

    let button_box = gtk::Box::new(Orientation::Horizontal, 8);
    button_box.set_hexpand(true);

    let left_box = gtk::Box::new(Orientation::Horizontal, 8);
    left_box.set_halign(Align::Start);
    left_box.append(&toggle_logs_btn);

    let right_box = gtk::Box::new(Orientation::Horizontal, 8);
    right_box.set_halign(Align::End);
    right_box.set_hexpand(true);
    right_box.append(&analyze_button);
    right_box.append(&apply_button);

    button_box.append(&left_box);
    button_box.append(&right_box);

    let footer = gtk::Box::new(Orientation::Vertical, 8);
    footer.set_margin_top(10);
    footer.set_margin_bottom(10);
    footer.set_margin_start(18);
    footer.set_margin_end(18);
    footer.append(&progress);
    footer.append(&button_box);

    let root = gtk::Box::new(Orientation::Vertical, 0);
    root.append(&paned);
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
        let kde_group = kde_group.clone();
        let fonts_group = fonts_group.clone();
        let empty_row = empty_row.clone();
        let progress = progress.clone();
        let apply_button = apply_button.clone();
        let log_view = log_view.clone();
        let log_scroller = log_scroller.clone();
        let rendered_action_rows = rendered_action_rows.clone();
        let window_clone = window.clone();
        let view_stack_clone = view_stack.clone();
        let analyze_button_clone = analyze_button.clone();
        let log_panel_clone = log_panel.clone();
        let paned_clone = paned.clone();
        let toggle_logs_btn_clone = toggle_logs_btn.clone();
        glib::timeout_add_local(Duration::from_millis(100), move || {
            for message in receiver.try_iter() {
                match message {
                    WorkerMessage::Analyzed(Ok((system, plan))) => {
                        log_panel_clone.set_visible(false);
                        toggle_logs_btn_clone.set_label("Show Logs");
                        *state.system.borrow_mut() = Some(system.clone());
                        *state.plan.borrow_mut() = Some(plan.clone());
                        state.selected.borrow_mut().clear();
                        {
                            let mut completed = state.completed.borrow_mut();
                            completed.clear();
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
                                ActionCategory::OfficeProductivity => {
                                    office_group.remove(&widget.row)
                                }
                                ActionCategory::MediaCreative => creative_group.remove(&widget.row),
                                ActionCategory::UtilitiesTools => {
                                    utilities_group.remove(&widget.row)
                                }
                                ActionCategory::Kde => kde_group.remove(&widget.row),
                            }
                        }
                        if empty_row.parent().is_some() {
                            action_group.remove(&empty_row);
                        }
                        render_actions(
                            &action_group,
                            &extra_group,
                            &kde_group,
                            &fonts_group,
                            &browsers_group,
                            &dev_group,
                            &office_group,
                            &creative_group,
                            &utilities_group,
                            &state,
                            &plan,
                            &rendered_action_rows,
                            &window_clone,
                            &analysis_sender,
                            &apply_button,
                            &analyze_button_clone,
                            &view_stack_clone,
                            &progress,
                            &log_view,
                            &log_scroller,
                            &log_panel_clone,
                            &paned_clone,
                            &toggle_logs_btn_clone,
                        );
                        status_row.set_title(&format_system_title(&system));
                        status_row.set_subtitle(&format_system_subtitle(&system));
                        progress.set_fraction(0.0);
                        progress.set_text(Some("Analysis complete"));
                        apply_button.set_sensitive(
                            plan.actions.iter().any(|action| !action.already_complete),
                        );
                        analyze_button_clone.set_sensitive(true);
                        view_stack_clone.set_sensitive(true);
                        window_clone.set_cursor(None);
                        append_log(&log_view, &log_scroller, "Analysis complete.");
                    }
                    WorkerMessage::Analyzed(Err(error)) => {
                        log_panel_clone.set_visible(true);
                        paned_clone.set_position(440);
                        toggle_logs_btn_clone.set_label("Hide Logs");
                        for widget in rendered_action_rows.borrow_mut().drain(..) {
                            match widget.category {
                                ActionCategory::FedoraSetup => action_group.remove(&widget.row),
                                ActionCategory::ExtraApps => extra_group.remove(&widget.row),
                                ActionCategory::NerdFonts => fonts_group.remove(&widget.row),
                                ActionCategory::WebBrowsers => browsers_group.remove(&widget.row),
                                ActionCategory::DevDatabase => dev_group.remove(&widget.row),
                                ActionCategory::OfficeProductivity => {
                                    office_group.remove(&widget.row)
                                }
                                ActionCategory::MediaCreative => creative_group.remove(&widget.row),
                                ActionCategory::UtilitiesTools => {
                                    utilities_group.remove(&widget.row)
                                }
                                ActionCategory::Kde => kde_group.remove(&widget.row),
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
                        analyze_button_clone.set_sensitive(true);
                        view_stack_clone.set_sensitive(true);
                        window_clone.set_cursor(None);
                        append_log(
                            &log_view,
                            &log_scroller,
                            &format!("Analysis failed: {error}"),
                        );
                    }
                    WorkerMessage::HelperLine(line) => {
                        progress.pulse();
                        progress.set_text(Some("Applying changes"));
                        append_log(&log_view, &log_scroller, &line);
                    }
                    WorkerMessage::ApplyFinished {
                        result: Ok(has_updates),
                        is_update,
                        applied_actions,
                    } => {
                        log_panel_clone.set_visible(false);
                        toggle_logs_btn_clone.set_label("Show Logs");
                        let selected_actions = state.selected.borrow().clone();
                        state.selected.borrow_mut().clear();
                        state.completed.borrow_mut().extend(selected_actions);
                        for widget in rendered_action_rows.borrow().iter() {
                            widget.check.set_active(false);
                        }
                        progress.pulse();
                        progress.set_text(Some("Refreshing status"));
                        apply_button.set_sensitive(false);
                        analyze_button_clone.set_sensitive(true);
                        view_stack_clone.set_sensitive(true);
                        window_clone.set_cursor(None);
                        status_row.set_title("Refreshing status");
                        status_row.set_subtitle("Re-analyzing system state after apply.");
                        append_log(
                            &log_view,
                            &log_scroller,
                            "Apply finished. Refreshing status...",
                        );

                        let needs_restart = (is_update && has_updates)
                            || applied_actions.contains(&ActionId::NvidiaDriver)
                            || applied_actions.contains(&ActionId::AmdAcceleration)
                            || applied_actions.contains(&ActionId::IntelAcceleration)
                            || applied_actions.contains(&ActionId::ZshDefault);

                        if needs_restart {
                            let dialog = adw::MessageDialog::builder()
                                .transient_for(&window_clone)
                                .heading("Restart or Log Out Recommended")
                                .body("A system update, driver installation, or default shell change has been successfully applied. Please restart or log out to ensure all changes take effect properly before performing further operations.")
                                .build();
                            dialog.add_response("ok", "OK");
                            dialog.set_default_response(Some("ok"));
                            dialog.connect_response(None, move |d, _| {
                                d.close();
                            });
                            dialog.present();
                        }

                        spawn_analysis(analysis_sender.clone());
                    }
                    WorkerMessage::ApplyFinished {
                        result: Err(error), ..
                    } => {
                        log_panel_clone.set_visible(true);
                        paned_clone.set_position(440);
                        toggle_logs_btn_clone.set_label("Hide Logs");
                        progress.set_fraction(0.0);
                        progress.set_text(Some("Apply failed"));
                        apply_button.set_sensitive(true);
                        analyze_button_clone.set_sensitive(true);
                        view_stack_clone.set_sensitive(true);
                        window_clone.set_cursor(None);
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
        let apply_button_clone = apply_button.clone();
        let view_stack_clone = view_stack.clone();
        let window_clone = window.clone();
        let log_panel_clone = log_panel.clone();
        let paned_clone = paned.clone();
        let toggle_logs_btn_clone = toggle_logs_btn.clone();
        analyze_button.connect_clicked(move |btn| {
            log_panel_clone.set_visible(true);
            paned_clone.set_position(440);
            toggle_logs_btn_clone.set_label("Hide Logs");
            btn.set_sensitive(false);
            apply_button_clone.set_sensitive(false);
            view_stack_clone.set_sensitive(false);
            let wait_cursor = gtk::gdk::Cursor::from_name("wait", None);
            window_clone.set_cursor(wait_cursor.as_ref());

            progress.pulse();
            progress.set_text(Some("Waiting for authorization"));
            append_log(
                &log_view,
                &log_scroller,
                "Starting system update before analysis...",
            );
            append_log(
                &log_view,
                &log_scroller,
                "Requesting PolicyKit authorization...",
            );
            let sender = sender.clone();
            std::thread::spawn(move || {
                let request = ApplyRequest {
                    plan_id: uuid::Uuid::new_v4(),
                    selected_actions: BTreeSet::new(),
                    uninstall_actions: BTreeSet::new(),
                    detected_fedora_version: 0,
                    detected_gpu_vendors: BTreeSet::new(),
                    target_user: std::env::var("USER").ok(),
                    target_home: std::env::var("HOME").ok(),
                    run_update: true,
                };
                let result = run_helper(request, sender.clone());
                match result {
                    Ok(has_updates) => {
                        let _ = sender.send(WorkerMessage::ApplyFinished {
                            result: Ok(has_updates),
                            is_update: true,
                            applied_actions: BTreeSet::new(),
                        });
                    }
                    Err(error) => {
                        let _ = sender.send(WorkerMessage::HelperLine(format!(
                            "System update failed or skipped: {error}"
                        )));
                        let _ = sender.send(WorkerMessage::HelperLine(
                            "Proceeding with system analysis...".into(),
                        ));
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
        let analyze_button_clone = analyze_button.clone();
        let view_stack_clone = view_stack.clone();
        let window_clone = window.clone();
        let log_panel_clone = log_panel.clone();
        let paned_clone = paned.clone();
        let toggle_logs_btn_clone = toggle_logs_btn.clone();
        apply_button.connect_clicked(move |button| {
            log_panel_clone.set_visible(true);
            paned_clone.set_position(440);
            toggle_logs_btn_clone.set_label("Hide Logs");
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
            analyze_button_clone.set_sensitive(false);
            view_stack_clone.set_sensitive(false);
            let wait_cursor = gtk::gdk::Cursor::from_name("wait", None);
            window_clone.set_cursor(wait_cursor.as_ref());

            progress.pulse();
            progress.set_text(Some("Waiting for authorization"));
            append_log(
                &log_view,
                &log_scroller,
                "Requesting PolicyKit authorization...",
            );
            let sender = sender.clone();
            std::thread::spawn(move || {
                let request = ApplyRequest {
                    plan_id: plan.plan_id,
                    selected_actions: selected_actions.clone(),
                    uninstall_actions: BTreeSet::new(),
                    detected_fedora_version: plan.fedora_version,
                    detected_gpu_vendors: system.gpu_vendors,
                    target_user: std::env::var("USER").ok(),
                    target_home: std::env::var("HOME").ok(),
                    run_update: false,
                };
                let result = run_helper(request, sender.clone());
                let _ = sender.send(WorkerMessage::ApplyFinished {
                    result,
                    is_update: false,
                    applied_actions: selected_actions,
                });
            });
        });
    }

    window.present();
}

#[allow(clippy::too_many_arguments)]
fn render_actions(
    setup_group: &adw::PreferencesGroup,
    extra_group: &adw::PreferencesGroup,
    kde_group: &adw::PreferencesGroup,
    fonts_group: &adw::PreferencesGroup,
    browsers_group: &adw::PreferencesGroup,
    dev_group: &adw::PreferencesGroup,
    office_group: &adw::PreferencesGroup,
    creative_group: &adw::PreferencesGroup,
    utilities_group: &adw::PreferencesGroup,
    state: &UiState,
    plan: &Plan,
    rendered_rows: &Rc<RefCell<Vec<ActionWidgets>>>,
    window: &adw::ApplicationWindow,
    sender: &mpsc::Sender<WorkerMessage>,
    apply_button: &gtk::Button,
    analyze_button: &gtk::Button,
    view_stack: &adw::ViewStack,
    progress: &gtk::ProgressBar,
    log_view: &gtk::TextView,
    log_scroller: &gtk::ScrolledWindow,
    log_panel: &gtk::Box,
    paned: &gtk::Paned,
    toggle_logs_btn: &gtk::Button,
) {
    let completed_actions = state.completed.borrow().clone();
    for action in &plan.actions {
        let completed = action.already_complete || completed_actions.contains(&action.id);
        let row = adw::ActionRow::builder()
            .title(&action.title)
            .subtitle(action_subtitle(action, completed))
            .activatable(true)
            .build();
        row.set_subtitle_lines(4);
        row.set_title_lines(2);
        if completed {
            let status_label = gtk::Label::new(Some(action_status_label(action)));
            status_label.add_css_class("dim-label");
            status_label.set_halign(Align::End);
            row.add_suffix(&status_label);

            let can_uninstall =
                action.category != ActionCategory::FedoraSetup && action.id != ActionId::ZshDefault;
            if can_uninstall {
                let uninstall_button = gtk::Button::builder()
                    .label("Uninstall")
                    .valign(Align::Center)
                    .build();
                uninstall_button.add_css_class("destructive");

                let win = window.clone();
                let snd = sender.clone();
                let app_btn = apply_button.clone();
                let anz_btn = analyze_button.clone();
                let stk = view_stack.clone();
                let prg = progress.clone();
                let view = log_view.clone();
                let scroller = log_scroller.clone();
                let panel = log_panel.clone();
                let pnd = paned.clone();
                let tgl_btn = toggle_logs_btn.clone();

                let action_id = action.id;
                let action_title = action.title.clone();
                let plan_id = plan.plan_id;
                let version = plan.fedora_version;
                let system_info = state.system.borrow().clone();

                uninstall_button.connect_clicked(move |_btn| {
                    let confirm = adw::MessageDialog::builder()
                        .transient_for(&win)
                        .heading(format!("Uninstall {}?", action_title))
                        .body(format!("Are you sure you want to uninstall {}? This action will execute the uninstallation plan.", action_title))
                        .build();
                    confirm.add_response("cancel", "Cancel");
                    confirm.add_response("uninstall", "Uninstall");
                    confirm.set_response_appearance("uninstall", adw::ResponseAppearance::Destructive);
                    confirm.set_default_response(Some("cancel"));

                    let win_clone = win.clone();
                    let snd_clone = snd.clone();
                    let app_btn_clone = app_btn.clone();
                    let anz_btn_clone = anz_btn.clone();
                    let stk_clone = stk.clone();
                    let prg_clone = prg.clone();
                    let view_clone = view.clone();
                    let scroller_clone = scroller.clone();
                    let panel_clone = panel.clone();
                    let pnd_clone = pnd.clone();
                    let tgl_clone = tgl_btn.clone();

                    let system_info_clone = system_info.clone();
                    let title_val = action_title.clone();

                    confirm.connect_response(None, move |dialog, response| {
                        dialog.close();
                        if response == "uninstall" {
                            panel_clone.set_visible(true);
                            pnd_clone.set_position(440);
                            tgl_clone.set_label("Hide Logs");
                            app_btn_clone.set_sensitive(false);
                            anz_btn_clone.set_sensitive(false);
                            stk_clone.set_sensitive(false);
                            let wait_cursor = gtk::gdk::Cursor::from_name("wait", None);
                            win_clone.set_cursor(wait_cursor.as_ref());

                            prg_clone.pulse();
                            let text = format!("Uninstalling: {}", title_val);
                            prg_clone.set_text(Some(&text));
                            append_log(&view_clone, &scroller_clone, &format!("Requesting uninstallation of {}...", title_val));
                            append_log(&view_clone, &scroller_clone, "Requesting PolicyKit authorization...");

                            let snd_thread = snd_clone.clone();
                            let system_info_val = system_info_clone.clone();
                            std::thread::spawn(move || {
                                let mut uninstall_set = BTreeSet::new();
                                uninstall_set.insert(action_id);

                                let request = ApplyRequest {
                                    plan_id,
                                    selected_actions: BTreeSet::new(),
                                    uninstall_actions: uninstall_set,
                                    detected_fedora_version: version,
                                    detected_gpu_vendors: system_info_val.map(|s| s.gpu_vendors).unwrap_or_default(),
                                    target_user: std::env::var("USER").ok(),
                                    target_home: std::env::var("HOME").ok(),
                                    run_update: false,
                                };
                                let result = run_helper(request, snd_thread.clone());
                                let _ = snd_thread.send(WorkerMessage::ApplyFinished {
                                    result,
                                    is_update: false,
                                    applied_actions: BTreeSet::new(),
                                });
                            });
                        }
                    });
                    confirm.present();
                });
                row.add_suffix(&uninstall_button);
            }
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
            ActionCategory::Kde => kde_group.add(&row),
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
        | ActionCategory::UtilitiesTools
        | ActionCategory::Kde => "Installed",
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

fn run_helper(request: ApplyRequest, sender: mpsc::Sender<WorkerMessage>) -> Result<bool, String> {
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
    let mut has_updates = false;
    for line in reader.lines() {
        let line = line.map_err(|error| format!("failed to read helper output: {error}"))?;
        let lower = line.to_ascii_lowercase();
        if lower.contains("upgrading")
            || lower.contains("installing")
            || lower.contains("upgraded:")
            || lower.contains("installed:")
        {
            has_updates = true;
        }
        let _ = sender.send(WorkerMessage::HelperLine(line));
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed to wait for helper: {error}"))?;
    if output.status.success() {
        Ok(has_updates)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "helper exited with {}; {}",
            output.status,
            stderr.trim()
        ))
    }
}
