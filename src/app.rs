use gtk4 as gtk;
use gtk::prelude::*;
use libadwaita as adw;
use adw::prelude::*;

use crate::window::MainWindow;
use crate::CSS;
use crate::APP_ID;

pub struct TaskManagerApp {
    app: adw::Application,
}

impl TaskManagerApp {
    pub fn new() -> Self {
        let app = adw::Application::builder()
            .application_id(APP_ID)
            .build();

        app.connect_startup(|_| {
            load_css();
        });

        app.connect_activate(|app| {
            if let Some(window) = app.active_window() {
                window.present();
                return;
            }
            let window = MainWindow::new(app);
            window.present();
        });

        Self { app }
    }

    pub fn run(&self) -> i32 {
        self.app.run().into()
    }
}

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);

    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("Could not get default display"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}
