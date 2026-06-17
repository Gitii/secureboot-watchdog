//! Step list widget for the fix-progress screen.
//!
//! Holds an ordered list of named steps; each step has one of three visual
//! states (pending/current/done) plus an explicit failed state. Driven by
//! `[STEP] name` lines coming out of `secureboot-fix`.

use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Pending,
    Current,
    Done,
    Failed,
}

struct StepRow {
    container: gtk::Box,
    icon: gtk::Image,
    label: gtk::Label,
}

impl StepRow {
    fn new(name: &str, state: StepState) -> Self {
        let container = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(10)
            .css_classes(["step-row"])
            .build();
        let icon = gtk::Image::new();
        icon.add_css_class("step-icon");
        icon.set_pixel_size(16);
        let label = gtk::Label::builder()
            .label(humanize_step_name(name))
            .halign(gtk::Align::Start)
            .build();
        container.append(&icon);
        container.append(&label);
        let row = Self {
            container,
            icon,
            label,
        };
        row.apply_state(state);
        row
    }

    fn apply_state(&self, state: StepState) {
        for cls in ["step-pending", "step-current", "step-done", "step-failed"] {
            self.container.remove_css_class(cls);
        }
        match state {
            StepState::Pending => {
                self.icon.set_icon_name(Some("media-record-symbolic"));
                self.container.add_css_class("step-pending");
            }
            StepState::Current => {
                self.icon.set_icon_name(Some("content-loading-symbolic"));
                self.container.add_css_class("step-current");
            }
            StepState::Done => {
                self.icon.set_icon_name(Some("object-select-symbolic"));
                self.container.add_css_class("step-done");
            }
            StepState::Failed => {
                self.icon.set_icon_name(Some("dialog-error-symbolic"));
                self.container.add_css_class("step-failed");
            }
        }
        let _ = self.label;
    }
}

#[derive(Clone)]
pub struct StepList {
    container: gtk::Box,
    rows: Rc<RefCell<Vec<(String, StepRow)>>>,
}

impl StepList {
    pub fn new(container: gtk::Box) -> Self {
        Self {
            container,
            rows: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Replace the contents with the given step names, all in Pending state.
    pub fn set_steps(&self, names: &[&str]) {
        let mut child = self.container.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            self.container.remove(&c);
        }
        let mut rows = self.rows.borrow_mut();
        rows.clear();
        for name in names {
            let row = StepRow::new(name, StepState::Pending);
            self.container.append(&row.container);
            rows.push(((*name).to_string(), row));
        }
    }

    /// Mark previous as Done and given as Current. If the name doesn't match
    /// any known step, append a new step.
    pub fn advance(&self, name: &str) {
        let mut rows = self.rows.borrow_mut();
        // Find the row to make Current; mark anything before it Done.
        let mut found = false;
        let mut found_idx = 0;
        for (idx, (n, _row)) in rows.iter().enumerate() {
            if n == name {
                found = true;
                found_idx = idx;
                break;
            }
        }
        if !found {
            let row = StepRow::new(name, StepState::Current);
            self.container.append(&row.container);
            // mark previous current/pending as done
            for (_, prev) in rows.iter() {
                prev.apply_state(StepState::Done);
            }
            rows.push((name.to_string(), row));
            return;
        }
        for (idx, (_, row)) in rows.iter().enumerate() {
            if idx < found_idx {
                row.apply_state(StepState::Done);
            } else if idx == found_idx {
                row.apply_state(StepState::Current);
            } else {
                row.apply_state(StepState::Pending);
            }
        }
    }

    /// Mark all current/pending steps as Done (call on successful completion).
    pub fn mark_all_done(&self) {
        let rows = self.rows.borrow();
        for (_, row) in rows.iter() {
            row.apply_state(StepState::Done);
        }
    }

    /// Mark the current step as Failed, leave done steps as Done.
    pub fn mark_current_failed(&self) {
        let rows = self.rows.borrow();
        for (_, row) in rows.iter() {
            // Only flip current → failed; pending stays pending.
            if row.container.has_css_class("step-current") {
                row.apply_state(StepState::Failed);
                return;
            }
        }
    }
}

fn humanize_step_name(name: &str) -> String {
    name.split(&['-', '_'][..])
        .map(|word| {
            let mut c = word.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().chain(c).collect(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
