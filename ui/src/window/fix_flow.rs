//! Fix-flow as an `AdwNavigationPage`: confirm → password → progress → success/failure.

use adw::prelude::*;
use futures::StreamExt;
use glib::MainContext;
use std::cell::RefCell;
use std::rc::Rc;
use zeroize::Zeroizing;

use crate::mock;
use crate::pkexec::{self, FixEvent, FixHandle, FixMode};
use crate::status::{secret_kind, Issue, SecretKind, DEFAULT_MOK_DIR};
use crate::widgets::StepList;

thread_local! {
    /// Set globally by main.rs from `--mock-fix`. If `Some(mode)`, the
    /// fix-flow uses a canned shell script instead of pkexec.
    pub static MOCK_FIX: RefCell<Option<String>> = const { RefCell::new(None) };
}

struct ProgressBits {
    step_list: StepList,
    log_buffer: gtk::TextBuffer,
}

struct FlowState {
    issue: Issue,
    secret: SecretKind,
    nav: adw::NavigationView,
    page: RefCell<Option<adw::NavigationPage>>,
    stack: gtk::Stack,
    progress: Rc<ProgressBits>,
    failure_log: gtk::TextBuffer,
    handle: Rc<RefCell<Option<FixHandle>>>,
}

pub fn build_fix_flow_page(
    _app: &adw::Application,
    issue: Issue,
    nav: &adw::NavigationView,
) -> adw::NavigationPage {
    let secret = secret_kind(&issue.code, DEFAULT_MOK_DIR);

    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&adw::HeaderBar::new());

    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::SlideLeftRight)
        .transition_duration(180)
        .build();

    let (progress_widget, step_list, log_buffer) = build_progress_skeleton();
    let progress = Rc::new(ProgressBits {
        step_list,
        log_buffer,
    });
    let (failure_widget, failure_log_buf) = build_failure_skeleton();

    let state = Rc::new(FlowState {
        issue: issue.clone(),
        secret,
        nav: nav.clone(),
        page: RefCell::new(None),
        stack: stack.clone(),
        progress: progress.clone(),
        failure_log: failure_log_buf.clone(),
        handle: Rc::new(RefCell::new(None)),
    });

    stack.add_named(&build_confirm_page(state.clone()), Some("confirm"));
    if !matches!(secret, SecretKind::None) {
        stack.add_named(&build_password_page(state.clone()), Some("password"));
    }
    stack.add_named(&progress_widget, Some("progress"));
    wire_progress_cancel(&progress_widget, state.clone());
    stack.add_named(&build_success_page(state.clone()), Some("success"));
    stack.add_named(&failure_widget, Some("failed"));
    wire_failure_buttons(&failure_widget, state.clone(), failure_log_buf);

    tv.set_content(Some(&stack));

    let page = adw::NavigationPage::builder()
        .title(format!("Repair {}", issue.code))
        .tag("fix-flow")
        .child(&tv)
        .build();
    *state.page.borrow_mut() = Some(page.clone());
    page
}

// -------------------- Pages --------------------

fn build_confirm_page(state: Rc<FlowState>) -> gtk::Widget {
    let outer = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(18)
        .margin_top(18)
        .margin_bottom(18)
        .margin_start(24)
        .margin_end(24)
        .build();

    let title = gtk::Label::builder()
        .label(&state.issue.summary)
        .halign(gtk::Align::Start)
        .xalign(0.0)
        .wrap(true)
        .css_classes(["title-2"])
        .build();

    let detail = gtk::Label::builder()
        .label(&state.issue.detail)
        .halign(gtk::Align::Fill)
        .xalign(0.0)
        .hexpand(true)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .css_classes(["dim-label"])
        .build();

    let inner = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    inner.append(&detail);

    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&inner)
        .build();

    let actions = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::End)
        .build();
    let cont = gtk::Button::builder()
        .label("Continue")
        .css_classes(["pill", "suggested-action"])
        .build();
    actions.append(&cont);

    outer.append(&title);
    outer.append(&scroll);
    outer.append(&actions);

    let state_cont = state.clone();
    cont.connect_clicked(move |_| {
        if matches!(state_cont.secret, SecretKind::None) {
            launch_fix(state_cont.clone(), None);
        } else {
            state_cont.stack.set_visible_child_name("password");
        }
    });

    outer.upcast()
}

fn build_password_page(state: Rc<FlowState>) -> gtk::Widget {
    let (resource, root_id) = match state.secret {
        SecretKind::Luks => (
            "/io/github/gitii/SecurebootWatchdog/screen4_luks.ui",
            "screen4_root",
        ),
        SecretKind::Mok => (
            "/io/github/gitii/SecurebootWatchdog/screen5_mok.ui",
            "screen5_root",
        ),
        SecretKind::None => unreachable!(),
    };
    let builder = gtk::Builder::from_resource(resource);
    let root: gtk::Box = builder.object(root_id).expect("password root");

    match state.secret {
        SecretKind::Luks => wire_luks_page(&builder, state),
        SecretKind::Mok => wire_mok_page(&builder, state),
        SecretKind::None => unreachable!(),
    }
    root.upcast()
}

fn wire_luks_page(builder: &gtk::Builder, state: Rc<FlowState>) {
    let pw_row: adw::PasswordEntryRow = builder.object("passphrase_row").unwrap();
    let cancel: gtk::Button = builder.object("cancel_btn").unwrap();
    let cont: gtk::Button = builder.object("continue_btn").unwrap();

    cont.set_sensitive(false);
    let cont_for_text = cont.clone();
    pw_row.connect_changed(move |entry| {
        cont_for_text.set_sensitive(!entry.text().is_empty());
    });

    let state_cancel = state.clone();
    cancel.connect_clicked(move |_| {
        state_cancel.stack.set_visible_child_name("confirm");
    });

    let state_cont = state;
    cont.connect_clicked(move |_| {
        let pw = Zeroizing::new(pw_row.text().as_bytes().to_vec());
        launch_fix(state_cont.clone(), Some(pw));
    });
}

fn wire_mok_page(builder: &gtk::Builder, state: Rc<FlowState>) {
    let pw_row: adw::PasswordEntryRow = builder.object("password_row").unwrap();
    let confirm_row: adw::PasswordEntryRow = builder.object("confirm_row").unwrap();
    let mismatch: gtk::Label = builder.object("mismatch_label").unwrap();
    let cancel: gtk::Button = builder.object("cancel_btn").unwrap();
    let enroll: gtk::Button = builder.object("enroll_btn").unwrap();

    enroll.set_sensitive(false);

    let pw_row_c = pw_row.clone();
    let confirm_row_c = confirm_row.clone();
    let mismatch_c = mismatch.clone();
    let enroll_c = enroll.clone();
    let validate = move || {
        let a = pw_row_c.text();
        let b = confirm_row_c.text();
        let ok = !a.is_empty() && a == b;
        enroll_c.set_sensitive(ok);
        mismatch_c.set_visible(!b.is_empty() && a != b);
    };
    let v1 = validate.clone();
    pw_row.connect_changed(move |_| v1());
    let v2 = validate;
    confirm_row.connect_changed(move |_| v2());

    let state_cancel = state.clone();
    cancel.connect_clicked(move |_| {
        state_cancel.stack.set_visible_child_name("confirm");
    });

    let state_enroll = state;
    let pw_row_for_send = pw_row;
    enroll.connect_clicked(move |_| {
        let pw = Zeroizing::new(pw_row_for_send.text().as_bytes().to_vec());
        launch_fix(state_enroll.clone(), Some(pw));
    });
}

fn build_progress_skeleton() -> (gtk::Widget, StepList, gtk::TextBuffer) {
    let builder =
        gtk::Builder::from_resource("/io/github/gitii/SecurebootWatchdog/screen6_progress.ui");
    let root: gtk::Box = builder.object("screen6_root").expect("screen6_root");
    let step_box: gtk::Box = builder.object("step_list_box").expect("step_list_box");
    let log_view: gtk::TextView = builder.object("log_view").expect("log_view");
    let cancel: gtk::Button = builder.object("cancel_btn").expect("cancel_btn");
    cancel.set_widget_name("progress_cancel_btn");
    let step_list = StepList::new(step_box);
    let buf = log_view.buffer();
    (root.upcast(), step_list, buf)
}

fn wire_progress_cancel(progress_widget: &gtk::Widget, state: Rc<FlowState>) {
    if let Some(btn) = find_descendant_by_name(progress_widget, "progress_cancel_btn")
        .and_then(|w| w.downcast::<gtk::Button>().ok())
    {
        btn.connect_clicked(move |_| {
            if let Some(h) = state.handle.borrow_mut().take() {
                h.subprocess.force_exit();
            }
        });
    }
}

fn build_success_page(state: Rc<FlowState>) -> gtk::Widget {
    let builder =
        gtk::Builder::from_resource("/io/github/gitii/SecurebootWatchdog/screen7_done.ui");
    let root: gtk::Box = builder.object("screen7_root").expect("screen7_root");
    let later: gtk::Button = builder.object("later_btn").expect("later_btn");
    let reboot: gtk::Button = builder.object("reboot_btn").expect("reboot_btn");

    let state_later = state.clone();
    later.connect_clicked(move |_| {
        state_later.nav.pop();
    });
    reboot.connect_clicked(|_| {
        if let Err(e) = reboot_via_logind() {
            eprintln!("logind reboot failed: {e}");
        }
    });
    root.upcast()
}

fn build_failure_skeleton() -> (gtk::Widget, gtk::TextBuffer) {
    let builder =
        gtk::Builder::from_resource("/io/github/gitii/SecurebootWatchdog/screen8_failed.ui");
    let root: gtk::Box = builder.object("screen8_root").expect("screen8_root");
    let log_view: gtk::TextView = builder.object("log_view").expect("log_view");
    let buf = log_view.buffer();
    for id in ["copy_btn", "dismiss_btn", "retry_btn"] {
        if let Some(btn) = builder.object::<gtk::Button>(id) {
            btn.set_widget_name(&format!("failure_{id}"));
        }
    }
    (root.upcast(), buf)
}

fn wire_failure_buttons(
    failure_widget: &gtk::Widget,
    state: Rc<FlowState>,
    failure_log: gtk::TextBuffer,
) {
    let copy = find_descendant_by_name(failure_widget, "failure_copy_btn")
        .and_then(|w| w.downcast::<gtk::Button>().ok())
        .unwrap();
    let dismiss = find_descendant_by_name(failure_widget, "failure_dismiss_btn")
        .and_then(|w| w.downcast::<gtk::Button>().ok())
        .unwrap();
    let retry = find_descendant_by_name(failure_widget, "failure_retry_btn")
        .and_then(|w| w.downcast::<gtk::Button>().ok())
        .unwrap();

    let buf = failure_log;
    copy.connect_clicked(move |btn| {
        let text = buf
            .text(&buf.start_iter(), &buf.end_iter(), false)
            .to_string();
        btn.clipboard().set_text(&text);
    });

    let state_dismiss = state.clone();
    dismiss.connect_clicked(move |_| {
        state_dismiss.nav.pop();
    });

    let state_retry = state;
    retry.connect_clicked(move |_| {
        state_retry.progress.step_list.set_steps(&[]);
        state_retry.progress.log_buffer.set_text("");
        let target = match state_retry.secret {
            SecretKind::None => "confirm",
            _ => "password",
        };
        state_retry.stack.set_visible_child_name(target);
    });
}

// -------------------- Fix launch + event pump --------------------

fn launch_fix(state: Rc<FlowState>, password: Option<Zeroizing<Vec<u8>>>) {
    state.stack.set_visible_child_name("progress");
    state.progress.step_list.set_steps(&[]);
    state.progress.log_buffer.set_text("");

    let mock_mode = MOCK_FIX.with(|m| m.borrow().clone());
    let script = mock_mode.as_deref().map(mock::mock_fix_script);
    let mode = match script {
        Some(s) => FixMode::Mock { script: s },
        None => FixMode::Pkexec {
            code: &state.issue.code,
        },
    };

    let handle = match pkexec::spawn(mode, password) {
        Ok(h) => h,
        Err(e) => {
            state
                .failure_log
                .set_text(&format!("Failed to spawn fix: {e}"));
            state.stack.set_visible_child_name("failed");
            return;
        }
    };

    let mut events = handle.events;
    *state.handle.borrow_mut() = Some(FixHandle {
        subprocess: handle.subprocess,
        events: futures::channel::mpsc::unbounded().1,
    });

    let state_evt = state;
    MainContext::default().spawn_local(async move {
        while let Some(ev) = events.next().await {
            match ev {
                FixEvent::Steps(steps) => {
                    let refs: Vec<&str> = steps.iter().map(String::as_str).collect();
                    state_evt.progress.step_list.set_steps(&refs);
                }
                FixEvent::Step(name) => {
                    state_evt.progress.step_list.advance(&name);
                }
                FixEvent::LogLine(line) => {
                    let mut end = state_evt.progress.log_buffer.end_iter();
                    state_evt
                        .progress
                        .log_buffer
                        .insert(&mut end, &format!("{line}\n"));
                }
                FixEvent::SpawnError(msg) => {
                    state_evt.failure_log.set_text(&msg);
                    state_evt.progress.step_list.mark_current_failed();
                    state_evt.stack.set_visible_child_name("failed");
                    return;
                }
                FixEvent::Done(rc) => {
                    if rc == 0 {
                        state_evt.progress.step_list.mark_all_done();
                        state_evt.stack.set_visible_child_name("success");
                    } else {
                        let text = state_evt.progress.log_buffer.text(
                            &state_evt.progress.log_buffer.start_iter(),
                            &state_evt.progress.log_buffer.end_iter(),
                            false,
                        );
                        state_evt
                            .failure_log
                            .set_text(&format!("Fix script exited with code {rc}.\n\n{text}"));
                        state_evt.progress.step_list.mark_current_failed();
                        state_evt.stack.set_visible_child_name("failed");
                    }
                    *state_evt.handle.borrow_mut() = None;
                    return;
                }
            }
        }
    });
}

fn reboot_via_logind() -> anyhow::Result<()> {
    let conn = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE)?;
    conn.call_sync(
        Some("org.freedesktop.login1"),
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
        "Reboot",
        Some(&(true,).to_variant()),
        None,
        gio::DBusCallFlags::NONE,
        5_000,
        gio::Cancellable::NONE,
    )?;
    Ok(())
}

fn find_descendant_by_name(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    let mut child = root.first_child();
    while let Some(c) = child {
        if let Some(found) = find_descendant_by_name(&c, name) {
            return Some(found);
        }
        child = c.next_sibling();
    }
    None
}
