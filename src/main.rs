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

    let app = app::TaskManagerApp::new();
    std::process::exit(app.run());
}
