#[cfg(test)]
pub mod gtk_test;
pub mod nav;
pub mod pages;
pub mod widgets;
pub mod window;

/// Compiled into the binary so the theme also loads on an installed system.
/// The plain GTK4 build follows the desktop theme; the libadwaita build adds its dark palette.
#[cfg(feature = "adwaita")]
pub const STYLE_CSS: &str = concat!(
    include_str!("style-adwaita.css"),
    "\n",
    include_str!("style.css")
);
#[cfg(not(feature = "adwaita"))]
pub const STYLE_CSS: &str = include_str!("style.css");

#[derive(Debug, Clone, Default)]
pub struct SysData {
    pub efi: bool,
    pub net: bool,
    /// The login manager the live image runs, which the Login manager menu starts on.
    pub display_manager: Option<String>,
    /// The live image's packages: what it ships tells its desktop, and what an offline install can use.
    pub packages: std::collections::HashSet<String>,
    /// The repository URLs the live image's xbps.d configures (some desktops need a particular one).
    pub repositories: Vec<String>,
    /// The Welcome page's system check; a failed required one blocks the installation.
    pub checks: Vec<crate::backend::requirements::Check>,
}

#[cfg(test)]
mod tests {
    use super::STYLE_CSS;

    #[test]
    fn stylesheet_is_embedded_and_styles_the_sidebar() {
        assert!(STYLE_CSS.contains(".voyage-sidebar-list"));
    }

    #[cfg(feature = "adwaita")]
    #[test]
    fn adwaita_build_defines_the_dark_palette() {
        assert!(STYLE_CSS.contains("@define-color window_bg_color"));
        assert!(STYLE_CSS.contains("@define-color accent_bg_color"));
    }

    #[cfg(not(feature = "adwaita"))]
    #[test]
    fn plain_build_leaves_colors_to_the_gtk_theme() {
        assert!(!STYLE_CSS.contains("@define-color"));
    }

    #[test]
    fn main_does_not_load_the_stylesheet_from_the_build_directory() {
        let main_rs = include_str!("../main.rs");
        assert!(
            !main_rs.contains("CARGO_MANIFEST_DIR"),
            "main.rs must not read style.css from a build-time path: it does not exist on an installed system"
        );
    }

    #[test]
    fn checkout_icon_is_available_by_name() {
        super::gtk_test::run("checkout icon", || {
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/images");
            let display = gtk::gdk::Display::default().expect("GTK test has a display");
            let theme = gtk::IconTheme::for_display(&display);
            theme.add_search_path(dir);
            assert!(theme.has_icon("org.voidlinux.voyage"));
        });
    }
}
