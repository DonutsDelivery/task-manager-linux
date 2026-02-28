#![allow(unused)]

mod app;
mod backend;
mod config;
mod model;
mod ui;
mod util;
mod window;

const APP_ID: &str = "com.task-manager.linux";
const CSS: &str = include_str!("../style/style.css");

fn main() {
    env_logger::init();

    // If launched as shortcut daemon, run the evdev listener instead of the GUI
    if std::env::args().any(|a| a == "--shortcut-daemon") {
        backend::shortcut_daemon::run_daemon();
    }

    let app = app::TaskManagerApp::new();
    std::process::exit(app.run());
}
