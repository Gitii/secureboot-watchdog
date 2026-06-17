//! Adw.Application bootstrap + CLI argparse + window routing.

use adw::prelude::*;
use clap::{Parser, Subcommand};
use gtk::glib;
use std::cell::RefCell;
use std::path::PathBuf;

use crate::mock;
use crate::status::{load_status, Issue, Status, DEFAULT_STATUS_PATH};
use crate::window::{InitialRoute, SecurebootWindow};

/// The compiled gresource bundle (`.gresource` produced by build.rs).
const GRESOURCE_BYTES: &[u8] = include_bytes!(concat!(
    env!("OUT_DIR"),
    "/secureboot-watchdog-ui.gresource"
));

const APP_ID: &str = "io.github.gitii.SecurebootWatchdog";

#[derive(Parser, Debug, Clone)]
#[command(name = "secureboot-watchdog-ui", version)]
struct Cli {
    /// Load a fixture JSON instead of /run/secureboot-monitor/status.json.
    #[arg(long)]
    mock_status: Option<String>,
    /// Replace `pkexec /usr/sbin/secureboot-fix` with a canned shell script:
    /// success / failure / slow.
    #[arg(long, value_parser = ["success", "failure", "slow"])]
    mock_fix: Option<String>,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug, Clone)]
enum Cmd {
    /// Show the overview window (default).
    Status,
    /// Show the info-only details page for an issue.
    Details {
        #[arg(long)]
        code: String,
    },
    /// Open the fix-flow window for an issue.
    Fix {
        #[arg(long)]
        code: String,
    },
}

thread_local! {
    static PARSED: RefCell<Option<Cli>> = const { RefCell::new(None) };
}

pub fn run() -> glib::ExitCode {
    // Parse args before Gtk grabs them.
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            // clap prints help / version itself; map exit codes.
            e.print().ok();
            return match e.kind() {
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion => {
                    glib::ExitCode::SUCCESS
                }
                _ => glib::ExitCode::from(2),
            };
        }
    };
    PARSED.with(|p| *p.borrow_mut() = Some(cli.clone()));

    // Stash mock-fix for the fix-flow to pick up.
    if let Some(mode) = &cli.mock_fix {
        crate::window::fix_flow::MOCK_FIX.with(|m| *m.borrow_mut() = Some(mode.clone()));
    }

    // Register embedded resources before Gtk needs them.
    let res = gio::Resource::from_data(&glib::Bytes::from_static(GRESOURCE_BYTES))
        .expect("load embedded gresource");
    gio::resources_register(&res);

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::FLAGS_NONE)
        .build();

    app.connect_startup(|_| {
        // Apply our stylesheet on startup so every later window inherits it.
        let provider = gtk::CssProvider::new();
        provider.load_from_resource("/io/github/gitii/SecurebootWatchdog/style.css");
        let display = gtk::gdk::Display::default().expect("no default display");
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });

    app.connect_activate(|app| {
        let cli = PARSED.with(|p| p.borrow().clone()).expect("parsed cli");
        match dispatch(app, &cli) {
            Ok(()) => {}
            Err(e) => {
                eprintln!("error: {e}");
                show_error_window(app, &e.to_string());
            }
        }
    });

    // Don't let GApplication parse argv again.
    app.run_with_args::<&str>(&[])
}

fn dispatch(app: &adw::Application, cli: &Cli) -> anyhow::Result<()> {
    let status = load_status_with_mock(cli)?;
    let route = match &cli.cmd {
        None | Some(Cmd::Status) => InitialRoute::Overview,
        Some(Cmd::Details { code }) => InitialRoute::Details(find_issue(&status, code)?),
        Some(Cmd::Fix { code }) => InitialRoute::Fix(find_issue(&status, code)?),
    };
    let win = SecurebootWindow::build(app, status, route);
    present_window(&win);
    Ok(())
}

fn load_status_with_mock(cli: &Cli) -> anyhow::Result<Status> {
    let path = if let Some(name) = &cli.mock_status {
        mock::resolve_fixture(name).ok_or_else(|| anyhow::anyhow!("fixture not found: {name}"))?
    } else {
        PathBuf::from(DEFAULT_STATUS_PATH)
    };
    load_status(&path).map_err(|e| anyhow::anyhow!("loading status from {}: {e}", path.display()))
}

fn find_issue(status: &Status, code: &str) -> anyhow::Result<Issue> {
    status
        .issues
        .iter()
        .find(|i| i.code == code)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("status does not contain issue with code {code}"))
}

fn present_window(win: &adw::ApplicationWindow) {
    win.present();
    // present_with_time would force-raise on Wayland, but the current API
    // takes the window's own monitor frame timestamp; plain present() works
    // when launched by a notification action that carries an activation token.
}

fn show_error_window(app: &adw::Application, msg: &str) {
    let win = adw::ApplicationWindow::builder()
        .application(app)
        .title("Secure Boot — error")
        .default_width(480)
        .default_height(220)
        .build();
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&adw::HeaderBar::new());
    tv.set_content(Some(
        &adw::StatusPage::builder()
            .icon_name("dialog-error-symbolic")
            .title("Couldn't load status")
            .description(msg)
            .build(),
    ));
    win.set_content(Some(&tv));
    win.present();
}
