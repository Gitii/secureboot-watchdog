//! Build an `AdwActionRow` for one `Issue`.
//!
//! Composition rather than a custom subclass — we don't need a new signal,
//! a closure on the Fix button is enough.

use adw::prelude::*;
use gtk::glib;

use crate::status::{is_info_only, Issue, Severity};

const REBOOT_HINT: &str = "Reboot required after fix";
const REBOOT_INFO_HINT: &str = "Complete at next reboot";

/// Build a row. `on_fix` fires when the user clicks the Fix button. For
/// info-only codes the button is replaced with a chevron and `on_fix` is
/// never called.
pub fn build_issue_row<F>(issue: &Issue, on_fix: F) -> adw::ActionRow
where
    F: Fn(&str) + 'static,
{
    let row = adw::ActionRow::builder()
        .title(glib::markup_escape_text(&issue.summary))
        .subtitle(format!(
            "{} · {}",
            issue.severity.caption(),
            subtitle_for(issue)
        ))
        .activatable(false)
        .selectable(false)
        .build();

    // Severity-tinted icon as prefix.
    let icon = gtk::Image::from_icon_name(issue.severity.icon_name());
    icon.set_pixel_size(20);
    icon.add_css_class("severity-icon");
    icon.add_css_class(issue.severity.css_class());
    icon.set_valign(gtk::Align::Center);
    row.add_prefix(&icon);

    // Suffix: Fix button if auto_fixable, chevron otherwise.
    if issue.auto_fixable && !is_info_only(&issue.code) {
        let btn = gtk::Button::with_label("Fix");
        btn.add_css_class("pill");
        btn.add_css_class("suggested-action");
        btn.set_valign(gtk::Align::Center);
        let code = issue.code.clone();
        btn.connect_clicked(move |_| on_fix(&code));
        row.add_suffix(&btn);
    } else {
        let chev = gtk::Image::from_icon_name("go-next-symbolic");
        chev.add_css_class("dim-label");
        chev.set_valign(gtk::Align::Center);
        row.add_suffix(&chev);
        row.set_activatable(true);
        let code = issue.code.clone();
        row.connect_activated(move |_| on_fix(&code));
    }

    row
}

fn subtitle_for(issue: &Issue) -> &'static str {
    match issue.severity {
        Severity::Critical | Severity::Warning if issue.auto_fixable => REBOOT_HINT,
        Severity::Info => REBOOT_INFO_HINT,
        _ => "Manual steps needed",
    }
}
