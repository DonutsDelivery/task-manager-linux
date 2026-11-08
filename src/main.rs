#![allow(unused)]

mod app;
mod backend;
mod config;
mod model;
mod ui;
mod util;
mod window;

const APP_ID: &str = "online.donutsdelivery.DonutTaskManager";
const CSS: &str = include_str!("../style/style.css");

fn main() {
    env_logger::init();

    // If launched as shortcut daemon, run the evdev listener instead of the GUI
    if std::env::args().any(|a| a == "--shortcut-daemon") {
        backend::shortcut_daemon::run_daemon();
    }

    // Under X11 the window class comes from the program name. Setting it to the
    // app id makes it match StartupWMClass in the desktop file.
    gtk4::glib::set_prgname(Some(APP_ID));

    let app = app::DonutTaskManagerApp::new();
    std::process::exit(app.run());
}
