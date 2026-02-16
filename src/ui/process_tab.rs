use gtk4 as gtk;
use gtk::prelude::*;
use gtk::glib;
use gtk::gio;
use gtk::subclass::prelude::ObjectSubclassIsExt;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;

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
        pub container_type: RefCell<String>,
        pub user: RefCell<String>,
        pub uid: RefCell<u32>,
        pub threads: RefCell<u64>,
        pub command: RefCell<String>,
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
        *imp.container_type.borrow_mut() = group.leader.container_type.clone();
        *imp.user.borrow_mut() = group.leader.user.clone();
        *imp.uid.borrow_mut() = group.leader.uid;
        *imp.threads.borrow_mut() = group.leader.threads;
        *imp.command.borrow_mut() = group.leader.command.clone();
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
        *imp.container_type.borrow_mut() = proc.container_type.clone();
        *imp.user.borrow_mut() = proc.user.clone();
        *imp.uid.borrow_mut() = proc.uid;
        *imp.threads.borrow_mut() = proc.threads;
        *imp.command.borrow_mut() = proc.command.clone();
    }

    pub fn pid(&self) -> i32 { *self.imp().pid.borrow() }
    pub fn ppid(&self) -> i32 { *self.imp().ppid.borrow() }
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
    pub fn container_type(&self) -> String { self.imp().container_type.borrow().clone() }
    pub fn user(&self) -> String { self.imp().user.borrow().clone() }
    pub fn uid(&self) -> u32 { *self.imp().uid.borrow() }
    pub fn threads(&self) -> u64 { *self.imp().threads.borrow() }
    pub fn command(&self) -> String { self.imp().command.borrow().clone() }
}

/// Helper to unwrap TreeListRow → ProcessObject from a ListItem
fn get_process_obj(item: &gtk::ListItem) -> ProcessObject {
    item.item()
        .and_then(|i| i.downcast::<gtk::TreeListRow>().ok())
        .and_then(|row| row.item())
        .and_then(|i| i.downcast::<ProcessObject>().ok())
        .unwrap()
}

/// Helper to get ProcessObject from selection (with TreeListRow unwrapping)
fn selected_process(sel: &gtk::SingleSelection) -> Option<ProcessObject> {
    sel.selected_item()
        .and_then(|i| i.downcast::<gtk::TreeListRow>().ok())
        .and_then(|row| row.item())
        .and_then(|i| i.downcast::<ProcessObject>().ok())
}

pub struct ProcessTab {
    pub widget: gtk::Box,
    store: gio::ListStore,
    search_entry: gtk::SearchEntry,
    column_view: gtk::ColumnView,
    sort_model: gtk::SortListModel,
    scroll: gtk::ScrolledWindow,
    // Cache for group children data
    children_cache: Rc<RefCell<HashMap<i32, Vec<crate::model::ProcessInfo>>>>,
    child_stores: Rc<RefCell<HashMap<i32, gio::ListStore>>>,
}

impl ProcessTab {
    pub fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.add_css_class("process-view");

        // Add CSS provider for resource level colors
        let css_provider = gtk::CssProvider::new();
        css_provider.load_from_string(
            ".resource-medium { color: @warning_color; }
             .resource-high { color: orange; }
             .resource-critical { color: @error_color; font-weight: bold; }"
        );
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().unwrap(),
            &css_provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        // Search bar
        let search_entry = gtk::SearchEntry::new();
        search_entry.set_placeholder_text(Some("Search processes..."));
        search_entry.add_css_class("search-bar");
        widget.append(&search_entry);

        // List store for process objects
        let store = gio::ListStore::new::<ProcessObject>();

        // Child stores for tree expansion
        let child_stores: Rc<RefCell<HashMap<i32, gio::ListStore>>> =
            Rc::new(RefCell::new(HashMap::new()));

        // TreeListModel wrapping the root store
        let child_stores_for_tree = child_stores.clone();
        let tree_model = gtk::TreeListModel::new(
            store.clone(),
            false, // passthrough
            false, // autoexpand
            move |obj| {
                let proc_obj = obj.downcast_ref::<ProcessObject>()?;
                if proc_obj.is_group() && proc_obj.child_count() > 0 {
                    let stores = child_stores_for_tree.borrow();
                    stores.get(&proc_obj.pid()).map(|s| s.clone().upcast::<gio::ListModel>())
                } else {
                    None
                }
            },
        );

        // Filter model for search (operates on TreeListRow items)
        let filter = gtk::CustomFilter::new(glib::clone!(
            #[weak] search_entry,
            #[upgrade_or] false,
            move |obj| {
                let text = search_entry.text().to_string().to_lowercase();
                if text.is_empty() {
                    return true;
                }
                if let Some(row) = obj.downcast_ref::<gtk::TreeListRow>() {
                    if let Some(proc_obj) = row.item().and_then(|i| i.downcast::<ProcessObject>().ok()) {
                        let name = proc_obj.display_name().to_lowercase();
                        let pid = proc_obj.pid().to_string();
                        let path = proc_obj.exe_path().to_lowercase();
                        return name.contains(&text) || pid.contains(&text) || path.contains(&text);
                    }
                }
                true
            }
        ));
        let filter_model = gtk::FilterListModel::new(Some(tree_model), Some(filter.clone()));

        // Re-filter on search text change
        search_entry.connect_search_changed(move |_| {
            filter.changed(gtk::FilterChange::Different);
        });

        // Sort model (sorter set after columns are built)
        let sort_model = gtk::SortListModel::new(Some(filter_model), None::<gtk::Sorter>);

        // Selection model
        let selection = gtk::SingleSelection::new(Some(sort_model.clone()));
        selection.set_autoselect(false);

        // ColumnView
        let column_view = gtk::ColumnView::new(Some(selection.clone()));
        column_view.set_show_column_separators(true);
        column_view.set_show_row_separators(false);

        // --- Columns ---

        // Name column (with TreeExpander for expand/collapse)
        let name_factory = gtk::SignalListItemFactory::new();
        name_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let expander = gtk::TreeExpander::new();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::Start);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            expander.set_child(Some(&label));
            item.set_child(Some(&expander));
        });
        name_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let row = item.item().and_downcast::<gtk::TreeListRow>().unwrap();
            let obj = row.item().and_downcast::<ProcessObject>().unwrap();
            let expander = item.child().and_downcast::<gtk::TreeExpander>().unwrap();
            expander.set_list_row(Some(&row));
            let label = expander.child().and_downcast::<gtk::Label>().unwrap();
            let name = obj.display_name();
            if obj.is_group() && obj.child_count() > 0 {
                label.set_text(&format!("{} ({})", name, obj.child_count() + 1));
            } else {
                label.set_text(&name);
            }
        });
        name_factory.connect_unbind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            if let Some(expander) = item.child().and_downcast::<gtk::TreeExpander>() {
                expander.set_list_row(None::<&gtk::TreeListRow>);
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
            let obj = get_process_obj(item);
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
            let obj = get_process_obj(item);
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            let cpu = obj.cpu_percent();
            label.set_text(&util::format_percent(cpu));

            // Remove previous level classes
            label.remove_css_class("resource-low");
            label.remove_css_class("resource-medium");
            label.remove_css_class("resource-high");
            label.remove_css_class("resource-critical");

            // Add class based on CPU usage
            if cpu > 90.0 {
                label.add_css_class("resource-critical");
            } else if cpu > 50.0 {
                label.add_css_class("resource-high");
            } else if cpu > 20.0 {
                label.add_css_class("resource-medium");
            }
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
            let obj = get_process_obj(item);
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            let memory_bytes = obj.memory_bytes();
            label.set_text(&util::format_bytes(memory_bytes));

            // Calculate memory percentage (assume 16GB system total for coloring)
            // This is approximate - ideally should get from SystemSnapshot
            let total_memory_bytes = 16u64 * 1024 * 1024 * 1024; // 16GB
            let memory_percent = (memory_bytes as f64 / total_memory_bytes as f64) * 100.0;

            // Remove previous level classes
            label.remove_css_class("resource-low");
            label.remove_css_class("resource-medium");
            label.remove_css_class("resource-high");
            label.remove_css_class("resource-critical");

            // Add class based on memory usage
            if memory_percent > 6.25 { // > 1GB
                label.add_css_class("resource-critical");
            } else if memory_percent > 3.125 { // > 512MB
                label.add_css_class("resource-high");
            } else if memory_percent > 1.25 { // > 200MB
                label.add_css_class("resource-medium");
            }
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
            let obj = get_process_obj(item);
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
            let obj = get_process_obj(item);
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
            let obj = get_process_obj(item);
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
            let obj = get_process_obj(item);
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
            let obj = get_process_obj(item);
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

        // Container column
        let container_factory = gtk::SignalListItemFactory::new();
        container_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::Start);
            item.set_child(Some(&label));
        });
        container_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = get_process_obj(item);
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            let ct = obj.container_type();
            if ct.is_empty() {
                label.set_text("—");
            } else {
                label.set_text(&ct);
            }
        });
        let container_col = gtk::ColumnViewColumn::new(Some("Container"), Some(container_factory));
        container_col.set_fixed_width(90);
        container_col.set_resizable(true);
        let container_sorter = gtk::CustomSorter::new(|a, b| {
            let pa = a.downcast_ref::<ProcessObject>().unwrap();
            let pb = b.downcast_ref::<ProcessObject>().unwrap();
            pa.container_type().cmp(&pb.container_type()).into()
        });
        container_col.set_sorter(Some(&container_sorter));
        column_view.append_column(&container_col);

        // Enable sorting via TreeListRowSorter wrapping the column view sorter
        if let Some(cv_sorter) = column_view.sorter() {
            let tree_sorter = gtk::TreeListRowSorter::new(Some(cv_sorter));
            sort_model.set_sorter(Some(&tree_sorter));
        }

        // Scroll window
        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .child(&column_view)
            .build();
        widget.append(&scroll);
        let scroll_ref = scroll.clone();

        // Context menu
        let children_cache: Rc<RefCell<HashMap<i32, Vec<crate::model::ProcessInfo>>>> =
            Rc::new(RefCell::new(HashMap::new()));

        let menu = gio::Menu::new();
        menu.append(Some("End Task"), Some("process.kill-term"));
        menu.append(Some("Force Kill"), Some("process.kill-force"));
        menu.append(Some("End Group"), Some("process.kill-group"));
        menu.append(Some("Open File Location"), Some("process.open-location"));

        let nice_menu = gio::Menu::new();
        nice_menu.append(Some("Very High (-20)"), Some("process.nice-neg20"));
        nice_menu.append(Some("High (-10)"), Some("process.nice-neg10"));
        nice_menu.append(Some("Normal (0)"), Some("process.nice-0"));
        nice_menu.append(Some("Low (10)"), Some("process.nice-10"));
        nice_menu.append(Some("Very Low (19)"), Some("process.nice-19"));
        menu.append_submenu(Some("Set Priority"), &nice_menu);

        // Create "Send Signal" submenu
        let signal_menu = gio::Menu::new();
        signal_menu.append(Some("SIGSTOP (Pause)"), Some("process.signal-stop"));
        signal_menu.append(Some("SIGCONT (Resume)"), Some("process.signal-cont"));
        signal_menu.append(Some("SIGHUP (Hangup)"), Some("process.signal-hup"));
        signal_menu.append(Some("SIGINT (Interrupt)"), Some("process.signal-int"));
        signal_menu.append(Some("SIGUSR1"), Some("process.signal-usr1"));
        signal_menu.append(Some("SIGUSR2"), Some("process.signal-usr2"));
        menu.append_submenu(Some("Send Signal"), &signal_menu);

        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(&column_view);
        popover.set_has_arrow(false);

        // Action group
        let action_group = gio::SimpleActionGroup::new();

        let sel_clone = selection.clone();
        let cv_ref = column_view.clone();
        let kill_term = gio::SimpleAction::new("kill-term", None);
        kill_term.connect_activate(move |_, _| {
            if let Some(obj) = selected_process(&sel_clone) {
                kill_process(obj.pid(), obj.display_name(), nix::sys::signal::Signal::SIGTERM, &cv_ref);
            }
        });
        action_group.add_action(&kill_term);

        let sel_clone2 = selection.clone();
        let cv_ref2 = column_view.clone();
        let kill_force = gio::SimpleAction::new("kill-force", None);
        kill_force.connect_activate(move |_, _| {
            if let Some(obj) = selected_process(&sel_clone2) {
                kill_process(obj.pid(), obj.display_name(), nix::sys::signal::Signal::SIGKILL, &cv_ref2);
            }
        });
        action_group.add_action(&kill_force);

        let sel_clone3 = selection.clone();
        let open_loc = gio::SimpleAction::new("open-location", None);
        open_loc.connect_activate(move |_, _| {
            if let Some(obj) = selected_process(&sel_clone3) {
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
                if let Some(obj) = selected_process(&sel_c) {
                    set_priority(obj.pid(), obj.display_name(), value, &cv_c);
                }
            });
            action_group.add_action(&action);
        }

        // Signal actions
        let signal_actions = [
            ("stop", Signal::SIGSTOP),
            ("cont", Signal::SIGCONT),
            ("hup", Signal::SIGHUP),
            ("int", Signal::SIGINT),
            ("usr1", Signal::SIGUSR1),
            ("usr2", Signal::SIGUSR2),
        ];

        for (name, sig) in signal_actions {
            let sel_c = selection.clone();
            let cv_c = column_view.clone();
            let action = gio::SimpleAction::new(&format!("signal-{}", name), None);
            action.connect_activate(move |_, _| {
                if let Some(obj) = selected_process(&sel_c) {
                    send_signal(obj.pid(), obj.display_name(), sig, &cv_c);
                }
            });
            action_group.add_action(&action);
        }

        // Kill Group action
        let children_cache_for_kill = children_cache.clone();
        let sel_for_kill_group = selection.clone();
        let cv_for_kill_group = column_view.clone();
        let kill_group = gio::SimpleAction::new("kill-group", None);
        kill_group.set_enabled(false);
        kill_group.connect_activate(move |_, _| {
            if let Some(obj) = selected_process(&sel_for_kill_group) {
                if obj.is_group() && obj.child_count() > 0 {
                    let leader_pid = obj.pid();
                    let name = obj.display_name();
                    if is_critical_process(leader_pid) {
                        show_error_dialog(&cv_for_kill_group,
                            &format!("Cannot kill group \"{}\" — leader (PID {}) is a critical system process.",
                                name, leader_pid));
                        return;
                    }
                    let cache = children_cache_for_kill.borrow();
                    if let Some(children) = cache.get(&leader_pid) {
                        // Kill children first (reverse order), then leader
                        for child in children.iter().rev() {
                            let _ = nix::sys::signal::kill(
                                nix::unistd::Pid::from_raw(child.pid),
                                nix::sys::signal::Signal::SIGKILL,
                            );
                        }
                    }
                    let _ = nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(leader_pid),
                        nix::sys::signal::Signal::SIGKILL,
                    );
                    log::info!("Killed group '{}' (leader PID {})", name, leader_pid);
                }
            }
        });
        action_group.add_action(&kill_group);

        // Dynamically enable/disable kill-group based on selection
        let kill_group_for_sel = kill_group.clone();
        selection.connect_notify_local(Some("selected"), move |sel, _| {
            let enabled = selected_process(sel)
                .map(|obj| obj.is_group() && obj.child_count() > 0)
                .unwrap_or(false);
            kill_group_for_sel.set_enabled(enabled);
        });

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
                    if let Some(obj) = selected_process(&sel_for_keys) {
                        kill_process(obj.pid(), obj.display_name(), nix::sys::signal::Signal::SIGTERM, &cv_for_keys);
                    }
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        widget.add_controller(key_controller);

        // Double-click to open process details
        let dbl_gesture = gtk::GestureClick::new();
        dbl_gesture.set_button(1);
        let sel_for_dbl = selection.clone();
        let cv_for_dbl = column_view.clone();
        dbl_gesture.connect_released(move |gesture, n_press, _, _| {
            if n_press == 2 {
                if let Some(obj) = selected_process(&sel_for_dbl) {
                    show_process_details(&cv_for_dbl, &obj);
                }
                gesture.set_state(gtk::EventSequenceState::Claimed);
            }
        });
        column_view.add_controller(dbl_gesture);

        Self {
            widget,
            store,
            search_entry,
            column_view,
            sort_model,
            scroll: scroll_ref,
            children_cache,
            child_stores,
        }
    }

    pub fn update(&mut self, snapshot: &SystemSnapshot) {
        // 1. Update children cache (keep for kill-group)
        {
            let mut cache = self.children_cache.borrow_mut();
            cache.clear();
            for group in &snapshot.app_groups {
                if !group.children.is_empty() {
                    cache.insert(group.leader.pid, group.children.clone());
                }
            }
        }

        // 2. Populate/update child_stores BEFORE updating root store
        //    (root store changes can trigger create_func calls)
        {
            let mut stores = self.child_stores.borrow_mut();
            let mut active_pids: std::collections::HashSet<i32> = std::collections::HashSet::new();

            for group in &snapshot.app_groups {
                if group.children.is_empty() {
                    continue;
                }
                active_pids.insert(group.leader.pid);

                let child_store = stores.entry(group.leader.pid)
                    .or_insert_with(|| gio::ListStore::new::<ProcessObject>());

                let new_count = group.children.len();
                let old_count = child_store.n_items() as usize;

                for (i, child) in group.children.iter().enumerate() {
                    if i < old_count {
                        if let Some(obj) = child_store.item(i as u32).and_then(|o| o.downcast::<ProcessObject>().ok()) {
                            obj.set_from_process(child);
                        }
                    } else {
                        let obj = ProcessObject::new();
                        obj.set_from_process(child);
                        child_store.append(&obj);
                    }
                }

                if old_count > new_count {
                    child_store.splice(new_count as u32, (old_count - new_count) as u32, &[] as &[ProcessObject]);
                }

                // Notify child store that items were updated in-place
                child_store.items_changed(0, 0, 0);
            }

            // Remove stale child stores for groups that disappeared
            stores.retain(|pid, _| active_pids.contains(pid));
        }

        // 3. Update root store with group leaders
        let new_count = snapshot.app_groups.len();
        let old_count = self.store.n_items() as usize;

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

        if old_count > new_count {
            self.store.splice(new_count as u32, (old_count - new_count) as u32, &[] as &[ProcessObject]);
        }

        // Save scroll position before triggering re-sort
        let vadj = self.scroll.vadjustment();
        let scroll_pos = vadj.value();

        // Notify the sort/filter/tree model that items changed
        self.store.items_changed(0, 0, 0);

        // Trigger re-sort so columns reflect updated values
        if let Some(sorter) = self.sort_model.sorter() {
            sorter.changed(gtk::SorterChange::Different);
        }

        // Restore scroll position
        vadj.set_value(scroll_pos);
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

fn send_signal(pid: i32, name: String, sig: Signal, widget: &gtk::ColumnView) {
    if is_critical_process(pid) {
        let msg = format!(
            "\"{}\" (PID {}) is a critical system process.\n\nSending signal {:?} may crash your system.\n\nAre you sure?",
            name, pid, sig
        );
        show_confirm_dialog(widget, &msg, pid, sig);
        return;
    }

    do_signal(pid, &name, sig, widget);
}

fn do_signal(pid: i32, name: &str, sig: Signal, widget: &gtk::ColumnView) {
    match signal::kill(Pid::from_raw(pid), sig) {
        Ok(_) => log::info!("Sent {:?} to PID {} ({})", sig, pid, name),
        Err(e) => {
            log::error!("Failed to send {:?} to PID {} ({}): {}", sig, pid, name, e);
            let msg = format!(
                "Failed to send signal {:?} to \"{}\" (PID {})\n\n{}\n\nTry launching Task Manager with elevated privileges.",
                sig,
                name,
                pid,
                e
            );
            show_error_dialog(widget, &msg);
        }
    }
}

fn show_confirm_dialog(widget: &gtk::ColumnView, message: &str, pid: i32, signal: Signal) {
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

    // Choose button label based on signal type
    let button_label = if signal == Signal::SIGKILL || signal == Signal::SIGTERM {
        "Kill Anyway"
    } else {
        "Send Anyway"
    };
    let action_btn = dialog.add_button(button_label, gtk::ResponseType::Accept);
    action_btn.add_css_class("destructive-action");

    let name = std::fs::read_to_string(format!("/proc/{}/comm", pid))
        .unwrap_or_else(|_| "unknown".to_string())
        .trim()
        .to_string();

    dialog.connect_response(move |d, response| {
        if response == gtk::ResponseType::Accept {
            if signal == Signal::SIGKILL || signal == Signal::SIGTERM {
                do_kill(pid, &name, signal, &widget_clone);
            } else {
                do_signal(pid, &name, signal, &widget_clone);
            }
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

// ── Process Details Panel (Feature 6) ────────────────────

fn show_process_details(widget: &gtk::ColumnView, obj: &ProcessObject) {
    let window = widget.root()
        .and_then(|r| r.downcast::<gtk::Window>().ok());

    let pid = obj.pid();
    let name = obj.display_name();

    let dialog = gtk::Window::builder()
        .title(&format!("{} (PID {}) — Details", name, pid))
        .default_width(700)
        .default_height(500)
        .modal(true)
        .build();

    if let Some(win) = &window {
        dialog.set_transient_for(Some(win));
    }

    let notebook = gtk::Notebook::new();

    // General tab
    notebook.append_page(&build_general_tab(obj), Some(&gtk::Label::new(Some("General"))));

    // Environment tab
    notebook.append_page(&build_environ_tab(pid), Some(&gtk::Label::new(Some("Environment"))));

    // Open Files tab
    notebook.append_page(&build_files_tab(pid), Some(&gtk::Label::new(Some("Open Files"))));

    // Memory Maps tab
    notebook.append_page(&build_maps_tab(pid), Some(&gtk::Label::new(Some("Memory Maps"))));

    // Network tab
    notebook.append_page(&build_network_tab(pid), Some(&gtk::Label::new(Some("Network"))));

    // Cgroup tab
    notebook.append_page(&build_cgroup_tab(pid), Some(&gtk::Label::new(Some("Cgroup"))));

    dialog.set_child(Some(&notebook));
    dialog.present();
}

fn build_general_tab(obj: &ProcessObject) -> gtk::ScrolledWindow {
    let grid = gtk::Grid::new();
    grid.set_row_spacing(6);
    grid.set_column_spacing(16);
    grid.set_margin_top(12);
    grid.set_margin_start(12);
    grid.set_margin_end(12);
    grid.set_margin_bottom(12);

    let rows: Vec<(&str, String)> = vec![
        ("PID", obj.pid().to_string()),
        ("Parent PID", obj.ppid().to_string()),
        ("Name", obj.display_name()),
        ("User", obj.user()),
        ("State", obj.state()),
        ("Nice", obj.nice().to_string()),
        ("Threads", obj.threads().to_string()),
        ("CPU %", util::format_percent(obj.cpu_percent())),
        ("Memory", util::format_bytes(obj.memory_bytes())),
        ("Container", if obj.container_type().is_empty() { "None".to_string() } else { obj.container_type() }),
        ("Exe Path", obj.exe_path()),
        ("Command", obj.command()),
    ];

    for (i, (label, value)) in rows.iter().enumerate() {
        let key = gtk::Label::new(Some(label));
        key.set_halign(gtk::Align::Start);
        key.add_css_class("dim-label");
        let val = gtk::Label::new(Some(value));
        val.set_halign(gtk::Align::Start);
        val.set_selectable(true);
        val.set_wrap(true);
        grid.attach(&key, 0, i as i32, 1, 1);
        grid.attach(&val, 1, i as i32, 1, 1);
    }

    gtk::ScrolledWindow::builder()
        .child(&grid)
        .vexpand(true)
        .build()
}

fn build_environ_tab(pid: i32) -> gtk::ScrolledWindow {
    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::None);

    if let Ok(environ) = std::fs::read_to_string(format!("/proc/{}/environ", pid)) {
        let mut vars: Vec<&str> = environ.split('\0').filter(|s| !s.is_empty()).collect();
        vars.sort();
        for var in vars {
            let label = gtk::Label::new(Some(var));
            label.set_halign(gtk::Align::Start);
            label.set_selectable(true);
            label.set_wrap(true);
            label.set_margin_top(2);
            label.set_margin_bottom(2);
            label.set_margin_start(8);
            list_box.append(&label);
        }
    } else {
        let label = gtk::Label::new(Some("Unable to read environment (permission denied?)"));
        label.set_margin_top(12);
        list_box.append(&label);
    }

    gtk::ScrolledWindow::builder()
        .child(&list_box)
        .vexpand(true)
        .build()
}

fn build_files_tab(pid: i32) -> gtk::ScrolledWindow {
    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::None);

    let fd_dir = format!("/proc/{}/fd", pid);
    if let Ok(entries) = std::fs::read_dir(&fd_dir) {
        let mut fds: Vec<(String, String)> = Vec::new();
        for entry in entries.flatten() {
            let fd_name = entry.file_name().to_string_lossy().to_string();
            let target = std::fs::read_link(entry.path())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "?".to_string());
            fds.push((fd_name, target));
        }
        fds.sort_by(|a, b| {
            a.0.parse::<u32>().unwrap_or(0).cmp(&b.0.parse::<u32>().unwrap_or(0))
        });
        for (fd, target) in &fds {
            let label = gtk::Label::new(Some(&format!("fd {} → {}", fd, target)));
            label.set_halign(gtk::Align::Start);
            label.set_selectable(true);
            label.set_margin_top(2);
            label.set_margin_bottom(2);
            label.set_margin_start(8);
            list_box.append(&label);
        }
    } else {
        let label = gtk::Label::new(Some("Unable to read file descriptors (permission denied?)"));
        label.set_margin_top(12);
        list_box.append(&label);
    }

    gtk::ScrolledWindow::builder()
        .child(&list_box)
        .vexpand(true)
        .build()
}

fn build_maps_tab(pid: i32) -> gtk::ScrolledWindow {
    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::None);

    if let Ok(maps) = std::fs::read_to_string(format!("/proc/{}/maps", pid)) {
        for line in maps.lines().take(500) {
            let label = gtk::Label::new(Some(line));
            label.set_halign(gtk::Align::Start);
            label.set_selectable(true);
            label.set_margin_top(1);
            label.set_margin_bottom(1);
            label.set_margin_start(8);
            label.add_css_class("monospace");
            list_box.append(&label);
        }
    } else {
        let label = gtk::Label::new(Some("Unable to read memory maps (permission denied?)"));
        label.set_margin_top(12);
        list_box.append(&label);
    }

    gtk::ScrolledWindow::builder()
        .child(&list_box)
        .vexpand(true)
        .build()
}

fn build_network_tab(pid: i32) -> gtk::ScrolledWindow {
    use crate::backend::net_per_process;

    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::None);

    let connections = net_per_process::collect_process_connections(pid);
    if connections.is_empty() {
        let label = gtk::Label::new(Some("No network connections"));
        label.set_margin_top(12);
        list_box.append(&label);
    } else {
        // Header
        let header = gtk::Label::new(Some("Proto    Local Address              Remote Address             State"));
        header.set_halign(gtk::Align::Start);
        header.add_css_class("monospace");
        header.add_css_class("dim-label");
        header.set_margin_start(8);
        header.set_margin_top(4);
        list_box.append(&header);

        for conn in &connections {
            let text = format!(
                "{:<8} {}:{:<6} → {}:{:<6} {}",
                conn.protocol, conn.local_addr, conn.local_port,
                conn.remote_addr, conn.remote_port, conn.state
            );
            let label = gtk::Label::new(Some(&text));
            label.set_halign(gtk::Align::Start);
            label.set_selectable(true);
            label.add_css_class("monospace");
            label.set_margin_start(8);
            label.set_margin_top(1);
            label.set_margin_bottom(1);
            list_box.append(&label);
        }
    }

    gtk::ScrolledWindow::builder()
        .child(&list_box)
        .vexpand(true)
        .build()
}

fn build_cgroup_tab(pid: i32) -> gtk::ScrolledWindow {
    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::None);

    if let Ok(cgroup) = std::fs::read_to_string(format!("/proc/{}/cgroup", pid)) {
        for line in cgroup.lines() {
            let label = gtk::Label::new(Some(line));
            label.set_halign(gtk::Align::Start);
            label.set_selectable(true);
            label.set_margin_top(2);
            label.set_margin_bottom(2);
            label.set_margin_start(8);
            list_box.append(&label);
        }
    } else {
        let label = gtk::Label::new(Some("Unable to read cgroup info"));
        label.set_margin_top(12);
        list_box.append(&label);
    }

    gtk::ScrolledWindow::builder()
        .child(&list_box)
        .vexpand(true)
        .build()
}
