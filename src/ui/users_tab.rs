use gtk4 as gtk;
use gtk::prelude::*;
use gtk::glib;
use gtk::gio;
use gtk::subclass::prelude::ObjectSubclassIsExt;

use crate::backend::users::{self, UserInfo};
use crate::model::SystemSnapshot;
use crate::util;

// GObject wrapper for user data in the model
mod imp {
    use gtk4 as gtk;
    use gtk::glib;
    use gtk::subclass::prelude::*;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct UserObject {
        pub uid: RefCell<u32>,
        pub username: RefCell<String>,
        pub session_count: RefCell<u32>,
        pub cpu_percent: RefCell<f64>,
        pub memory_bytes: RefCell<u64>,
        pub process_count: RefCell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for UserObject {
        const NAME: &'static str = "UserObject";
        type Type = super::UserObject;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for UserObject {}
}

glib::wrapper! {
    pub struct UserObject(ObjectSubclass<imp::UserObject>);
}

impl UserObject {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_from_info(&self, info: &UserInfo) {
        let imp = self.imp();
        *imp.uid.borrow_mut() = info.uid;
        *imp.username.borrow_mut() = info.username.clone();
        *imp.session_count.borrow_mut() = info.session_count;
        *imp.cpu_percent.borrow_mut() = info.cpu_percent;
        *imp.memory_bytes.borrow_mut() = info.memory_bytes;
        *imp.process_count.borrow_mut() = info.process_count;
    }

    pub fn uid(&self) -> u32 {
        *self.imp().uid.borrow()
    }
    pub fn username(&self) -> String {
        self.imp().username.borrow().clone()
    }
    pub fn session_count(&self) -> u32 {
        *self.imp().session_count.borrow()
    }
    pub fn cpu_percent(&self) -> f64 {
        *self.imp().cpu_percent.borrow()
    }
    pub fn memory_bytes(&self) -> u64 {
        *self.imp().memory_bytes.borrow()
    }
    pub fn process_count(&self) -> u32 {
        *self.imp().process_count.borrow()
    }
}

pub struct UsersTab {
    pub widget: gtk::Box,
    store: gio::ListStore,
}

impl UsersTab {
    pub fn new() -> Self {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.add_css_class("users-view");

        // List store for user objects
        let store = gio::ListStore::new::<UserObject>();

        // Sort model (default: sort by CPU descending)
        let sorter = gtk::CustomSorter::new(move |a, b| {
            let ua = a.downcast_ref::<UserObject>().unwrap();
            let ub = b.downcast_ref::<UserObject>().unwrap();
            ub.cpu_percent()
                .partial_cmp(&ua.cpu_percent())
                .unwrap_or(std::cmp::Ordering::Equal)
                .into()
        });
        let sort_model = gtk::SortListModel::new(Some(store.clone()), Some(sorter.clone()));

        // Selection model
        let selection = gtk::SingleSelection::new(Some(sort_model.clone()));
        selection.set_autoselect(false);

        // ColumnView
        let column_view = gtk::ColumnView::new(Some(selection.clone()));
        column_view.set_show_column_separators(true);
        column_view.set_show_row_separators(false);

        // --- Columns ---

        // User column
        let user_factory = gtk::SignalListItemFactory::new();
        user_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::Start);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            item.set_child(Some(&label));
        });
        user_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<UserObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&obj.username());
        });
        let user_col = gtk::ColumnViewColumn::new(Some("User"), Some(user_factory));
        user_col.set_expand(true);
        user_col.set_resizable(true);
        let user_sorter = gtk::CustomSorter::new(|a, b| {
            let ua = a.downcast_ref::<UserObject>().unwrap();
            let ub = b.downcast_ref::<UserObject>().unwrap();
            ua.username()
                .to_lowercase()
                .cmp(&ub.username().to_lowercase())
                .into()
        });
        user_col.set_sorter(Some(&user_sorter));
        column_view.append_column(&user_col);

        // Processes column
        let proc_factory = gtk::SignalListItemFactory::new();
        proc_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::End);
            item.set_child(Some(&label));
        });
        proc_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<UserObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&obj.process_count().to_string());
        });
        let proc_col = gtk::ColumnViewColumn::new(Some("Processes"), Some(proc_factory));
        proc_col.set_fixed_width(80);
        proc_col.set_resizable(true);
        let proc_sorter = gtk::CustomSorter::new(|a, b| {
            let ua = a.downcast_ref::<UserObject>().unwrap();
            let ub = b.downcast_ref::<UserObject>().unwrap();
            ua.process_count().cmp(&ub.process_count()).into()
        });
        proc_col.set_sorter(Some(&proc_sorter));
        column_view.append_column(&proc_col);

        // CPU % column
        let cpu_factory = gtk::SignalListItemFactory::new();
        cpu_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::End);
            item.set_child(Some(&label));
        });
        cpu_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<UserObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&util::format_percent(obj.cpu_percent()));
        });
        let cpu_col = gtk::ColumnViewColumn::new(Some("CPU"), Some(cpu_factory));
        cpu_col.set_fixed_width(80);
        cpu_col.set_resizable(true);
        let cpu_sorter = gtk::CustomSorter::new(|a, b| {
            let ua = a.downcast_ref::<UserObject>().unwrap();
            let ub = b.downcast_ref::<UserObject>().unwrap();
            ua.cpu_percent()
                .partial_cmp(&ub.cpu_percent())
                .unwrap_or(std::cmp::Ordering::Equal)
                .into()
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
            let obj = item.item().and_downcast::<UserObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&util::format_bytes(obj.memory_bytes()));
        });
        let mem_col = gtk::ColumnViewColumn::new(Some("Memory"), Some(mem_factory));
        mem_col.set_fixed_width(100);
        mem_col.set_resizable(true);
        let mem_sorter = gtk::CustomSorter::new(|a, b| {
            let ua = a.downcast_ref::<UserObject>().unwrap();
            let ub = b.downcast_ref::<UserObject>().unwrap();
            ua.memory_bytes().cmp(&ub.memory_bytes()).into()
        });
        mem_col.set_sorter(Some(&mem_sorter));
        column_view.append_column(&mem_col);

        // Sessions column
        let sess_factory = gtk::SignalListItemFactory::new();
        sess_factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::new(None);
            label.set_halign(gtk::Align::End);
            item.set_child(Some(&label));
        });
        sess_factory.connect_bind(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<UserObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_text(&obj.session_count().to_string());
        });
        let sess_col = gtk::ColumnViewColumn::new(Some("Sessions"), Some(sess_factory));
        sess_col.set_fixed_width(80);
        sess_col.set_resizable(true);
        let sess_sorter = gtk::CustomSorter::new(|a, b| {
            let ua = a.downcast_ref::<UserObject>().unwrap();
            let ub = b.downcast_ref::<UserObject>().unwrap();
            ua.session_count().cmp(&ub.session_count()).into()
        });
        sess_col.set_sorter(Some(&sess_sorter));
        column_view.append_column(&sess_col);

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

        // Context menu - Log Off User
        let menu = gio::Menu::new();
        menu.append(Some("Log Off User"), Some("user.logoff"));

        let popover = gtk::PopoverMenu::from_model(Some(&menu));
        popover.set_parent(&column_view);
        popover.set_has_arrow(false);

        // Action group
        let action_group = gio::SimpleActionGroup::new();

        let sel_clone = selection.clone();
        let cv_ref = column_view.clone();
        let logoff_action = gio::SimpleAction::new("logoff", None);
        logoff_action.connect_activate(move |_, _| {
            if let Some(obj) = sel_clone
                .selected_item()
                .and_then(|i| i.downcast::<UserObject>().ok())
            {
                let username = obj.username();
                show_logoff_confirm_dialog(&cv_ref, &username);
            }
        });
        action_group.add_action(&logoff_action);

        column_view.insert_action_group("user", Some(&action_group));

        // Right-click gesture
        let gesture = gtk::GestureClick::new();
        gesture.set_button(3); // Right click
        let popover_clone = popover.clone();
        gesture.connect_pressed(move |gesture, _, x, y| {
            popover_clone
                .set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover_clone.popup();
            gesture.set_state(gtk::EventSequenceState::Claimed);
        });
        column_view.add_controller(gesture);

        Self { widget, store }
    }

    pub fn update(&mut self, snapshot: &SystemSnapshot) {
        let user_infos = users::collect_users(&snapshot.processes);
        let new_count = user_infos.len();
        let old_count = self.store.n_items() as usize;

        // Reuse existing objects where possible, add/remove as needed
        for (i, info) in user_infos.iter().enumerate() {
            if i < old_count {
                if let Some(obj) = self
                    .store
                    .item(i as u32)
                    .and_then(|o| o.downcast::<UserObject>().ok())
                {
                    obj.set_from_info(info);
                }
            } else {
                let obj = UserObject::new();
                obj.set_from_info(info);
                self.store.append(&obj);
            }
        }

        // Remove extras
        if old_count > new_count {
            self.store.splice(
                new_count as u32,
                (old_count - new_count) as u32,
                &[] as &[UserObject],
            );
        }

        // Notify the sort model that items changed
        self.store.items_changed(0, 0, 0);
    }
}

fn show_logoff_confirm_dialog(widget: &gtk::ColumnView, username: &str) {
    let window = widget
        .root()
        .and_then(|r| r.downcast::<gtk::Window>().ok());

    let msg = format!(
        "Are you sure you want to log off user \"{}\"?\n\n\
         This will terminate all their sessions and running processes.",
        username
    );

    let dialog = gtk::MessageDialog::new(
        window.as_ref(),
        gtk::DialogFlags::MODAL | gtk::DialogFlags::DESTROY_WITH_PARENT,
        gtk::MessageType::Warning,
        gtk::ButtonsType::None,
        &msg,
    );
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    let logoff_btn = dialog.add_button("Log Off", gtk::ResponseType::Accept);
    logoff_btn.add_css_class("destructive-action");

    let username_owned = username.to_string();
    dialog.connect_response(move |d, response| {
        if response == gtk::ResponseType::Accept {
            let name = username_owned.clone();
            std::thread::spawn(move || {
                match users::logoff_user(&name) {
                    Ok(_) => {
                        log::info!("Logged off user: {}", name);
                    }
                    Err(e) => {
                        log::error!("Failed to log off user {}: {}", name, e);
                    }
                }
            });
        }
        d.close();
    });
    dialog.present();
}

fn show_error_dialog(widget: &gtk::ColumnView, message: &str) {
    let window = widget
        .root()
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
