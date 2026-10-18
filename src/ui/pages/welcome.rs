use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::thread;
use std::time::Duration;

use gtk::glib;
use gtk::prelude::*;

use crate::backend::requirements::{self, Level};
use crate::backend::{locales, system_detect};
use crate::ui::pages::{add_column, group_columns};
use crate::ui::widgets::{ActionRow, Banner, ComboRow, Group};
use crate::ui::SysData;

pub const TITLE: &str = "Welcome";

struct Inner {
    locale_codes: Vec<String>,
    regions: Vec<String>,
    zones: BTreeMap<String, Vec<String>>,
    keymap_codes: Vec<String>,
}

pub struct WelcomePage {
    pub widget: gtk::Box,
    locale_row: ComboRow,
    region_row: ComboRow,
    city_row: ComboRow,
    keymap_row: ComboRow,
    inner: Rc<RefCell<Inner>>,
}

impl WelcomePage {
    pub fn new(sys_data: &SysData) -> Self {
        let widget = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(16)
            .margin_start(16)
            .margin_end(16)
            .build();

        let logo = gtk::Image::builder()
            .pixel_size(64)
            .halign(gtk::Align::Center)
            .build();
        // installed: from the icon theme; run from the source tree: straight from the file
        let in_theme = gtk::gdk::Display::default()
            .map(|d| gtk::IconTheme::for_display(&d).has_icon("org.voidlinux.voyage"))
            .unwrap_or(false);
        match crate::backend::paths::icon_file() {
            Some(file) if !in_theme => logo.set_from_file(Some(file)),
            _ => logo.set_icon_name(Some("org.voidlinux.voyage")),
        }
        widget.append(&logo);

        let heading = gtk::Label::builder()
            .label("Welcome! Let's set up your regional settings")
            .css_classes(["title-1"])
            .wrap(true)
            .build();
        widget.append(&heading);

        let columns = group_columns();
        widget.append(&columns);
        let settings = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .build();

        let check_group = Group::new("System check");
        for check in &sys_data.checks {
            let row = ActionRow::new(&check.label);
            row.set_subtitle(&check.detail);
            row.add_suffix(status_icon(check.level).upcast_ref());
            check_group.add(&row);
        }
        add_column(&columns, check_group.as_ref());
        if let Some(reason) = requirements::blocking_summary(&sys_data.checks) {
            let banner = Banner::new(&format!("The installation cannot start:\n{reason}"));
            widget.append(banner.as_ref());
        }

        let mut locales_list = system_detect::detect_locales();
        if locales_list.is_empty() {
            locales_list.push("en_US.UTF-8".to_string());
        }
        let locale_group = Group::new("Locale");
        let language_names: Vec<String> = locales_list
            .iter()
            .map(|loc| {
                let lang_code = loc
                    .split('.')
                    .next()
                    .unwrap_or(loc)
                    .split('_')
                    .next()
                    .unwrap_or(loc);
                let name = locales::language_name(lang_code);
                if name.is_empty() {
                    loc.clone()
                } else {
                    name
                }
            })
            .collect();
        let locale_row = ComboRow::new("System language", &language_names);
        locale_group.add(&locale_row);
        settings.append(locale_group.as_ref());
        let default_locale_idx = locales_list
            .iter()
            .position(|l| l.starts_with("en_US"))
            .unwrap_or(0);
        locale_row.set_selected(default_locale_idx as u32);

        let mut zones = system_detect::detect_timezones();
        if zones.is_empty() {
            zones.insert("UTC".to_string(), vec!["UTC".to_string()]);
        }
        let regions: Vec<String> = zones.keys().cloned().collect();
        let tz_group = Group::new("Timezone");
        let region_row = ComboRow::new("Region", &regions);
        let first_region_cities = zones.get(&regions[0]).cloned().unwrap_or_default();
        let city_row = ComboRow::new("City", &first_region_cities);
        tz_group.add(&region_row);
        tz_group.add(&city_row);
        settings.append(tz_group.as_ref());

        let mut keymaps = system_detect::detect_keymaps();
        if keymaps.is_empty() {
            keymaps.push("us".to_string());
        }
        let keymap_names: Vec<String> = keymaps.iter().map(|k| locales::keymap_name(k)).collect();
        let keymap_group = Group::new("Keyboard");
        let keymap_row = ComboRow::new("Layout", &keymap_names);
        keymap_group.add(&keymap_row);
        settings.append(keymap_group.as_ref());
        add_column(&columns, settings.upcast_ref());
        let default_keymap_idx = keymaps.iter().position(|k| k == "us").unwrap_or(0);
        keymap_row.set_selected(default_keymap_idx as u32);

        let has_net = sys_data.net;
        let inner = Rc::new(RefCell::new(Inner {
            locale_codes: locales_list,
            regions,
            zones,
            keymap_codes: keymaps,
        }));

        {
            let inner = inner.clone();
            let city_row = city_row.clone();
            region_row.connect_selected(move |row| {
                let inner = inner.borrow();
                let selected = row.selected() as usize;
                let Some(region) = inner.regions.get(selected) else {
                    return;
                };
                let cities = inner.zones.get(region).cloned().unwrap_or_default();
                city_row.set_items(&cities);
            });
        }

        if has_net {
            let (tx, rx) = async_channel::bounded::<Option<String>>(1);
            thread::spawn(move || {
                let tz = system_detect::detect_timezone_from_network(Duration::from_secs(3));
                let _ = tx.send_blocking(tz);
            });
            let inner = inner.clone();
            let region_row = region_row.clone();
            let city_row = city_row.clone();
            glib::spawn_future_local(async move {
                if let Ok(Some(tz)) = rx.recv().await {
                    apply_network_timezone(&inner, &region_row, &city_row, &tz);
                }
            });
        }

        Self {
            widget,
            locale_row,
            region_row,
            city_row,
            keymap_row,
            inner,
        }
    }

    pub fn collect(&self) -> (WelcomeFields, Vec<(String, String)>) {
        let inner = self.inner.borrow();
        let region = inner.regions[self.region_row.selected() as usize].clone();
        let city = inner.zones[&region][self.city_row.selected() as usize].clone();
        let fields = WelcomeFields {
            locale: inner.locale_codes[self.locale_row.selected() as usize].clone(),
            timezone_region: region,
            timezone_city: city,
            keymap: inner.keymap_codes[self.keymap_row.selected() as usize].clone(),
        };
        (fields, Vec::new())
    }
}

fn status_icon(level: Level) -> gtk::DrawingArea {
    let (color, label) = match level {
        Level::Ok => ((0.55, 0.76, 0.49), "System check passed"),
        Level::Warn => ((0.85, 0.65, 0.34), "System check warning"),
        Level::Fail => ((0.92, 0.41, 0.38), "System check failed"),
    };
    let icon = gtk::DrawingArea::builder()
        .content_width(20)
        .content_height(20)
        .valign(gtk::Align::Center)
        .build();
    icon.set_tooltip_text(Some(label));
    icon.set_draw_func(move |_, cr, width, height| {
        let cx = f64::from(width) / 2.0;
        let cy = f64::from(height) / 2.0;
        cr.set_source_rgb(color.0, color.1, color.2);
        cr.set_line_width(2.0);
        cr.set_line_cap(gtk::cairo::LineCap::Round);
        cr.set_line_join(gtk::cairo::LineJoin::Round);

        match level {
            Level::Ok => {
                cr.arc(cx, cy, 8.0, 0.0, std::f64::consts::TAU);
                let _ = cr.stroke();
                cr.move_to(cx - 4.0, cy);
                cr.line_to(cx - 1.0, cy + 3.0);
                cr.line_to(cx + 5.0, cy - 4.0);
                let _ = cr.stroke();
            }
            Level::Warn => {
                cr.move_to(cx, cy - 8.0);
                cr.line_to(cx + 8.0, cy + 7.0);
                cr.line_to(cx - 8.0, cy + 7.0);
                cr.close_path();
                let _ = cr.stroke();
                cr.move_to(cx, cy - 3.0);
                cr.line_to(cx, cy + 2.0);
                let _ = cr.stroke();
                cr.arc(cx, cy + 5.0, 0.7, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
            }
            Level::Fail => {
                cr.arc(cx, cy, 8.0, 0.0, std::f64::consts::TAU);
                let _ = cr.stroke();
                cr.move_to(cx - 3.0, cy - 3.0);
                cr.line_to(cx + 3.0, cy + 3.0);
                cr.move_to(cx + 3.0, cy - 3.0);
                cr.line_to(cx - 3.0, cy + 3.0);
                let _ = cr.stroke();
            }
        }
    });
    icon
}

fn apply_network_timezone(
    inner: &Rc<RefCell<Inner>>,
    region_row: &ComboRow,
    city_row: &ComboRow,
    tz: &str,
) {
    let Some((region, city)) = tz.split_once('/') else {
        return;
    };
    let inner_ref = inner.borrow();
    let Some(region_idx) = inner_ref.regions.iter().position(|r| r == region) else {
        return;
    };
    let Some(cities) = inner_ref.zones.get(region) else {
        return;
    };
    let Some(city_idx) = cities.iter().position(|c| c == city) else {
        return;
    };
    let cities = cities.clone();
    drop(inner_ref);

    region_row.set_selected(region_idx as u32);
    city_row.set_items(&cities);
    city_row.set_selected(city_idx as u32);
}

#[derive(Debug, Clone, Default)]
pub struct WelcomeFields {
    pub locale: String,
    pub timezone_region: String,
    pub timezone_city: String,
    pub keymap: String,
}
