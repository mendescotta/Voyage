mod backend;
mod renderer;
mod ui;

use std::env;

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;

use backend::{requirements, system_detect};
use ui::SysData;

fn load_theme() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(ui::STYLE_CSS);
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn load_app_icon() {
    if let (Some(display), Some(icon)) = (gdk::Display::default(), backend::paths::icon_file()) {
        let theme = gtk::IconTheme::for_display(&display);
        if !theme.has_icon("org.voidlinux.voyage") {
            if let Some(dir) = icon.parent() {
                theme.add_search_path(dir);
            }
        }
    }
    gtk::Window::set_default_icon_name("org.voidlinux.voyage");
}

fn detect_system_data() -> SysData {
    let net = system_detect::has_internet(std::time::Duration::from_secs(3));
    let checks = requirements::evaluate(&requirements::Facts::detect(net));
    SysData {
        efi: system_detect::detect_efi(),
        net,
        display_manager: system_detect::detect_display_manager(),
        packages: system_detect::installed_packages(),
        repositories: system_detect::configured_repositories(),
        checks,
    }
}

fn main() -> glib::ExitCode {
    renderer::configure_at_startup();
    let demo = env::args().any(|a| a == "--demo");

    glib::set_prgname(Some("voyage"));
    ui::widgets::init();
    let app = ui::widgets::new_app("org.voidlinux.Voyage");

    app.connect_activate(move |app| {
        load_app_icon();
        load_theme();
        let sys_data = detect_system_data();
        let window = ui::window::build(app, sys_data, demo);
        window.present();
    });

    app.run_with_args::<&str>(&[])
}
