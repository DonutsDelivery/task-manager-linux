use gtk4 as gtk;
use gtk::prelude::*;
use gtk::glib;
use gtk::gio;
use gtk::subclass::prelude::ObjectSubclassIsExt;

use crate::backend::startup::StartupCollector;
use crate::model::startup_entry::{StartupEntry, StartupSource};

// GObject wrapper for startup entry data in the model
mod imp {
    use super::*;
    use gtk::glib;
    use gtk::subclass::prelude::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct StartupObject {
        pub name: RefCell<String>,
        pub enabled: RefCell<bool>,
        pub source: RefCell<String>,
        pub exec: RefCell<String>,
        pub comment: RefCell<String>,
        pub file_path: RefCell<String>,
        pub icon: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StartupObject {
        const NAME: &'static str = "StartupObject";
        type Type = super::StartupObject;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for StartupObject {}
}

glib::wrapper! {
    pub struct StartupObject(ObjectSubclass<imp::StartupObject>);
}

impl StartupObject {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_from_entry(&self, entry: &StartupEntry) {
        let imp = self.imp();
        *imp.name.borrow_mut() = entry.name.clone();
        *imp.enabled.borrow_mut() = entry.enabled;
        *imp.source.borrow_mut() = entry.source.to_string();
        *imp.exec.borrow_mut() = entry.exec.clone();
        *imp.comment.borrow_mut() = entry.comment.clone();
        *imp.file_path.borrow_mut() = entry.file_path.clone();
        *imp.icon.borrow_mut() = entry.icon.clone();
    }

    pub fn name(&self) -> String {
        self.imp().name.borrow().clone()
    }
    pub fn enabled(&self) -> bool {
        *self.imp().enabled.borrow()
    }
    pub fn source(&self) -> String {
        self.imp().source.borrow().clone()
    }
    pub fn exec(&self) -> String {
        self.imp().exec.borrow().clone()
    }
    pub fn comment(&self) -> String {
        self.imp().comment.borrow().clone()
    }
    pub fn file_path(&self) -> String {
        self.imp().file_path.borrow().clone()
    }

    pub fn to_startup_entry(&self) -> StartupEntry {
        let imp = self.imp();
        StartupEntry {
            name: imp.name.borrow().clone(),
            comment: imp.comment.borrow().clone(),
            exec: imp.exec.borrow().clone(),
            icon: imp.icon.borrow().clone(),
            enabled: *imp.enabled.borrow(),
            file_path: imp.file_path.borrow().clone(),
            source: if *imp.source.borrow() == "Systemd" {
                StartupSource::SystemdUser
            } else {
                StartupSource::Autostart
            },
        }
    }

    pub fn set_enabled(&self, enabled: bool) {
        *self.imp().enabled.borrow_mut() = enabled;
    }
}

pub struct StartupTab {
    pub widget: gtk::Box,
    store: gio::ListStore,
}

impl StartupTab {
    pub fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.add_css_class("startup-view");

        // Toolbar with search + refresh
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        toolbar.set_margin_start(6);
        toolbar.set_margin_end(6);
        toolbar.set_margin_top(6);
        toolbar.set_margin_bottom(6);

        let search_entry = gtk::SearchEntry::new();
        search_entry.set_placeholder_text(Some("Search startup apps..."));
        search_entry.set_hexpand(true);
        search_entry.add_css_class("search-bar");
        toolbar.append(&search_entry);

        let refresh_button = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh_button.set_tooltip_text(Some("Refresh startup entries"));
        toolbar.append(&refresh_button);

        widget.append(&toolbar);

        // List store for startup objects
        let store = gio::ListStore::new::<StartupObject>();

        // Filter model for search
        let filter = gtk::CustomFilter::new(glib::clone!(
            #[weak] search_entry,
            #[upgrade_or] false,
            move |obj| {
                let text = search_entry.text().to_string().to_lowercase();
                if text.is_empty() {
                    return true;
                }
                let startup_obj = obj.downcast_ref::<StartupObject>().unwrap();
                let name = startup_obj.name().to_lowercase();
                let exec = startup_obj.exec().to_lowercase();
                let comment = startup_obj.comment().to_lowercase();
                name.contains(&text) || exec.contains(&text) || comment.contains(&text)
            }
        ));
        let filter_model = gtk::FilterListModel::new(Some(store.clone()), Some(filter.clone()));

        // Re-filter on search text change
        search_entry.connect_search_changed(move |_| {
            filter.changed(gtk::FilterChange::Different);
        });

        // Sort model (alphabetical by name by default)
        let sorter = gtk::CustomSorter::new(move |a, b| {
            let sa = a.downcast_ref::<StartupObject>().unwrap();
            let sb = b.downcast_ref::<StartupObject>().unwrap();
            sa.name()
                .to_lowercase()
                .cmp(&sb.name().to_lowercase())
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
            let obj = item.item().and_downcast::<StartupObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&obj.name());
            let comment = obj.comment();
            if !comment.is_empty() {
                label.set_tooltip_text(Some(&comment));
            }
        });
        let name_col = gtk::ColumnViewColumn::new(Some("Name"), Some(name_factory));
        name_col.set_expand(true);
        name_col.set_resizable(true);
        let name_sorter = gtk::CustomSorter::new(|a, b| {
            let sa = a.downcast_ref::<StartupObject>().unwrap();
            let sb = b.downcast_ref::<StartupObject>().unwrap();
            sa.name()
                .to_lowercase()
                .cmp(&sb.name().to_lowercase())
                .into()
        });
        name_col.set_sorter(Some(&name_sorter));
        column_view.append_column(&name_col);

        // Status column (Switch toggle)
        let status_factory = gtk::SignalListItemFactory::new();
        status_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let switch = gtk::Switch::new();
            switch.set_halign(gtk::Align::Center);
            switch.set_valign(gtk::Align::Center);
            item.set_child(Some(&switch));
        });
        status_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<StartupObject>().unwrap();
            let switch = item.child().and_downcast::<gtk::Switch>().unwrap();

            // Block signal while setting initial state
            let enabled = obj.enabled();
            switch.set_active(enabled);

            // Connect the switch state-set signal for toggling
            let obj_clone = obj.clone();
            switch.connect_state_set(move |_switch, active| {
                let entry = obj_clone.to_startup_entry();
                obj_clone.set_enabled(active);
                std::thread::spawn(move || {
                    if let Err(e) = StartupCollector::toggle_autostart(&entry, active) {
                        log::error!("Failed to toggle startup entry '{}': {}", entry.name, e);
                    }
                });
                glib::Propagation::Proceed
            });
        });
        // Unbind: disconnect switch signal to avoid stale closures on recycling
        status_factory.connect_unbind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            if let Some(switch) = item.child().and_downcast::<gtk::Switch>() {
                // Replace with a fresh switch to drop old signal connections
                // The next bind will set up the correct handler
                let new_switch = gtk::Switch::new();
                new_switch.set_halign(gtk::Align::Center);
                new_switch.set_valign(gtk::Align::Center);
                item.set_child(Some(&new_switch));
            }
        });
        let status_col = gtk::ColumnViewColumn::new(Some("Status"), Some(status_factory));
        status_col.set_fixed_width(80);
        status_col.set_resizable(false);
        column_view.append_column(&status_col);

        // Type column
        let type_factory = gtk::SignalListItemFactory::new();
        type_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::Center);
            item.set_child(Some(&label));
        });
        type_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<StartupObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&obj.source());
        });
        let type_col = gtk::ColumnViewColumn::new(Some("Type"), Some(type_factory));
        type_col.set_fixed_width(80);
        type_col.set_resizable(true);
        let type_sorter = gtk::CustomSorter::new(|a, b| {
            let sa = a.downcast_ref::<StartupObject>().unwrap();
            let sb = b.downcast_ref::<StartupObject>().unwrap();
            sa.source().cmp(&sb.source()).into()
        });
        type_col.set_sorter(Some(&type_sorter));
        column_view.append_column(&type_col);

        // Command column
        let cmd_factory = gtk::SignalListItemFactory::new();
        cmd_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::Start);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            item.set_child(Some(&label));
        });
        cmd_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<StartupObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&obj.exec());
        });
        let cmd_col = gtk::ColumnViewColumn::new(Some("Command"), Some(cmd_factory));
        cmd_col.set_expand(true);
        cmd_col.set_resizable(true);
        let cmd_sorter = gtk::CustomSorter::new(|a, b| {
            let sa = a.downcast_ref::<StartupObject>().unwrap();
            let sb = b.downcast_ref::<StartupObject>().unwrap();
            sa.exec().to_lowercase().cmp(&sb.exec().to_lowercase()).into()
        });
        cmd_col.set_sorter(Some(&cmd_sorter));
        column_view.append_column(&cmd_col);

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

        // Keyboard shortcut: Ctrl+F to focus search
        let key_controller = gtk::EventControllerKey::new();
        let search_entry_clone = search_entry.clone();
        key_controller.connect_key_pressed(move |_, key, _, modifier| {
            if key == gtk::gdk::Key::f && modifier == gtk::gdk::ModifierType::CONTROL_MASK {
                search_entry_clone.grab_focus();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        widget.add_controller(key_controller);

        let mut tab = Self { widget, store };

        // Initial load
        tab.load();

        // Refresh button: reload entries
        let store_ref = tab.store.clone();
        refresh_button.connect_clicked(move |_| {
            let entries = StartupCollector::collect();
            store_ref.remove_all();
            for entry in &entries {
                let obj = StartupObject::new();
                obj.set_from_entry(entry);
                store_ref.append(&obj);
            }
            log::info!("Refreshed startup entries: {} found", entries.len());
        });

        tab
    }

    pub fn load(&mut self) {
        let entries = StartupCollector::collect();
        self.store.remove_all();
        for entry in &entries {
            let obj = StartupObject::new();
            obj.set_from_entry(entry);
            self.store.append(&obj);
        }
        log::info!("Loaded startup entries: {} found", entries.len());
    }
}
