use gtk4 as gtk;
use gtk::prelude::*;
use gtk::glib;
use gtk::gio;
use gtk::subclass::prelude::ObjectSubclassIsExt;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::model::{AppGroup, SystemSnapshot};
use crate::util;

// GObject wrapper for process data in the model
mod imp {
    use super::*;
    use gtk::glib;
    use gtk::subclass::prelude::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct ProcessObject {
        pub pid: RefCell<i32>,
        pub ppid: RefCell<i32>,
        pub display_name: RefCell<String>,
        pub cpu_percent: RefCell<f64>,
        pub memory_bytes: RefCell<u64>,
        pub vram_bytes: RefCell<u64>,
        pub disk_read_rate: RefCell<f64>,
        pub disk_write_rate: RefCell<f64>,
        pub state: RefCell<String>,
        pub exe_path: RefCell<String>,
        pub is_group: RefCell<bool>,
        pub child_count: RefCell<u32>,
        pub nice: RefCell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProcessObject {
        const NAME: &'static str = "ProcessObject";
        type Type = super::ProcessObject;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for ProcessObject {}
}

glib::wrapper! {
    pub struct ProcessObject(ObjectSubclass<imp::ProcessObject>);
}

impl ProcessObject {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_from_group(&self, group: &AppGroup) {
        let imp = self.imp();
        *imp.pid.borrow_mut() = group.leader.pid;
        *imp.ppid.borrow_mut() = group.leader.ppid;
        *imp.display_name.borrow_mut() = group.display_name().to_string();
        *imp.cpu_percent.borrow_mut() = group.total_cpu;
        *imp.memory_bytes.borrow_mut() = group.total_memory;
        *imp.vram_bytes.borrow_mut() = group.total_vram;
        *imp.disk_read_rate.borrow_mut() = group.total_disk_read_rate;
        *imp.disk_write_rate.borrow_mut() = group.total_disk_write_rate;
        *imp.state.borrow_mut() = group.leader.state.clone();
        *imp.exe_path.borrow_mut() = group.leader.exe_path.clone();
        *imp.is_group.borrow_mut() = !group.children.is_empty();
        *imp.child_count.borrow_mut() = group.children.len() as u32;
        *imp.nice.borrow_mut() = group.leader.nice;
    }

    pub fn set_from_process(&self, proc: &crate::model::ProcessInfo) {
        let imp = self.imp();
        *imp.pid.borrow_mut() = proc.pid;
        *imp.ppid.borrow_mut() = proc.ppid;
        *imp.display_name.borrow_mut() = proc.display_name.clone();
        *imp.cpu_percent.borrow_mut() = proc.cpu_percent;
        *imp.memory_bytes.borrow_mut() = proc.memory_bytes;
        *imp.vram_bytes.borrow_mut() = proc.vram_bytes;
        *imp.disk_read_rate.borrow_mut() = proc.disk_read_rate;
        *imp.disk_write_rate.borrow_mut() = proc.disk_write_rate;
        *imp.state.borrow_mut() = proc.state.clone();
        *imp.exe_path.borrow_mut() = proc.exe_path.clone();
        *imp.is_group.borrow_mut() = false;
        *imp.child_count.borrow_mut() = 0;
        *imp.nice.borrow_mut() = proc.nice;
    }

    pub fn pid(&self) -> i32 { *self.imp().pid.borrow() }
    pub fn display_name(&self) -> String { self.imp().display_name.borrow().clone() }
    pub fn cpu_percent(&self) -> f64 { *self.imp().cpu_percent.borrow() }
    pub fn memory_bytes(&self) -> u64 { *self.imp().memory_bytes.borrow() }
    pub fn vram_bytes(&self) -> u64 { *self.imp().vram_bytes.borrow() }
    pub fn disk_read_rate(&self) -> f64 { *self.imp().disk_read_rate.borrow() }
    pub fn disk_write_rate(&self) -> f64 { *self.imp().disk_write_rate.borrow() }
    pub fn state(&self) -> String { self.imp().state.borrow().clone() }
    pub fn exe_path(&self) -> String { self.imp().exe_path.borrow().clone() }
    pub fn is_group(&self) -> bool { *self.imp().is_group.borrow() }
    pub fn child_count(&self) -> u32 { *self.imp().child_count.borrow() }
    pub fn nice(&self) -> i32 { *self.imp().nice.borrow() }
}

pub struct ProcessTab {
    pub widget: gtk::Box,
    store: gio::ListStore,
    search_entry: gtk::SearchEntry,
    column_view: gtk::ColumnView,
    // Cache for group children data
    children_cache: Rc<RefCell<HashMap<i32, Vec<crate::model::ProcessInfo>>>>,
}

impl ProcessTab {
    pub fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.add_css_class("process-view");

        // Search bar
        let search_entry = gtk::SearchEntry::new();
        search_entry.set_placeholder_text(Some("Search processes..."));
        search_entry.add_css_class("search-bar");
        widget.append(&search_entry);

        // List store for process objects
        let store = gio::ListStore::new::<ProcessObject>();

        // Filter model for search
        let filter = gtk::CustomFilter::new(glib::clone!(
            #[weak] search_entry,
            #[upgrade_or] false,
            move |obj| {
                let text = search_entry.text().to_string().to_lowercase();
                if text.is_empty() {
                    return true;
                }
                let proc_obj = obj.downcast_ref::<ProcessObject>().unwrap();
                let name = proc_obj.display_name().to_lowercase();
                let pid = proc_obj.pid().to_string();
                let path = proc_obj.exe_path().to_lowercase();
                name.contains(&text) || pid.contains(&text) || path.contains(&text)
            }
        ));
        let filter_model = gtk::FilterListModel::new(Some(store.clone()), Some(filter.clone()));

        // Re-filter on search text change
        search_entry.connect_search_changed(move |_| {
            filter.changed(gtk::FilterChange::Different);
        });

        // Sort model
        let sorter = gtk::CustomSorter::new(move |a, b| {
            let pa = a.downcast_ref::<ProcessObject>().unwrap();
            let pb = b.downcast_ref::<ProcessObject>().unwrap();
            pb.cpu_percent()
                .partial_cmp(&pa.cpu_percent())
                .unwrap_or(std::cmp::Ordering::Equal)
                .into()
        });
        let sort_model = gtk::SortListModel::new(Some(filter_model), Some(sorter.clone()));

        // Selection model
        let selection = gtk::SingleSelection::new(Some(sort_model.clone()));
        selection.set_autoselect(false);

        // ColumnView
        let column_view = gtk::ColumnView::new(Some(selection.clone()));
        column_view.set_show_column_separators(true);
        column_view.set_show_row_separators(false);

        // --- Columns ---

        // Name column
        let name_factory = gtk::SignalListItemFactory::new();
        name_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::Start);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            item.set_child(Some(&label));
        });
        name_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<ProcessObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            let name = obj.display_name();
            if obj.is_group() && obj.child_count() > 0 {
                label.set_text(&format!("{} ({})", name, obj.child_count() + 1));
            } else {
                label.set_text(&name);
            }
        });
        let name_col = gtk::ColumnViewColumn::new(Some("Name"), Some(name_factory));
        name_col.set_expand(true);
        name_col.set_resizable(true);

        // Name sorter
        let name_sorter = gtk::CustomSorter::new(|a, b| {
            let pa = a.downcast_ref::<ProcessObject>().unwrap();
            let pb = b.downcast_ref::<ProcessObject>().unwrap();
            pa.display_name().to_lowercase().cmp(&pb.display_name().to_lowercase()).into()
        });
        name_col.set_sorter(Some(&name_sorter));
        column_view.append_column(&name_col);

        // PID column
        let pid_factory = gtk::SignalListItemFactory::new();
        pid_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::End);
            item.set_child(Some(&label));
        });
        pid_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<ProcessObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&obj.pid().to_string());
        });
        let pid_col = gtk::ColumnViewColumn::new(Some("PID"), Some(pid_factory));
        pid_col.set_fixed_width(80);
        pid_col.set_resizable(true);
        let pid_sorter = gtk::CustomSorter::new(|a, b| {
            let pa = a.downcast_ref::<ProcessObject>().unwrap();
            let pb = b.downcast_ref::<ProcessObject>().unwrap();
            pa.pid().cmp(&pb.pid()).into()
        });
        pid_col.set_sorter(Some(&pid_sorter));
        column_view.append_column(&pid_col);

        // CPU% column
        let cpu_factory = gtk::SignalListItemFactory::new();
        cpu_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::End);
            item.set_child(Some(&label));
        });
        cpu_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<ProcessObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&util::format_percent(obj.cpu_percent()));
        });
        let cpu_col = gtk::ColumnViewColumn::new(Some("CPU"), Some(cpu_factory));
        cpu_col.set_fixed_width(80);
        cpu_col.set_resizable(true);
        let cpu_sorter = gtk::CustomSorter::new(|a, b| {
            let pa = a.downcast_ref::<ProcessObject>().unwrap();
            let pb = b.downcast_ref::<ProcessObject>().unwrap();
            pa.cpu_percent().partial_cmp(&pb.cpu_percent()).unwrap_or(std::cmp::Ordering::Equal).into()
        });
        cpu_col.set_sorter(Some(&cpu_sorter));
        column_view.append_column(&cpu_col);

        // Memory column
        let mem_factory = gtk::SignalListItemFactory::new();
        mem_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::End);
            item.set_child(Some(&label));
        });
        mem_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<ProcessObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&util::format_bytes(obj.memory_bytes()));
        });
        let mem_col = gtk::ColumnViewColumn::new(Some("Memory"), Some(mem_factory));
        mem_col.set_fixed_width(100);
        mem_col.set_resizable(true);
        let mem_sorter = gtk::CustomSorter::new(|a, b| {
            let pa = a.downcast_ref::<ProcessObject>().unwrap();
            let pb = b.downcast_ref::<ProcessObject>().unwrap();
            pa.memory_bytes().cmp(&pb.memory_bytes()).into()
        });
        mem_col.set_sorter(Some(&mem_sorter));
        column_view.append_column(&mem_col);

        // VRAM column
        let vram_factory = gtk::SignalListItemFactory::new();
        vram_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::End);
            item.set_child(Some(&label));
        });
        vram_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<ProcessObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            let vram = obj.vram_bytes();
            if vram > 0 {
                label.set_text(&util::format_bytes(vram));
            } else {
                label.set_text("—");
            }
        });
        let vram_col = gtk::ColumnViewColumn::new(Some("VRAM"), Some(vram_factory));
        vram_col.set_fixed_width(90);
        vram_col.set_resizable(true);
        let vram_sorter = gtk::CustomSorter::new(|a, b| {
            let pa = a.downcast_ref::<ProcessObject>().unwrap();
            let pb = b.downcast_ref::<ProcessObject>().unwrap();
            pa.vram_bytes().cmp(&pb.vram_bytes()).into()
        });
        vram_col.set_sorter(Some(&vram_sorter));
        column_view.append_column(&vram_col);

        // Disk Read column
        let dr_factory = gtk::SignalListItemFactory::new();
        dr_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::End);
            item.set_child(Some(&label));
        });
        dr_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<ProcessObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&util::format_bytes_rate(obj.disk_read_rate()));
        });
        let dr_col = gtk::ColumnViewColumn::new(Some("Disk Read"), Some(dr_factory));
        dr_col.set_fixed_width(100);
        dr_col.set_resizable(true);
        let dr_sorter = gtk::CustomSorter::new(|a, b| {
            let pa = a.downcast_ref::<ProcessObject>().unwrap();
            let pb = b.downcast_ref::<ProcessObject>().unwrap();
            pa.disk_read_rate().partial_cmp(&pb.disk_read_rate()).unwrap_or(std::cmp::Ordering::Equal).into()
        });
        dr_col.set_sorter(Some(&dr_sorter));
        column_view.append_column(&dr_col);

        // Disk Write column
        let dw_factory = gtk::SignalListItemFactory::new();
        dw_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::End);
            item.set_child(Some(&label));
        });
        dw_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<ProcessObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&util::format_bytes_rate(obj.disk_write_rate()));
        });
        let dw_col = gtk::ColumnViewColumn::new(Some("Disk Write"), Some(dw_factory));
        dw_col.set_fixed_width(100);
        dw_col.set_resizable(true);
        let dw_sorter = gtk::CustomSorter::new(|a, b| {
            let pa = a.downcast_ref::<ProcessObject>().unwrap();
            let pb = b.downcast_ref::<ProcessObject>().unwrap();
            pa.disk_write_rate().partial_cmp(&pb.disk_write_rate()).unwrap_or(std::cmp::Ordering::Equal).into()
        });
        dw_col.set_sorter(Some(&dw_sorter));
        column_view.append_column(&dw_col);

        // State column
        let state_factory = gtk::SignalListItemFactory::new();
        state_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::Center);
            item.set_child(Some(&label));
        });
        state_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<ProcessObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&obj.state());
        });
        let state_col = gtk::ColumnViewColumn::new(Some("State"), Some(state_factory));
        state_col.set_fixed_width(60);
        state_col.set_resizable(true);
        let state_sorter = gtk::CustomSorter::new(|a, b| {
            let pa = a.downcast_ref::<ProcessObject>().unwrap();
            let pb = b.downcast_ref::<ProcessObject>().unwrap();
            pa.state().cmp(&pb.state()).into()
        });
        state_col.set_sorter(Some(&state_sorter));
        column_view.append_column(&state_col);

        // Path column
        let path_factory = gtk::SignalListItemFactory::new();
        path_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::Start);
            label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            item.set_child(Some(&label));
        });
        path_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<ProcessObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&obj.exe_path());
        });
        let path_col = gtk::ColumnViewColumn::new(Some("Path"), Some(path_factory));
        path_col.set_fixed_width(200);
        path_col.set_resizable(true);
        let path_sorter = gtk::CustomSorter::new(|a, b| {
            let pa = a.downcast_ref::<ProcessObject>().unwrap();
            let pb = b.downcast_ref::<ProcessObject>().unwrap();
            pa.exe_path().cmp(&pb.exe_path()).into()
        });
        path_col.set_sorter(Some(&path_sorter));
        column_view.append_column(&path_col);

        // Enable sorting via the column view sorter
        let cv_sorter = column_view.sorter();
        if let Some(s) = cv_sorter {
            sort_model.set_sorter(Some(&s));
        }

        // Scroll window
        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .child(&column_view)
            .build();
        widget.append(&scroll);

        // Context menu
        let children_cache: Rc<RefCell<HashMap<i32, Vec<crate::model::ProcessInfo>>>> =
            Rc::new(RefCell::new(HashMap::new()));

        let menu = gio::Menu::new();
        menu.append(Some("End Task"), Some("process.kill-term"));
        menu.append(Some("Force Kill"), Some("process.kill-force"));
        menu.append(Some("Open File Location"), Some("process.open-location"));

        let nice_menu = gio::Menu::new();
        nice_menu.append(Some("Very High (-20)"), Some("process.nice-neg20"));
        nice_menu.append(Some("High (-10)"), Some("process.nice-neg10"));
        nice_menu.append(Some("Normal (0)"), Some("process.nice-0"));
        nice_menu.append(Some("Low (10)"), Some("process.nice-10"));
        nice_menu.append(Some("Very Low (19)"), Some("process.nice-19"));
        menu.append_submenu(Some("Set Priority"), &nice_menu);

        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(&column_view);
        popover.set_has_arrow(false);

        // Action group
        let action_group = gio::SimpleActionGroup::new();

        let sel_clone = selection.clone();
        let cv_ref = column_view.clone();
        let kill_term = gio::SimpleAction::new("kill-term", None);
        kill_term.connect_activate(move |_, _| {
            if let Some(obj) = sel_clone.selected_item().and_then(|i| i.downcast::<ProcessObject>().ok()) {
                kill_process(obj.pid(), obj.display_name(), nix::sys::signal::Signal::SIGTERM, &cv_ref);
            }
        });
        action_group.add_action(&kill_term);

        let sel_clone2 = selection.clone();
        let cv_ref2 = column_view.clone();
        let kill_force = gio::SimpleAction::new("kill-force", None);
        kill_force.connect_activate(move |_, _| {
            if let Some(obj) = sel_clone2.selected_item().and_then(|i| i.downcast::<ProcessObject>().ok()) {
                kill_process(obj.pid(), obj.display_name(), nix::sys::signal::Signal::SIGKILL, &cv_ref2);
            }
        });
        action_group.add_action(&kill_force);

        let sel_clone3 = selection.clone();
        let open_loc = gio::SimpleAction::new("open-location", None);
        open_loc.connect_activate(move |_, _| {
            if let Some(obj) = sel_clone3.selected_item().and_then(|i| i.downcast::<ProcessObject>().ok()) {
                let path = obj.exe_path();
                if let Some(dir) = std::path::Path::new(&path).parent() {
                    let _ = std::process::Command::new("xdg-open")
                        .arg(dir)
                        .spawn();
                }
            }
        });
        action_group.add_action(&open_loc);

        // Nice actions
        for (suffix, value) in [("neg20", -20), ("neg10", -10), ("0", 0), ("10", 10), ("19", 19)] {
            let sel_c = selection.clone();
            let cv_c = column_view.clone();
            let action = gio::SimpleAction::new(&format!("nice-{}", suffix), None);
            action.connect_activate(move |_, _| {
                if let Some(obj) = sel_c.selected_item().and_then(|i| i.downcast::<ProcessObject>().ok()) {
                    set_priority(obj.pid(), obj.display_name(), value, &cv_c);
                }
            });
            action_group.add_action(&action);
        }

        column_view.insert_action_group("process", Some(&action_group));

        // Right-click gesture
        let gesture = gtk::GestureClick::new();
        gesture.set_button(3); // Right click
        let popover_clone = popover.clone();
        gesture.connect_pressed(move |gesture, _, x, y| {
            popover_clone.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover_clone.popup();
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        column_view.add_controller(gesture);

        // Keyboard shortcuts
        let key_controller = gtk::EventControllerKey::new();
        let search_entry_clone = search_entry.clone();
        let sel_for_keys = selection.clone();
        let cv_for_keys = column_view.clone();
        key_controller.connect_key_pressed(move |_, key, _, modifier| {
            match (key, modifier) {
                (gtk::gdk::Key::f, gtk::gdk::ModifierType::CONTROL_MASK) => {
                    search_entry_clone.grab_focus();
                    glib::Propagation::Stop
                }
                (gtk::gdk::Key::Delete, _) => {
                    if let Some(obj) = sel_for_keys.selected_item().and_then(|i| i.downcast::<ProcessObject>().ok()) {
                        kill_process(obj.pid(), obj.display_name(), nix::sys::signal::Signal::SIGTERM, &cv_for_keys);
                    }
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        widget.add_controller(key_controller);

        Self {
            widget,
            store,
            search_entry,
            column_view,
            children_cache,
        }
    }

    pub fn update(&mut self, snapshot: &SystemSnapshot) {
        // Update children cache
        {
            let mut cache = self.children_cache.borrow_mut();
            cache.clear();
            for group in &snapshot.app_groups {
                if !group.children.is_empty() {
                    cache.insert(group.leader.pid, group.children.clone());
                }
            }
        }

        // Update the store efficiently
        let new_count = snapshot.app_groups.len();
        let old_count = self.store.n_items() as usize;

        // Reuse existing objects where possible, add/remove as needed
        for (i, group) in snapshot.app_groups.iter().enumerate() {
            if i < old_count {
                if let Some(obj) = self.store.item(i as u32).and_then(|o| o.downcast::<ProcessObject>().ok()) {
                    obj.set_from_group(group);
                }
            } else {
                let obj = ProcessObject::new();
                obj.set_from_group(group);
                self.store.append(&obj);
            }
        }

        // Remove extras
        if old_count > new_count {
            self.store.splice(new_count as u32, (old_count - new_count) as u32, &[] as &[ProcessObject]);
        }

        // Notify the sort model that items changed
        self.store.items_changed(0, 0, 0);
    }
}

fn kill_process(pid: i32, name: String, signal: nix::sys::signal::Signal, widget: &gtk::ColumnView) {
    if is_critical_process(pid) {
        let action = if signal == nix::sys::signal::Signal::SIGKILL { "force kill" } else { "end" };
        let msg = format!(
            "\"{}\" (PID {}) is a critical system process.\n\nKilling it will crash your system.\n\nAre you sure you want to {} it?",
            name, pid, action
        );
        show_confirm_dialog(widget, &msg, pid, signal);
        return;
    }

    do_kill(pid, &name, signal, widget);
}

fn do_kill(pid: i32, name: &str, signal: nix::sys::signal::Signal, widget: &gtk::ColumnView) {
    use nix::sys::signal;
    use nix::unistd::Pid;

    match signal::kill(Pid::from_raw(pid), signal) {
        Ok(_) => log::info!("Sent {:?} to PID {} ({})", signal, pid, name),
        Err(e) => {
            log::error!("Failed to send {:?} to PID {} ({}): {}", signal, pid, name, e);
            let msg = format!(
                "Failed to {} \"{}\" (PID {})\n\n{}\n\nTry launching Task Manager with elevated privileges.",
                if signal == nix::sys::signal::Signal::SIGKILL { "force kill" } else { "end" },
                name,
                pid,
                e
            );
            show_error_dialog(widget, &msg);
        }
    }
}

fn is_critical_process(pid: i32) -> bool {
    if pid <= 2 {
        return true; // PID 1 (init/systemd), PID 2 (kthreadd)
    }
    // Check if it's a kernel thread or essential system service
    let comm = std::fs::read_to_string(format!("/proc/{}/comm", pid)).unwrap_or_default();
    let comm = comm.trim();
    matches!(
        comm,
        "systemd" | "init" | "kthreadd" | "Xorg" | "Xwayland"
        | "kwin_wayland" | "kwin_x11" | "plasmashell" | "sddm"
        | "dbus-daemon" | "polkitd" | "loginctl" | "logind"
        | "systemd-logind" | "pipewire" | "wireplumber"
    )
}

fn set_priority(pid: i32, name: String, nice: i32, widget: &gtk::ColumnView) {
    unsafe {
        let result = libc::setpriority(libc::PRIO_PROCESS, pid as u32, nice);
        if result == 0 {
            log::info!("Set PID {} ({}) priority to {}", pid, name, nice);
        } else {
            let err = std::io::Error::last_os_error();
            log::error!("Failed to set PID {} ({}) priority: {}", pid, name, err);
            let msg = format!(
                "Failed to set priority for \"{}\" (PID {})\n\n{}\n\nTry launching Task Manager with elevated privileges.",
                name, pid, err
            );
            show_error_dialog(widget, &msg);
        }
    }
}

fn show_confirm_dialog(widget: &gtk::ColumnView, message: &str, pid: i32, signal: nix::sys::signal::Signal) {
    let window = widget.root()
        .and_then(|r| r.downcast::<gtk::Window>().ok());
    let widget_clone = widget.clone();

    let dialog = gtk::MessageDialog::new(
        window.as_ref(),
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        gtk::MessageType::Warning,
        gtk::ButtonsType::None,
        message,
    );
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    let kill_btn = dialog.add_button("Kill Anyway", gtk::ResponseType::Accept);
    kill_btn.add_css_class("destructive-action");

    let name = std::fs::read_to_string(format!("/proc/{}/comm", pid))
        .unwrap_or_else(|_| "unknown".to_string())
        .trim()
        .to_string();

    dialog.connect_response(move |d, response| {
        if response == gtk::ResponseType::Accept {
            do_kill(pid, &name, signal, &widget_clone);
        }
        d.close();
    });
    dialog.present();
}

fn show_error_dialog(widget: &gtk::ColumnView, message: &str) {
    let window = widget.root()
        .and_then(|r| r.downcast::<gtk::Window>().ok());

    let dialog = gtk::MessageDialog::new(
        window.as_ref(),
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        gtk::MessageType::Error,
        gtk::ButtonsType::Ok,
        message,
    );
    dialog.connect_response(|d, _| d.close());
    dialog.present();
}
