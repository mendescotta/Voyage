//! The Desktop group of the Mirror and Software page: desktop, keeping the live image's one, login manager.
//! The menus are rebuilt when the mirror changes (another desktop needs an online mirror) and when the
//! desktop changes (the login managers depend on it).

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use gtk::prelude::*;

use crate::backend::desktop::{self, Choice, NONE};
use crate::backend::distro::DistroConf;
use crate::ui::widgets::{ComboRow, Group, SwitchRow};
use crate::ui::SysData;

/// Told the label of the chosen login manager, or None when there is none.
type LoginManagerHandler = Box<dyn Fn(Option<&str>)>;

struct Inner {
    conf: DistroConf,
    packages: HashSet<String>,
    repositories: Vec<String>,
    live_manager: Option<String>,
    shipped: Option<String>,
    online: bool,
    desktops: Vec<Choice>,
    managers: Vec<Choice>,
    /// The desktop the login manager menu was last built for.
    managers_for: Option<String>,
    on_manager: Option<LoginManagerHandler>,
}

#[derive(Clone)]
pub struct DesktopGroup {
    pub group: Group,
    desktop_row: ComboRow,
    keep_row: SwitchRow,
    manager_row: ComboRow,
    hint: gtk::Label,
    inner: Rc<RefCell<Inner>>,
}

/// Keeping the live image's desktop is a question only when another desktop is chosen.
fn keep_offered(shipped: Option<&str>, desktop: &str) -> bool {
    shipped.is_some_and(|s| s != desktop) && desktop != NONE
}

pub struct DesktopFields {
    pub desktop: String,
    pub keep_desktop: bool,
    pub login_manager: String,
}

impl DesktopGroup {
    pub fn new(sys_data: &SysData, online: bool) -> Self {
        let conf = DistroConf::load();
        let shipped = desktop::shipped(&conf, &sys_data.packages).map(|d| d.name.clone());
        let group = Group::new("Desktop");
        let desktop_row = ComboRow::new("Desktop", &[]);
        let shipped_label = shipped
            .as_deref()
            .and_then(|s| conf.desktops.iter().find(|d| d.name == s))
            .map(|d| d.label.clone())
            .unwrap_or_default();
        let keep_row = SwitchRow::new(&format!("Keep {shipped_label} too"));
        keep_row.set_subtitle(&format!(
            "Off: {shipped_label} is removed and only the chosen desktop is installed."
        ));
        let manager_row = ComboRow::new("Login manager", &[]);
        group.add(&desktop_row);
        group.add(&keep_row);
        group.add(&manager_row);
        let hint = gtk::Label::builder()
            .css_classes(["dim-label"])
            .wrap(true)
            .halign(gtk::Align::Start)
            .build();

        let this = Self {
            group,
            desktop_row,
            keep_row,
            manager_row,
            hint,
            inner: Rc::new(RefCell::new(Inner {
                conf,
                packages: sys_data.packages.clone(),
                repositories: sys_data.repositories.clone(),
                live_manager: sys_data.display_manager.clone(),
                shipped,
                online,
                desktops: Vec::new(),
                managers: Vec::new(),
                managers_for: None,
                on_manager: None,
            })),
        };
        this.refresh_desktops();
        {
            let this2 = this.clone();
            this.desktop_row
                .connect_selected(move |_| this2.refresh_managers());
        }
        {
            let this2 = this.clone();
            this.keep_row
                .connect_active(move |_| this2.refresh_managers());
        }
        {
            let this2 = this.clone();
            this.manager_row
                .connect_selected(move |_| this2.notify_manager());
        }
        this
    }

    /// The note under the page: what offline or a local mirror leaves out.
    pub fn hint(&self) -> &gtk::Label {
        &self.hint
    }

    /// Another desktop and login managers the live image lacks need an online mirror.
    pub fn set_online(&self, online: bool) {
        if self.inner.borrow().online != online {
            self.inner.borrow_mut().online = online;
            self.refresh_desktops();
        }
    }

    /// `f` is called now and whenever the login manager changes (Users uses it for autologin).
    pub fn connect_login_manager_changed(&self, f: impl Fn(Option<&str>) + 'static) {
        self.inner.borrow_mut().on_manager = Some(Box::new(f));
        self.notify_manager();
    }

    pub fn collect(&self) -> DesktopFields {
        let inner = self.inner.borrow();
        let pick = |choices: &[Choice], row: &ComboRow| {
            choices
                .get(row.selected() as usize)
                .map(|c| c.name.clone())
                .unwrap_or_else(|| NONE.to_string())
        };
        let desktop = pick(&inner.desktops, &self.desktop_row);
        DesktopFields {
            keep_desktop: keep_offered(inner.shipped.as_deref(), &desktop)
                && self.keep_row.is_active(),
            login_manager: pick(&inner.managers, &self.manager_row),
            desktop,
        }
    }

    /// None before the menu is first filled.
    fn selected_desktop(&self) -> Option<String> {
        let inner = self.inner.borrow();
        inner
            .desktops
            .get(self.desktop_row.selected() as usize)
            .map(|c| c.name.clone())
    }

    fn refresh_desktops(&self) {
        let previous = self.selected_desktop();
        let (labels, index, hint) = {
            let mut inner = self.inner.borrow_mut();
            inner.desktops = desktop::desktop_choices(
                &inner.conf,
                &inner.packages,
                &inner.repositories,
                inner.online,
            );
            let labels: Vec<String> = inner.desktops.iter().map(|c| c.label.clone()).collect();
            // the user's pick stays while it is offered; the menu starts on the shipped desktop
            let index = previous
                .and_then(|p| inner.desktops.iter().position(|c| c.name == p))
                .unwrap_or(0);
            let hint = if inner.online {
                String::new()
            } else {
                "Other desktops and login managers need the internet and a mirror other than the local ISO."
                    .to_string()
            };
            (labels, index, hint)
        };
        self.desktop_row.set_items(&labels);
        self.desktop_row.set_selected(index as u32);
        self.hint.set_label(&hint);
        self.hint.set_visible(!hint.is_empty());
        self.refresh_managers();
    }

    fn refresh_managers(&self) {
        let desktop = self.selected_desktop().unwrap_or_else(|| NONE.to_string());
        let previous = {
            let inner = self.inner.borrow();
            inner
                .managers
                .get(self.manager_row.selected() as usize)
                .map(|c| c.name.clone())
        };
        let (labels, index, keep_visible) = {
            let mut inner = self.inner.borrow_mut();
            let shipped = inner.shipped.clone();
            let keep_visible = keep_offered(shipped.as_deref(), &desktop);
            let keep = keep_visible && self.keep_row.is_active();
            let on_system = desktop::system_desktops(shipped.as_deref(), &desktop, keep);
            inner.managers = desktop::login_manager_choices(
                &inner.conf,
                &inner.packages,
                inner.online,
                &on_system,
            );
            let labels: Vec<String> = inner.managers.iter().map(|c| c.label.clone()).collect();
            let default = desktop::default_login_manager(
                &inner.conf,
                &inner.managers,
                &desktop,
                shipped.as_deref(),
                inner.live_manager.as_deref(),
            );
            // a login manager the user picked stays while it still fits; another desktop brings its own
            let same_desktop = inner.managers_for.as_deref() == Some(desktop.as_str());
            let index = previous
                .filter(|_| same_desktop)
                .and_then(|p| inner.managers.iter().position(|c| c.name == p))
                .unwrap_or(default);
            inner.managers_for = Some(desktop.clone());
            (labels, index, keep_visible)
        };
        self.keep_row.set_visible(keep_visible);
        self.manager_row.set_items(&labels);
        self.manager_row.set_selected(index as u32);
        self.notify_manager();
    }

    fn notify_manager(&self) {
        let inner = self.inner.borrow();
        let Some(f) = inner.on_manager.as_ref() else {
            return;
        };
        match inner.managers.get(self.manager_row.selected() as usize) {
            Some(c) if c.name != NONE => f(Some(&c.label)),
            _ => f(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xfce_live() -> SysData {
        SysData {
            packages: ["xfce4-session", "lightdm"]
                .iter()
                .map(|p| p.to_string())
                .collect(),
            display_manager: Some("lightdm".into()),
            repositories: vec!["https://example.org/voidlab-testing/repo".into()],
            ..SysData::default()
        }
    }

    fn keep_visible(group: &DesktopGroup) -> bool {
        AsRef::<gtk::Widget>::as_ref(&group.keep_row).is_visible()
    }

    fn select(group: &DesktopGroup, name: &str) {
        let index = group
            .inner
            .borrow()
            .desktops
            .iter()
            .position(|c| c.name == name)
            .unwrap();
        group.desktop_row.set_selected(index as u32);
    }

    #[test]
    fn another_desktop_online_brings_its_login_manager_and_the_keep_question() {
        crate::ui::gtk_test::run("the Desktop group test", || {
            let group = DesktopGroup::new(&xfce_live(), true);
            let seen = Rc::new(RefCell::new(Vec::<Option<String>>::new()));
            {
                let seen = seen.clone();
                group.connect_login_manager_changed(move |l| {
                    seen.borrow_mut().push(l.map(String::from))
                });
            }
            let f = group.collect();
            assert_eq!(
                (f.desktop.as_str(), f.login_manager.as_str()),
                ("xfce", "lightdm")
            );
            assert!(!keep_visible(&group));

            select(&group, "kde");
            let f = group.collect();
            assert_eq!(
                (f.desktop.as_str(), f.login_manager.as_str()),
                ("kde", "sddm")
            );
            assert!(keep_visible(&group));
            assert!(
                !f.keep_desktop,
                "the live image's desktop is replaced unless kept"
            );
            group.keep_row.set_active(true);
            assert!(group.collect().keep_desktop);

            select(&group, NONE);
            let f = group.collect();
            assert_eq!(f.login_manager, NONE);
            assert!(!f.keep_desktop);
            assert_eq!(
                seen.borrow().last().unwrap(),
                &None,
                "Users hears there is no login manager"
            );
        });
    }

    #[test]
    fn going_offline_falls_back_to_what_the_live_image_has() {
        crate::ui::gtk_test::run("the Desktop group test", || {
            let group = DesktopGroup::new(&xfce_live(), true);
            select(&group, "cosmic");
            assert_eq!(group.collect().login_manager, "cosmic-greeter");
            group.set_online(false);
            let f = group.collect();
            assert_eq!(
                (f.desktop.as_str(), f.login_manager.as_str()),
                ("xfce", "lightdm")
            );
            assert!(group.hint().is_visible());
        });
    }
}
