use gtk4 as gtk;
use libadwaita as adw;
use gtk::prelude::*;
use adw::prelude::*;
use gtk::glib;
use gtk::gio;
use gtk::subclass::prelude::ObjectSubclassIsExt;
use std::cell::RefCell;
use std::rc::Rc;

use crate::backend::services::{ServicesCollector, is_systemd_available};

// ---------------------------------------------------------------------------
// ServiceObject - GObject wrapper for a systemd service entry
// ---------------------------------------------------------------------------

mod imp {
    use super::*;
    use gtk::glib;
    use gtk::subclass::prelude::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct ServiceObject {
        pub name: RefCell<String>,
        pub description: RefCell<String>,
        pub active_state: RefCell<String>,
        pub sub_state: RefCell<String>,
        pub unit_file_state: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ServiceObject {
        const NAME: &'static str = "ServiceObject";
        type Type = super::ServiceObject;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for ServiceObject {}
}

glib::wrapper! {
    pub struct ServiceObject(ObjectSubclass<imp::ServiceObject>);
}

impl ServiceObject {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_from_entry(&self, entry: &crate::model::service_entry::ServiceEntry) {
        let imp = self.imp();
        *imp.name.borrow_mut() = entry.name.clone();
        *imp.description.borrow_mut() = entry.description.clone();
        *imp.active_state.borrow_mut() = entry.active_state.clone();
        *imp.sub_state.borrow_mut() = entry.sub_state.clone();
        *imp.unit_file_state.borrow_mut() = entry.unit_file_state.clone();
    }

    pub fn name(&self) -> String {
        self.imp().name.borrow().clone()
    }
    pub fn description(&self) -> String {
        self.imp().description.borrow().clone()
    }
    pub fn active_state(&self) -> String {
        self.imp().active_state.borrow().clone()
    }
    pub fn sub_state(&self) -> String {
        self.imp().sub_state.borrow().clone()
    }
    pub fn unit_file_state(&self) -> String {
        self.imp().unit_file_state.borrow().clone()
    }
}

// ---------------------------------------------------------------------------
// ServicesTab
// ---------------------------------------------------------------------------

pub struct ServicesTab {
    pub widget: gtk::Box,
    store: gio::ListStore,
    search_entry: gtk::SearchEntry,
    filter_dropdown: gtk::DropDown,
    filter: gtk::CustomFilter,
    content_box: gtk::Box,
    status_page: adw::StatusPage,
}

impl ServicesTab {
    pub fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.add_css_class("services-view");

        // --- Toolbar ---
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        toolbar.add_css_class("toolbar");
        toolbar.set_margin_start(6);
        toolbar.set_margin_end(6);
        toolbar.set_margin_top(6);
        toolbar.set_margin_bottom(6);

        let search_entry = gtk::SearchEntry::new();
        search_entry.set_placeholder_text(Some("Search services..."));
        search_entry.set_hexpand(true);
        search_entry.add_css_class("search-bar");
        toolbar.append(&search_entry);

        // Status filter dropdown
        let filter_options = gtk::StringList::new(&["All", "Active", "Failed", "Inactive"]);
        let filter_dropdown = gtk::DropDown::new(Some(filter_options), gtk::Expression::NONE);
        filter_dropdown.set_selected(0);
        toolbar.append(&filter_dropdown);

        // Refresh button
        let refresh_button = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh_button.set_tooltip_text(Some("Refresh services"));
        toolbar.append(&refresh_button);

        widget.append(&toolbar);

        // --- List Store ---
        let store = gio::ListStore::new::<ServiceObject>();

        // --- Filter model ---
        let search_entry_weak = search_entry.downgrade();
        let dropdown_weak = filter_dropdown.downgrade();
        let filter = gtk::CustomFilter::new(move |obj| {
            let search_entry = match search_entry_weak.upgrade() {
                Some(e) => e,
                None => return true,
            };
            let dropdown = match dropdown_weak.upgrade() {
                Some(d) => d,
                None => return true,
            };

            let svc = obj.downcast_ref::<ServiceObject>().unwrap();

            // Text filter
            let text = search_entry.text().to_string().to_lowercase();
            if !text.is_empty() {
                let name = svc.name().to_lowercase();
                let desc = svc.description().to_lowercase();
                if !name.contains(&text) && !desc.contains(&text) {
                    return false;
                }
            }

            // Status filter
            let selected = dropdown.selected();
            match selected {
                0 => true, // All
                1 => svc.active_state() == "active",
                2 => svc.active_state() == "failed",
                3 => svc.active_state() == "inactive",
                _ => true,
            }
        });

        let filter_model = gtk::FilterListModel::new(Some(store.clone()), Some(filter.clone()));

        // Re-filter on search text change
        {
            let filter_ref = filter.clone();
            search_entry.connect_search_changed(move |_| {
                filter_ref.changed(gtk::FilterChange::Different);
            });
        }

        // Re-filter on dropdown change
        {
            let filter_ref = filter.clone();
            filter_dropdown.connect_selected_notify(move |_| {
                filter_ref.changed(gtk::FilterChange::Different);
            });
        }

        // --- Sort model ---
        let sorter = gtk::CustomSorter::new(|a, b| {
            let sa = a.downcast_ref::<ServiceObject>().unwrap();
            let sb = b.downcast_ref::<ServiceObject>().unwrap();
            sa.name().to_lowercase().cmp(&sb.name().to_lowercase()).into()
        });
        let sort_model = gtk::SortListModel::new(Some(filter_model), Some(sorter.clone()));

        // --- Selection model ---
        let selection = gtk::SingleSelection::new(Some(sort_model.clone()));
        selection.set_autoselect(false);

        // --- ColumnView ---
        let column_view = gtk::ColumnView::new(Some(selection.clone()));
        column_view.set_show_column_separators(true);
        column_view.set_show_row_separators(false);

        // -- Name column --
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
            let obj = item.item().and_downcast::<ServiceObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&obj.name());
        });
        let name_col = gtk::ColumnViewColumn::new(Some("Name"), Some(name_factory));
        name_col.set_expand(true);
        name_col.set_resizable(true);
        let name_sorter = gtk::CustomSorter::new(|a, b| {
            let sa = a.downcast_ref::<ServiceObject>().unwrap();
            let sb = b.downcast_ref::<ServiceObject>().unwrap();
            sa.name().to_lowercase().cmp(&sb.name().to_lowercase()).into()
        });
        name_col.set_sorter(Some(&name_sorter));
        column_view.append_column(&name_col);

        // -- Description column --
        let desc_factory = gtk::SignalListItemFactory::new();
        desc_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::Start);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            item.set_child(Some(&label));
        });
        desc_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<ServiceObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&obj.description());
        });
        let desc_col = gtk::ColumnViewColumn::new(Some("Description"), Some(desc_factory));
        desc_col.set_expand(true);
        desc_col.set_resizable(true);
        let desc_sorter = gtk::CustomSorter::new(|a, b| {
            let sa = a.downcast_ref::<ServiceObject>().unwrap();
            let sb = b.downcast_ref::<ServiceObject>().unwrap();
            sa.description().to_lowercase().cmp(&sb.description().to_lowercase()).into()
        });
        desc_col.set_sorter(Some(&desc_sorter));
        column_view.append_column(&desc_col);

        // -- Active column (colored) --
        let active_factory = gtk::SignalListItemFactory::new();
        active_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::Center);
            item.set_child(Some(&label));
        });
        active_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<ServiceObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            let state = obj.active_state();
            label.set_text(&state);

            // Remove any previously applied state classes
            label.remove_css_class("success");
            label.remove_css_class("error");
            label.remove_css_class("dim-label");

            if state == "active" {
                label.add_css_class("success");
            } else if state == "failed" {
                label.add_css_class("error");
            } else {
                label.add_css_class("dim-label");
            }
        });
        active_factory.connect_unbind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            if let Some(label) = item.child().and_downcast::<gtk::Label>() {
                label.remove_css_class("success");
                label.remove_css_class("error");
                label.remove_css_class("dim-label");
            }
        });
        let active_col = gtk::ColumnViewColumn::new(Some("Active"), Some(active_factory));
        active_col.set_fixed_width(80);
        active_col.set_resizable(true);
        let active_sorter = gtk::CustomSorter::new(|a, b| {
            let sa = a.downcast_ref::<ServiceObject>().unwrap();
            let sb = b.downcast_ref::<ServiceObject>().unwrap();
            sa.active_state().cmp(&sb.active_state()).into()
        });
        active_col.set_sorter(Some(&active_sorter));
        column_view.append_column(&active_col);

        // -- Sub-State column --
        let sub_factory = gtk::SignalListItemFactory::new();
        sub_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::Center);
            item.set_child(Some(&label));
        });
        sub_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<ServiceObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&obj.sub_state());
        });
        let sub_col = gtk::ColumnViewColumn::new(Some("Sub-State"), Some(sub_factory));
        sub_col.set_fixed_width(80);
        sub_col.set_resizable(true);
        let sub_sorter = gtk::CustomSorter::new(|a, b| {
            let sa = a.downcast_ref::<ServiceObject>().unwrap();
            let sb = b.downcast_ref::<ServiceObject>().unwrap();
            sa.sub_state().cmp(&sb.sub_state()).into()
        });
        sub_col.set_sorter(Some(&sub_sorter));
        column_view.append_column(&sub_col);

        // -- Unit File State column --
        let ufs_factory = gtk::SignalListItemFactory::new();
        ufs_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::Center);
            item.set_child(Some(&label));
        });
        ufs_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<ServiceObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&obj.unit_file_state());
        });
        let ufs_col = gtk::ColumnViewColumn::new(Some("Unit File State"), Some(ufs_factory));
        ufs_col.set_fixed_width(100);
        ufs_col.set_resizable(true);
        let ufs_sorter = gtk::CustomSorter::new(|a, b| {
            let sa = a.downcast_ref::<ServiceObject>().unwrap();
            let sb = b.downcast_ref::<ServiceObject>().unwrap();
            sa.unit_file_state().cmp(&sb.unit_file_state()).into()
        });
        ufs_col.set_sorter(Some(&ufs_sorter));
        column_view.append_column(&ufs_col);

        // Enable sorting via column view sorter
        let cv_sorter = column_view.sorter();
        if let Some(s) = cv_sorter {
            sort_model.set_sorter(Some(&s));
        }

        // --- Scroll window ---
        let scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .child(&column_view)
            .build();

        // --- Status page for non-systemd systems ---
        let status_page = adw::StatusPage::builder()
            .icon_name("dialog-information-symbolic")
            .title("systemd not detected")
            .description("Services tab requires systemd to be available")
            .vexpand(true)
            .hexpand(true)
            .build();

        // Content box: either scroll view or status page
        let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        content_box.set_vexpand(true);
        content_box.set_hexpand(true);

        // Decide which to show based on systemd availability
        if is_systemd_available() {
            content_box.append(&scroll);
        } else {
            content_box.append(&status_page);
        }

        widget.append(&content_box);

        // --- Context menu ---
        let menu = gio::Menu::new();
        menu.append(Some("Start"), Some("service.start"));
        menu.append(Some("Stop"), Some("service.stop"));
        menu.append(Some("Restart"), Some("service.restart"));
        menu.append(Some("Enable"), Some("service.enable"));
        menu.append(Some("Disable"), Some("service.disable"));

        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(&column_view);
        popover.set_has_arrow(false);

        // Action group
        let action_group = gio::SimpleActionGroup::new();

        // Helper: create action that runs a service command, with optional confirmation
        fn make_service_action(
            action_name: &str,
            systemctl_action: &'static str,
            needs_confirm: bool,
            selection: &gtk::SingleSelection,
            column_view: &gtk::ColumnView,
        ) -> gio::SimpleAction {
            let action = gio::SimpleAction::new(action_name, None);
            let sel = selection.clone();
            let cv = column_view.clone();
            action.connect_activate(move |_, _| {
                let Some(obj) = sel
                    .selected_item()
                    .and_then(|i| i.downcast::<ServiceObject>().ok())
                else {
                    return;
                };
                let name = obj.name();
                let cv_ref = cv.clone();

                if needs_confirm {
                    show_confirm_and_run(&cv_ref, &name, systemctl_action);
                } else {
                    run_service_action(&cv_ref, &name, systemctl_action);
                }
            });
            action
        }

        action_group.add_action(&make_service_action(
            "start",
            "start",
            false,
            &selection,
            &column_view,
        ));
        action_group.add_action(&make_service_action(
            "stop",
            "stop",
            true,
            &selection,
            &column_view,
        ));
        action_group.add_action(&make_service_action(
            "restart",
            "restart",
            false,
            &selection,
            &column_view,
        ));
        action_group.add_action(&make_service_action(
            "enable",
            "enable",
            false,
            &selection,
            &column_view,
        ));
        action_group.add_action(&make_service_action(
            "disable",
            "disable",
            true,
            &selection,
            &column_view,
        ));

        column_view.insert_action_group("service", Some(&action_group));

        // Right-click gesture
        let gesture = gtk::GestureClick::new();
        gesture.set_button(3);
        let popover_clone = popover.clone();
        gesture.connect_pressed(move |gesture, _, x, y| {
            popover_clone.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
                x as i32, y as i32, 1, 1,
            )));
            popover_clone.popup();
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        column_view.add_controller(gesture);

        // Keyboard shortcut: Ctrl+F to focus search
        let key_controller = gtk::EventControllerKey::new();
        let search_entry_clone = search_entry.clone();
        key_controller.connect_key_pressed(move |_, key, _, modifier| {
            if key == gtk::gdk::Key::f && modifier == gtk::gdk::ModifierType::CONTROL_MASK {
                search_entry_clone.grab_focus();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        widget.add_controller(key_controller);

        // Refresh button handler
        let store_rc: Rc<RefCell<Option<gio::ListStore>>> =
            Rc::new(RefCell::new(Some(store.clone())));
        {
            let store_rc_clone = store_rc.clone();
            refresh_button.connect_clicked(move |_| {
                if let Some(ref s) = *store_rc_clone.borrow() {
                    populate_store(s);
                }
            });
        }

        ServicesTab {
            widget,
            store,
            search_entry,
            filter_dropdown,
            filter,
            content_box,
            status_page,
        }
    }

    /// Load (or reload) the service list from systemd.
    pub fn load(&mut self) {
        if is_systemd_available() {
            populate_store(&self.store);
        }
    }
}

/// Populate the list store with current service data from systemctl.
fn populate_store(store: &gio::ListStore) {
    let entries = ServicesCollector::collect();
    let new_count = entries.len();
    let old_count = store.n_items() as usize;

    for (i, entry) in entries.iter().enumerate() {
        if i < old_count {
            if let Some(obj) = store
                .item(i as u32)
                .and_then(|o| o.downcast::<ServiceObject>().ok())
            {
                obj.set_from_entry(entry);
            }
        } else {
            let obj = ServiceObject::new();
            obj.set_from_entry(entry);
            store.append(&obj);
        }
    }

    // Remove extras
    if old_count > new_count {
        store.splice(
            new_count as u32,
            (old_count - new_count) as u32,
            &[] as &[ServiceObject],
        );
    }

    store.items_changed(0, 0, 0);
}

/// Show a confirmation dialog, then run the action on approval.
fn show_confirm_and_run(column_view: &gtk::ColumnView, service_name: &str, action: &str) {
    let window = column_view
        .root()
        .and_then(|r| r.downcast::<gtk::Window>().ok());

    let msg = format!(
        "Are you sure you want to {} the service \"{}\"?",
        action, service_name
    );

    let dialog = gtk::MessageDialog::new(
        window.as_ref(),
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        gtk::MessageType::Warning,
        gtk::ButtonsType::None,
        &msg,
    );
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    let confirm_btn = dialog.add_button(&capitalize(action), gtk::ResponseType::Accept);
    confirm_btn.add_css_class("destructive-action");

    let name = service_name.to_string();
    let act = action.to_string();
    let cv = column_view.clone();
    dialog.connect_response(move |d, response| {
        if response == gtk::ResponseType::Accept {
            run_service_action(&cv, &name, &act);
        }
        d.close();
    });
    dialog.present();
}

/// Execute a systemctl action on a service in a background thread.
fn run_service_action(_column_view: &gtk::ColumnView, service_name: &str, action: &str) {
    let name = service_name.to_string();
    let act = action.to_string();

    std::thread::spawn(move || {
        match ServicesCollector::service_action(&name, &act) {
            Ok(()) => {
                log::info!("Service action '{}' on '{}' succeeded", act, name);
            }
            Err(e) => {
                log::error!("Service action '{}' on '{}' failed: {}", act, name, e);
            }
        }
    });
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}
