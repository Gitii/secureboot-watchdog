//! Single application window holding an `AdwNavigationView`.
//!
//! Pages are pushed/popped in-place: overview → details, overview → fix.
//! No more child dialogs — everything stays in one window with the standard
//! libadwaita back-button affordance.

use adw::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::status::{Issue, Status};
use crate::window::{details, fix_flow, overview};

/// Where to start when the window is created (from CLI subcommand).
pub enum InitialRoute {
    /// Overview only.
    Overview,
    /// Push the details page for this issue on top of overview.
    Details(Issue),
    /// Push the fix-flow page for this issue on top of overview.
    Fix(Issue),
}

pub struct SecurebootWindow;

impl SecurebootWindow {
    pub fn build(
        app: &adw::Application,
        status: Status,
        route: InitialRoute,
    ) -> adw::ApplicationWindow {
        let win = adw::ApplicationWindow::builder()
            .application(app)
            .title("Secure Boot")
            .default_width(560)
            .default_height(560)
            .build();

        let nav = adw::NavigationView::new();
        let nav_for_rows = nav.clone();
        let status_rc = Rc::new(RefCell::new(status.clone()));

        // Root page: overview.
        let app_clone = app.clone();
        let nav_for_routing = nav.clone();
        let status_for_routing = status_rc.clone();
        let overview_page = overview::build_overview_page(&status, move |code| {
            push_for_code(
                &app_clone,
                &nav_for_routing,
                &status_for_routing.borrow(),
                code,
            );
        });
        nav.add(&overview_page);

        // Initial route.
        match route {
            InitialRoute::Overview => {}
            InitialRoute::Details(issue) => {
                let app_clone = app.clone();
                let nav_clone = nav.clone();
                let page = details::build_details_page(issue, move |fix_issue| {
                    let p = fix_flow::build_fix_flow_page(&app_clone, fix_issue, &nav_clone);
                    nav_clone.push(&p);
                });
                nav_for_rows.push(&page);
            }
            InitialRoute::Fix(issue) => {
                let nav_clone = nav.clone();
                let page = fix_flow::build_fix_flow_page(app, issue, &nav_clone);
                nav_for_rows.push(&page);
            }
        }

        win.set_content(Some(&nav));
        win
    }
}

fn push_for_code(app: &adw::Application, nav: &adw::NavigationView, status: &Status, code: &str) {
    let Some(issue) = status.issues.iter().find(|i| i.code == code).cloned() else {
        return;
    };
    if issue.auto_fixable && !crate::status::is_info_only(&issue.code) {
        let page = fix_flow::build_fix_flow_page(app, issue, nav);
        nav.push(&page);
    } else {
        let app_clone = app.clone();
        let nav_clone = nav.clone();
        let page = details::build_details_page(issue, move |fix_issue| {
            let p = fix_flow::build_fix_flow_page(&app_clone, fix_issue, &nav_clone);
            nav_clone.push(&p);
        });
        nav.push(&page);
    }
}
