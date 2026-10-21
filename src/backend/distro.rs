//! What the UI needs from `distro.conf` (the installer's distro policy; see resources/backend/distro_config.sh,
//! which is the authority: it validates the file at install time). Here it only decides what to offer.

use std::fs;
use std::path::Path;

use super::paths::backend_dir;

/// The file shipped with the installer; also the fallback when nothing can be read from disk.
const SHIPPED: &str = include_str!("../../resources/backend/distro.conf");
const OVERRIDE: &str = "/etc/voyage/distro.conf";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shell {
    pub name: String,
    pub path: String,
    pub package: Option<String>,
}

impl Shell {
    /// "Bash", "Zsh"... for the combo box.
    pub fn label(&self) -> String {
        let mut chars = self.name.chars();
        match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => String::new(),
        }
    }

    /// True when the live image already has it, so an offline install can still use it.
    pub fn available_offline(&self) -> bool {
        self.package.is_none() || Path::new(&self.path).exists()
    }
}

/// A desktop the installer can put on the new system (`desktop` and `desktop-packages` lines).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Desktop {
    pub name: String,
    /// Its session's .desktop name, which the login manager starts.
    pub session: String,
    /// The login manager it comes with; "none" for none.
    pub login_manager: String,
    pub label: String,
    /// The first one tells whether a system has the desktop.
    pub packages: Vec<String>,
    /// Offered only when a configured repository URL contains this word.
    pub repository: Option<String>,
}

impl Desktop {
    pub fn marker(&self) -> Option<&str> {
        self.packages.first().map(String::as_str)
    }
}

/// A login manager (`login-manager` and `login-manager-needs` lines); its name is also its service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginManager {
    pub name: String,
    pub packages: Vec<String>,
    /// Offered only when the new system has one of these desktops; empty means with any desktop.
    pub needs: Vec<String>,
}

impl LoginManager {
    pub fn marker(&self) -> Option<&str> {
        self.packages.first().map(String::as_str)
    }

    /// "LightDM", "GDM"... for the combo box.
    pub fn label(&self) -> String {
        match self.name.as_str() {
            "lightdm" => "LightDM".into(),
            "gdm" => "GDM".into(),
            "sddm" => "SDDM".into(),
            "cosmic-greeter" => "COSMIC Greeter".into(),
            other => other.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DistroConf {
    pub shells: Vec<Shell>,
    pub default_shell: String,
    pub desktops: Vec<Desktop>,
    pub login_managers: Vec<LoginManager>,
}

/// A shell, desktop or login manager name as the backend accepts it: lowercase letters, digits and dashes,
/// starting with a letter.
pub fn name_valid(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

impl DistroConf {
    pub fn parse(text: &str) -> Self {
        let mut conf = DistroConf::default();
        // these may come before the line that defines their name, so they are attached at the end
        let mut desktop_packages: Vec<(String, Vec<String>)> = Vec::new();
        let mut needs: Vec<(String, Vec<String>)> = Vec::new();
        let mut repositories: Vec<(String, String)> = Vec::new();
        let words_from =
            |words: &[&str]| words.iter().map(|w| (*w).to_string()).collect::<Vec<_>>();
        for line in text.lines() {
            let line = line.split('#').next().unwrap_or("");
            let words: Vec<&str> = line.split_whitespace().collect();
            match words.as_slice() {
                ["shell", name, path] | ["shell", name, path, _]
                    if name_valid(name) && path.starts_with('/') =>
                {
                    conf.shells.push(Shell {
                        name: (*name).to_string(),
                        path: (*path).to_string(),
                        package: words.get(3).map(|p| (*p).to_string()),
                    });
                }
                ["default-shell", name] => conf.default_shell = (*name).to_string(),
                ["desktop", name, session, login_manager, label @ ..]
                    if !label.is_empty()
                        && name_valid(name)
                        && *name != "none"
                        && name_valid(login_manager) =>
                {
                    conf.desktops.push(Desktop {
                        name: (*name).to_string(),
                        session: (*session).to_string(),
                        login_manager: (*login_manager).to_string(),
                        label: label.join(" "),
                        packages: Vec::new(),
                        repository: None,
                    });
                }
                ["desktop-packages", name, packages @ ..] if !packages.is_empty() => {
                    desktop_packages.push(((*name).to_string(), words_from(packages)));
                }
                ["login-manager", name, packages @ ..]
                    if !packages.is_empty() && name_valid(name) && *name != "none" =>
                {
                    conf.login_managers.push(LoginManager {
                        name: (*name).to_string(),
                        packages: words_from(packages),
                        needs: Vec::new(),
                    });
                }
                ["desktop-repository", name, word] => {
                    repositories.push(((*name).to_string(), (*word).to_string()));
                }
                ["login-manager-needs", name, desktops @ ..] if !desktops.is_empty() => {
                    needs.push(((*name).to_string(), words_from(desktops)));
                }
                _ => {}
            }
        }
        for (name, packages) in desktop_packages {
            if let Some(d) = conf.desktops.iter_mut().find(|d| d.name == name) {
                d.packages.extend(packages);
            }
        }
        for (name, word) in repositories {
            if let Some(d) = conf.desktops.iter_mut().find(|d| d.name == name) {
                d.repository = Some(word);
            }
        }
        for (name, desktops) in needs {
            if let Some(m) = conf.login_managers.iter_mut().find(|m| m.name == name) {
                m.needs.extend(desktops);
            }
        }
        // a desktop nothing can tell apart or install is of no use to the installer
        conf.desktops.retain(|d| !d.packages.is_empty());
        if conf.shells.is_empty() {
            conf.shells.push(Shell {
                name: "bash".into(),
                path: "/bin/bash".into(),
                package: None,
            });
        }
        if !conf.shells.iter().any(|s| s.name == conf.default_shell) {
            conf.default_shell = conf.shells[0].name.clone();
        }
        conf
    }

    /// The override in the live image, else the file next to the scripts, else the shipped text.
    pub fn load() -> Self {
        for path in [
            Path::new(OVERRIDE).to_path_buf(),
            backend_dir().join("distro.conf"),
        ] {
            if let Ok(text) = fs::read_to_string(&path) {
                return Self::parse(&text);
            }
        }
        Self::parse(SHIPPED)
    }

    /// Position of the default shell in `shells` (what a combo box starts on).
    pub fn default_index(&self) -> usize {
        self.shells
            .iter()
            .position(|s| s.name == self.default_shell)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_file_offers_bash_zsh_and_fish_with_bash_first() {
        let conf = DistroConf::parse(SHIPPED);
        let names: Vec<&str> = conf.shells.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["bash", "zsh", "fish"]);
        assert_eq!(conf.default_shell, "bash");
        assert_eq!(conf.default_index(), 0);
    }

    #[test]
    fn paths_and_packages_are_read() {
        let conf = DistroConf::parse(SHIPPED);
        assert_eq!(conf.shells[0].package, None);
        assert_eq!(conf.shells[1].path, "/usr/bin/zsh");
        assert_eq!(conf.shells[1].package.as_deref(), Some("zsh"));
        assert_eq!(conf.shells[2].package.as_deref(), Some("fish-shell"));
    }

    #[test]
    fn comments_and_unrelated_lines_are_ignored() {
        let conf = DistroConf::parse(
            "# shell evil /bin/evil\nremove voyage\nshell zsh /usr/bin/zsh zsh  # trailing\ndefault-shell zsh\n",
        );
        assert_eq!(conf.shells.len(), 1);
        assert_eq!(conf.shells[0].name, "zsh");
        assert_eq!(conf.default_shell, "zsh");
    }

    #[test]
    fn malformed_shell_lines_are_skipped_like_the_backend_would_reject_them() {
        let conf = DistroConf::parse(
            "shell Fish /usr/bin/fish\nshell fish usr/bin/fish\nshell -x /bin/x\nshell ok /bin/ok\n",
        );
        let names: Vec<&str> = conf.shells.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["ok"]);
    }

    #[test]
    fn a_default_that_is_not_offered_falls_back_to_the_first_shell() {
        let conf = DistroConf::parse("shell a /bin/a\nshell b /bin/b\ndefault-shell nope\n");
        assert_eq!(conf.default_shell, "a");
        let conf = DistroConf::parse("shell a /bin/a\nshell b /bin/b\ndefault-shell b\n");
        assert_eq!(conf.default_index(), 1);
    }

    #[test]
    fn an_empty_or_unusable_file_still_offers_bash() {
        let conf = DistroConf::parse("garbage\n");
        assert_eq!(conf.shells.len(), 1);
        assert_eq!(conf.shells[0].path, "/bin/bash");
        assert_eq!(conf.default_shell, "bash");
    }

    #[test]
    fn labels_are_capitalised() {
        let conf = DistroConf::parse(SHIPPED);
        assert_eq!(conf.shells[0].label(), "Bash");
        assert_eq!(conf.shells[2].label(), "Fish");
    }

    #[test]
    fn a_shell_without_a_package_works_offline_and_one_with_a_missing_binary_does_not() {
        let bash = Shell {
            name: "bash".into(),
            path: "/bin/bash".into(),
            package: None,
        };
        assert!(bash.available_offline());
        let ghost = Shell {
            name: "ghost".into(),
            path: "/nonexistent/ghost".into(),
            package: Some("ghost".into()),
        };
        assert!(!ghost.available_offline());
        let present = Shell {
            name: "sh".into(),
            path: "/bin/sh".into(),
            package: Some("dash".into()),
        };
        assert!(present.available_offline());
    }

    #[test]
    fn the_shipped_file_offers_the_desktops_with_their_packages_and_login_managers() {
        let conf = DistroConf::parse(SHIPPED);
        let names: Vec<&str> = conf.desktops.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "xfce",
                "gnome",
                "gnome-void",
                "cinnamon",
                "cosmic",
                "kde",
                "lxqt",
                "budgie"
            ]
        );
        let kde = &conf.desktops[5];
        assert_eq!(kde.label, "KDE Plasma");
        assert_eq!(kde.session, "plasma");
        assert_eq!(kde.login_manager, "sddm");
        assert_eq!(kde.marker(), Some("plasma-workspace"));
        assert_eq!(kde.repository, None);
        assert_eq!(conf.desktops[4].repository.as_deref(), Some("voidlab"));
        let managers: Vec<&str> = conf
            .login_managers
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(managers, ["lightdm", "gdm", "sddm", "cosmic-greeter"]);
        assert_eq!(conf.login_managers[1].needs, ["gnome", "gnome-void"]);
        assert!(conf.login_managers[0].needs.is_empty());
        for d in &conf.desktops {
            assert!(
                conf.login_managers
                    .iter()
                    .any(|m| m.name == d.login_manager),
                "{} names a login manager the file defines",
                d.name
            );
        }
    }

    #[test]
    fn packages_and_needs_may_come_before_their_definition() {
        let conf = DistroConf::parse(
            "desktop-packages kde plasma-workspace\nlogin-manager-needs sddm kde\n\
             desktop kde plasma sddm KDE\nlogin-manager sddm sddm\n",
        );
        assert_eq!(conf.desktops[0].packages, ["plasma-workspace"]);
        assert_eq!(conf.login_managers[0].needs, ["kde"]);
    }

    #[test]
    fn a_desktop_without_packages_or_called_none_is_not_offered() {
        let conf = DistroConf::parse(
            "desktop kde plasma sddm KDE\ndesktop none x sddm Nothing\ndesktop-packages none x\n",
        );
        assert!(conf.desktops.is_empty());
    }
}
