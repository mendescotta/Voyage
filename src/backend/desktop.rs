//! What the Desktop and Login manager menus offer. The live image's desktop is copied to the new system;
//! online, another one from distro.conf can take its place or join it; any install can leave it out.
//! backend_install.sh applies the choice and checks it again (`check_desktop_choice`).

use std::collections::HashSet;

use super::distro::{Desktop, DistroConf};

/// The name the installer uses for "no desktop" and "no login manager".
pub const NONE: &str = "none";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub name: String,
    pub label: String,
}

impl Choice {
    fn new(name: &str, label: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            label: label.into(),
        }
    }
}

/// The desktop the live image has: the first one whose first package is installed (the backend's rule).
pub fn shipped<'a>(conf: &'a DistroConf, installed: &HashSet<String>) -> Option<&'a Desktop> {
    conf.desktops
        .iter()
        .find(|d| d.marker().is_some_and(|m| installed.contains(m)))
}

/// The Desktop menu: the shipped desktop, then (online) every other one, then none. A desktop whose
/// first package the live image already has is a variant of the shipped one and is not offered, nor is
/// one that needs a repository the live image does not configure (`repositories`: their URLs).
pub fn desktop_choices(
    conf: &DistroConf,
    installed: &HashSet<String>,
    repositories: &[String],
    online: bool,
) -> Vec<Choice> {
    let mut choices = Vec::new();
    if let Some(d) = shipped(conf, installed) {
        choices.push(Choice::new(
            &d.name,
            format!("{} (on this live image)", d.label),
        ));
    }
    if online {
        for d in &conf.desktops {
            let has_repository = d
                .repository
                .as_ref()
                .is_none_or(|w| repositories.iter().any(|r| r.contains(w.as_str())));
            if has_repository && !d.marker().is_some_and(|m| installed.contains(m)) {
                choices.push(Choice::new(&d.name, d.label.clone()));
            }
        }
    }
    choices.push(Choice::new(NONE, "No desktop (console only)"));
    choices
}

/// The desktops the new system ends up with.
pub fn system_desktops(shipped: Option<&str>, desktop: &str, keep_shipped: bool) -> Vec<String> {
    let mut desktops = Vec::new();
    if desktop != NONE {
        desktops.push(desktop.to_string());
    }
    if let Some(s) = shipped {
        if keep_shipped && desktop != NONE && s != desktop {
            desktops.push(s.to_string());
        }
    }
    desktops
}

/// The Login manager menu for the desktops the new system will have. Offline, only what the live image
/// has can be chosen. No desktop means no login manager.
pub fn login_manager_choices(
    conf: &DistroConf,
    installed: &HashSet<String>,
    online: bool,
    desktops: &[String],
) -> Vec<Choice> {
    let mut choices = Vec::new();
    if !desktops.is_empty() {
        for m in &conf.login_managers {
            let fits = m.needs.is_empty() || m.needs.iter().any(|d| desktops.contains(d));
            let available = online || m.marker().is_some_and(|p| installed.contains(p));
            if fits && available {
                choices.push(Choice::new(&m.name, m.label()));
            }
        }
    }
    choices.push(Choice::new(NONE, "None (console login)"));
    choices
}

/// Where the Login manager menu starts: the login manager the live image runs when its desktop stays,
/// else the one the chosen desktop comes with, else the first choice.
pub fn default_login_manager(
    conf: &DistroConf,
    choices: &[Choice],
    desktop: &str,
    shipped: Option<&str>,
    live_manager: Option<&str>,
) -> usize {
    let position = |name: &str| choices.iter().position(|c| c.name == name);
    if shipped == Some(desktop) {
        if let Some(i) = live_manager.and_then(position) {
            return i;
        }
    }
    conf.desktops
        .iter()
        .find(|d| d.name == desktop)
        .and_then(|d| position(&d.login_manager))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conf() -> DistroConf {
        DistroConf::parse(include_str!("../../resources/backend/distro.conf"))
    }

    fn set(packages: &[&str]) -> HashSet<String> {
        packages.iter().map(|p| (*p).to_string()).collect()
    }

    fn names(choices: &[Choice]) -> Vec<&str> {
        choices.iter().map(|c| c.name.as_str()).collect()
    }

    fn voidlab() -> Vec<String> {
        vec![
            "https://repo-default.voidlinux.org/current".into(),
            "https://github.com/mendescotta/voidlab-testing/releases/download/repo".into(),
        ]
    }

    #[test]
    fn without_the_voidlab_repository_its_desktops_are_not_offered() {
        let void_only = vec!["https://repo-default.voidlinux.org/current".to_string()];
        let choices = desktop_choices(&conf(), &set(XFCE_ISO), &void_only, true);
        let names = names(&choices);
        assert!(
            !names.contains(&"cosmic") && !names.contains(&"gnome"),
            "{names:?}"
        );
        assert!(names.contains(&"gnome-void") && names.contains(&"kde"));
    }

    const XFCE_ISO: &[&str] = &["xfce4-session", "xfce-core", "lightdm", "xorg-minimal"];
    const GNOME_ISO: &[&str] = &["gnome-shell", "gdm"];

    #[test]
    fn the_shipped_desktop_is_found_by_its_first_package() {
        assert_eq!(shipped(&conf(), &set(XFCE_ISO)).unwrap().name, "xfce");
        assert_eq!(shipped(&conf(), &set(GNOME_ISO)).unwrap().name, "gnome");
        assert!(shipped(&conf(), &set(&[])).is_none());
    }

    #[test]
    fn offline_only_the_shipped_desktop_or_none() {
        let choices = desktop_choices(&conf(), &set(XFCE_ISO), &voidlab(), false);
        assert_eq!(names(&choices), ["xfce", "none"]);
        assert_eq!(choices[0].label, "Xfce (on this live image)");
    }

    #[test]
    fn online_every_other_desktop_too() {
        let choices = desktop_choices(&conf(), &set(XFCE_ISO), &voidlab(), true);
        assert_eq!(
            names(&choices),
            [
                "xfce",
                "gnome",
                "gnome-void",
                "cinnamon",
                "cosmic",
                "kde",
                "lxqt",
                "budgie",
                "none"
            ]
        );
    }

    #[test]
    fn on_the_gnome_image_the_stock_gnome_is_a_variant_and_not_offered() {
        let choices = desktop_choices(&conf(), &set(GNOME_ISO), &voidlab(), true);
        assert_eq!(names(&choices)[0], "gnome");
        assert!(!names(&choices).contains(&"gnome-void"));
        assert!(names(&choices).contains(&"xfce"));
    }

    #[test]
    fn an_image_without_a_desktop_offline_offers_only_none() {
        assert_eq!(
            names(&desktop_choices(&conf(), &set(&[]), &voidlab(), false)),
            ["none"]
        );
    }

    #[test]
    fn the_desktops_of_the_new_system() {
        assert_eq!(system_desktops(Some("xfce"), "xfce", false), ["xfce"]);
        assert_eq!(system_desktops(Some("xfce"), "kde", false), ["kde"]);
        assert_eq!(system_desktops(Some("xfce"), "kde", true), ["kde", "xfce"]);
        assert!(system_desktops(Some("xfce"), NONE, true).is_empty());
        assert_eq!(system_desktops(None, "kde", true), ["kde"]);
    }

    #[test]
    fn login_managers_follow_the_desktops() {
        let c = conf();
        let online = |desktops: &[&str]| {
            let d: Vec<String> = desktops.iter().map(|s| s.to_string()).collect();
            names(&login_manager_choices(&c, &set(XFCE_ISO), true, &d))
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        };
        assert_eq!(online(&["kde"]), ["lightdm", "sddm", "none"]);
        assert_eq!(online(&["gnome"]), ["lightdm", "gdm", "sddm", "none"]);
        assert_eq!(
            online(&["cosmic"]),
            ["lightdm", "sddm", "cosmic-greeter", "none"]
        );
        assert_eq!(
            online(&["kde", "gnome-void"]),
            ["lightdm", "gdm", "sddm", "none"]
        );
        assert_eq!(online(&[]), ["none"]);
    }

    #[test]
    fn offline_only_the_login_managers_on_the_live_image() {
        let choices = login_manager_choices(&conf(), &set(XFCE_ISO), false, &["xfce".into()]);
        assert_eq!(names(&choices), ["lightdm", "none"]);
    }

    #[test]
    fn the_login_manager_starts_on_the_live_one_or_the_desktop_s_own() {
        let c = conf();
        let choices = login_manager_choices(&c, &set(XFCE_ISO), true, &["xfce".into()]);
        let pick = |desktop: &str, live: Option<&str>| {
            choices[default_login_manager(&c, &choices, desktop, Some("xfce"), live)]
                .name
                .clone()
        };
        assert_eq!(
            pick("xfce", Some("sddm")),
            "sddm",
            "the live image's choice stays"
        );
        assert_eq!(pick("xfce", None), "lightdm");
        assert_eq!(
            pick("kde", Some("lightdm")),
            "sddm",
            "another desktop brings its own"
        );
        let none = login_manager_choices(&c, &set(XFCE_ISO), true, &[]);
        assert_eq!(
            default_login_manager(&c, &none, NONE, Some("xfce"), None),
            0
        );
    }
}
