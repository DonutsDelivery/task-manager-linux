use gtk4 as gtk;
use gtk::prelude::*;
use gtk::glib;
use libadwaita as adw;
use adw::prelude::*;

use crate::backend::Collector;
use crate::backend::de_restart;
use crate::backend::shortcut_setup;
use crate::config::Config;
use crate::model::SystemSnapshot;
use crate::ui::performance_tab::PerformanceTab;
use crate::ui::process_tab::ProcessTab;
use crate::util;
use std::cell::RefCell;
use std::rc::Rc;

pub struct MainWindow {
    pub window: adw::ApplicationWindow,
}

impl MainWindow {
    pub fn new(app: &adw::Application) -> adw::ApplicationWindow {
        let config = Config::load();

        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Task Manager")
            .default_width(config.window_width)
            .default_height(config.window_height)
            .build();

        // Start backend collector
        let (collector, rx) = Collector::new();
        collector.start();

        // Main layout: sidebar + content
        let sidebar_list = gtk::ListBox::new();
        sidebar_list.set_selection_mode(gtk::SelectionMode::Single);
        sidebar_list.add_css_class("navigation-sidebar");

        let processes_row = adw::ActionRow::builder()
            .title("Processes")
            .build();
        processes_row.add_prefix(&gtk::Image::from_icon_name("system-run-symbolic"));
        let performance_row = adw::ActionRow::builder()
            .title("Performance")
            .build();
        performance_row.add_prefix(&gtk::Image::from_icon_name("utilities-system-monitor-symbolic"));

        sidebar_list.append(&processes_row);
        sidebar_list.append(&performance_row);

        let sidebar_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .width_request(200)
            .child(&sidebar_list)
            .build();

        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);

        // Process tab
        let process_tab = ProcessTab::new();
        stack.add_named(&process_tab.widget, Some("processes"));

        // Performance tab
        let performance_tab = PerformanceTab::new();
        stack.add_named(&performance_tab.widget, Some("performance"));

        // Sidebar selection handler
        let stack_ref = stack.clone();
        sidebar_list.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                let idx = row.index();
                match idx {
                    0 => stack_ref.set_visible_child_name("processes"),
                    1 => stack_ref.set_visible_child_name("performance"),
                    _ => {}
                }
            }
        });

        // Select first row
        if let Some(first_row) = sidebar_list.row_at_index(0) {
            sidebar_list.select_row(Some(&first_row));
        }

        // Status bar
        let status_bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        status_bar.add_css_class("status-bar");
        let status_processes = gtk::Label::new(Some("Processes: 0"));
        let status_cpu = gtk::Label::new(Some("CPU: 0%"));
        let status_memory = gtk::Label::new(Some("Memory: 0%"));
        let status_gpu = gtk::Label::new(Some(""));
        status_bar.append(&status_processes);
        status_bar.append(&status_cpu);
        status_bar.append(&status_memory);
        status_bar.append(&status_gpu);

        let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content_box.append(&stack);
        content_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        content_box.append(&status_bar);

        // Use Paned for resizable sidebar
        let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        paned.set_start_child(Some(&sidebar_scroll));
        paned.set_end_child(Some(&content_box));
        paned.set_position(200);
        paned.set_shrink_start_child(false);
        paned.set_shrink_end_child(false);

        // Header bar
        let header = adw::HeaderBar::new();

        // DE restart menu button
        if let Some(de) = de_restart::detect() {
            let menu = gtk::gio::Menu::new();
            for (i, cmd) in de.commands.iter().enumerate() {
                menu.append(Some(&cmd.label), Some(&format!("win.de-restart-{}", i)));
            }

            let menu_button = gtk::MenuButton::builder()
                .icon_name("system-reboot-symbolic")
                .menu_model(&menu)
                .tooltip_text(&format!("Restart {} components", de.name))
                .build();
            header.pack_end(&menu_button);

            // Register actions for each restart command
            let action_group = gtk::gio::SimpleActionGroup::new();
            for (i, cmd) in de.commands.iter().enumerate() {
                let action = gtk::gio::SimpleAction::new(&format!("de-restart-{}", i), None);
                let label = cmd.label.clone();
                let program = cmd.program.clone();
                let args = cmd.args.clone();
                let window_ref = window.clone();
                action.connect_activate(move |_, _| {
                    let cmd = de_restart::RestartCommand {
                        label: label.clone(),
                        program: program.clone(),
                        args: args.clone(),
                    };
                    show_restart_dialog(&window_ref, &cmd);
                });
                action_group.add_action(&action);
            }
            window.insert_action_group("win", Some(&action_group));
        }

        // Primary menu (hamburger)
        {
            let primary_menu = gtk::gio::Menu::new();
            let shortcut_label = if shortcut_setup::is_installed() {
                "Reinstall Ctrl+Shift+Esc Shortcut"
            } else {
                "Install Ctrl+Shift+Esc Shortcut"
            };
            primary_menu.append(Some(shortcut_label), Some("win.setup-shortcut"));

            let hamburger = gtk::MenuButton::builder()
                .icon_name("open-menu-symbolic")
                .menu_model(&primary_menu)
                .tooltip_text("Menu")
                .build();
            header.pack_end(&hamburger);

            let shortcut_action = gtk::gio::SimpleAction::new("setup-shortcut", None);
            let window_ref = window.clone();
            shortcut_action.connect_activate(move |_, _| {
                setup_shortcut_with_feedback(&window_ref);
            });
            window.add_action(&shortcut_action);
        }

        let main_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        main_box.append(&header);
        main_box.append(&paned);

        window.set_content(Some(&main_box));

        // Set stack to expand
        stack.set_vexpand(true);
        stack.set_hexpand(true);

        // Poll for updates from the collector
        let process_tab = Rc::new(RefCell::new(process_tab));
        let performance_tab = Rc::new(RefCell::new(performance_tab));
        let latest_snapshot: Rc<RefCell<Option<SystemSnapshot>>> = Rc::new(RefCell::new(None));

        let process_tab_clone = process_tab.clone();
        let performance_tab_clone = performance_tab.clone();
        let snapshot_clone = latest_snapshot.clone();
        let status_processes_clone = status_processes.clone();
        let status_cpu_clone = status_cpu.clone();
        let status_memory_clone = status_memory.clone();
        let status_gpu_clone = status_gpu.clone();

        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            // Drain channel, keep latest
            while let Ok(snapshot) = rx.try_recv() {
                *snapshot_clone.borrow_mut() = Some(snapshot);
            }

            if let Some(snapshot) = snapshot_clone.borrow().as_ref() {
                process_tab_clone.borrow_mut().update(snapshot);
                performance_tab_clone.borrow_mut().update(snapshot);

                // Update status bar
                status_processes_clone.set_text(&format!("Processes: {}", snapshot.process_count));
                status_cpu_clone.set_text(&format!("CPU: {}", util::format_percent(snapshot.cpu.total_percent)));
                let mem_pct = if snapshot.memory.total > 0 {
                    (snapshot.memory.used as f64 / snapshot.memory.total as f64) * 100.0
                } else { 0.0 };
                status_memory_clone.set_text(&format!("Memory: {}", util::format_percent(mem_pct)));
                if snapshot.gpu.available {
                    status_gpu_clone.set_text(&format!("GPU: {}", util::format_percent(snapshot.gpu.utilization_percent)));
                }
            }

            glib::ControlFlow::Continue
        });

        // Save window size on close
        let config_clone = config.clone();
        window.connect_close_request(move |win| {
            let mut cfg = config_clone.clone();
            cfg.window_width = win.width();
            cfg.window_height = win.height();
            cfg.save();
            glib::Propagation::Proceed
        });

        window
    }
}

fn setup_shortcut_with_feedback(window: &adw::ApplicationWindow) {
    match shortcut_setup::install() {
        Ok(msg) => {
            let dialog = gtk::MessageDialog::new(
                Some(window),
                gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
                gtk::MessageType::Info,
                gtk::ButtonsType::Ok,
                &msg,
            );
            dialog.connect_response(|d, _| d.close());
            dialog.present();
        }
        Err(e) => {
            let dialog = gtk::MessageDialog::new(
                Some(window),
                gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
                gtk::MessageType::Error,
                gtk::ButtonsType::Ok,
                &format!("Failed to install shortcut:\n\n{}", e),
            );
            dialog.connect_response(|d, _| d.close());
            dialog.present();
        }
    }
}

fn show_restart_dialog(window: &adw::ApplicationWindow, cmd: &de_restart::RestartCommand) {
    let dialog = gtk::MessageDialog::new(
        Some(window),
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        gtk::MessageType::Warning,
        gtk::ButtonsType::None,
        &format!(
            "Are you sure you want to restart this component?\n\n\
             This will run: {} {}\n\n\
             Your desktop may briefly flicker or become unresponsive.",
            cmd.program,
            cmd.args.join(" ")
        ),
    );
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    let restart_btn = dialog.add_button("Restart", gtk::ResponseType::Accept);
    restart_btn.add_css_class("destructive-action");

    let program = cmd.program.clone();
    let args = cmd.args.clone();
    let label = cmd.label.clone();
    let win = window.clone();
    dialog.connect_response(move |d, response| {
        if response == gtk::ResponseType::Accept {
            let restart_cmd = de_restart::RestartCommand {
                label: label.clone(),
                program: program.clone(),
                args: args.clone(),
            };
            if let Err(e) = de_restart::execute(&restart_cmd) {
                let err_dialog = gtk::MessageDialog::new(
                    Some(&win),
                    gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
                    gtk::MessageType::Error,
                    gtk::ButtonsType::Ok,
                    &e,
                );
                err_dialog.connect_response(|d, _| d.close());
                err_dialog.present();
            }
        }
        d.close();
    });
    dialog.present();
}
