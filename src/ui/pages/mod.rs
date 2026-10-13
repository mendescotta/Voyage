pub mod completion;
pub mod disks;
pub mod installation;
pub mod mirrors;
pub mod review;
pub mod users;
pub mod welcome;

use gtk::prelude::*;

/// Groups wrap into one column when the content area cannot fit two readable forms.
pub fn group_columns() -> gtk::FlowBox {
    gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .min_children_per_line(1)
        .max_children_per_line(2)
        .column_spacing(16)
        .row_spacing(16)
        .homogeneous(false)
        .hexpand(true)
        .css_classes(["voyage-columns"])
        .build()
}

pub fn add_column(columns: &gtk::FlowBox, group: &gtk::Widget) {
    group.set_width_request(290);
    group.set_hexpand(true);
    columns.insert(group, -1);
}
