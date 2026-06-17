//! Details page (info-only or pre-fix). Pushed onto the NavigationView.

use adw::prelude::*;

use crate::status::{is_info_only, Issue};

/// Build the details `AdwNavigationPage`. `on_fix` fires when the user clicks
/// "Fix now" (only relevant for fixable codes).
pub fn build_details_page<F>(issue: Issue, on_fix: F) -> adw::NavigationPage
where
    F: Fn(Issue) + 'static,
{
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&adw::HeaderBar::new());

    let builder =
        gtk::Builder::from_resource("/io/github/gitii/SecurebootWatchdog/screen3_details.ui");
    let root: gtk::Box = builder.object("screen3_root").expect("screen3_root");
    let icon: gtk::Image = builder.object("severity_icon").expect("severity_icon");
    let headline: gtk::Label = builder.object("headline").expect("headline");
    let severity_cap: gtk::Label = builder
        .object("severity_caption")
        .expect("severity_caption");
    let detail_view: gtk::TextView = builder.object("detail_view").expect("detail_view");
    let dismiss_btn: gtk::Button = builder.object("dismiss_btn").expect("dismiss_btn");
    let fix_btn: gtk::Button = builder.object("fix_btn").expect("fix_btn");

    icon.set_icon_name(Some(issue.severity.icon_name()));
    for cls in ["severity-critical", "severity-warning", "severity-info"] {
        icon.remove_css_class(cls);
    }
    icon.add_css_class(issue.severity.css_class());

    headline.set_label(&issue.summary);
    severity_cap.set_label(issue.severity.caption());
    detail_view.buffer().set_text(&issue.detail);

    let fixable = issue.auto_fixable && !is_info_only(&issue.code);
    fix_btn.set_visible(fixable);
    dismiss_btn.set_label(if fixable { "Cancel" } else { "Got it" });

    let issue_for_fix = issue.clone();
    fix_btn.connect_clicked(move |_| on_fix(issue_for_fix.clone()));

    tv.set_content(Some(&root));

    let title = issue.code.clone();
    let page = adw::NavigationPage::builder()
        .title(&title)
        .tag("details")
        .child(&tv)
        .build();
    let page_for_dismiss = page.clone();
    dismiss_btn.connect_clicked(move |_| {
        if let Some(parent) = page_for_dismiss
            .parent()
            .and_then(|p| p.downcast::<adw::NavigationView>().ok())
        {
            parent.pop();
        }
    });
    page
}
