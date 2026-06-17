//! Overview page (root of the NavigationView).

use adw::prelude::*;

use crate::status::{humanize_checked_at, Status};
use crate::widgets::build_issue_row;

/// Build the overview as an `AdwNavigationPage`. `on_row_click` fires with the
/// issue code when any row (or its Fix button) is activated.
pub fn build_overview_page<F>(status: &Status, on_row_click: F) -> adw::NavigationPage
where
    F: Fn(&str) + Clone + 'static,
{
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&adw::HeaderBar::new());

    let content: gtk::Widget = if status.ok {
        build_healthy(status)
    } else {
        build_issues(status, on_row_click)
    };
    tv.set_content(Some(&content));

    adw::NavigationPage::builder()
        .title("Secure Boot")
        .tag("overview")
        .child(&tv)
        .can_pop(false)
        .build()
}

fn build_healthy(status: &Status) -> gtk::Widget {
    let page = adw::StatusPage::builder()
        .icon_name("security-high-symbolic")
        .title("All clear")
        .description("Your Secure Boot chain is healthy.")
        .vexpand(true)
        .build();

    let footer = gtk::Label::builder()
        .label(format!(
            "Kernel {} · Checked {}",
            status.kernel,
            humanize_checked_at(&status.checked_at)
        ))
        .css_classes(["dim-label", "caption"])
        .margin_bottom(18)
        .build();

    let outer = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();
    outer.append(&page);
    outer.append(&footer);
    outer.upcast()
}

fn build_issues<F>(status: &Status, on_row_click: F) -> gtk::Widget
where
    F: Fn(&str) + Clone + 'static,
{
    let builder =
        gtk::Builder::from_resource("/io/github/gitii/SecurebootWatchdog/screen2_issues.ui");
    let root: gtk::Box = builder.object("screen2_root").expect("screen2_root");
    let group: adw::PreferencesGroup = builder
        .object("issue_list_group")
        .expect("issue_list_group");
    let summary: gtk::Label = builder.object("summary_label").expect("summary_label");
    let footer: gtk::Label = builder.object("footer").expect("footer");

    let n = status.issues.len();
    summary.set_label(&match n {
        1 => "1 issue found".to_string(),
        n => format!("{n} issues found"),
    });
    footer.set_label(&format!(
        "Checked {} · Kernel {}",
        humanize_checked_at(&status.checked_at),
        status.kernel
    ));

    for issue in &status.issues {
        let cb = on_row_click.clone();
        let row = build_issue_row(issue, move |code| cb(code));
        group.add(&row);
    }

    root.upcast()
}
