mod application;
mod mock;
mod pkexec;
mod status;
mod widgets;
mod window;

use gtk::glib;

fn main() -> glib::ExitCode {
    application::run()
}
