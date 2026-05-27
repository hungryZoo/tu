//! Modal dialogs: `~/.tmux.conf` setup and delete confirmation.

use cursive::event::Key;
use cursive::views::{Dialog, OnEventView, TextView};
use cursive::Cursive;

use crate::app;
use crate::conf_setup::{self, Directive};
use crate::tmux;

pub fn show_conf_setup(siv: &mut Cursive, missing: Vec<Directive>) {
    let body = build_conf_body(&missing);
    let yes_directives = missing.clone();
    let key_directives = missing;

    let dialog = Dialog::around(TextView::new(body))
        .title("~/.tmux.conf")
        .button("Yes, add (y)", move |s| {
            s.pop_layer();
            apply_conf(s, &yes_directives);
        })
        .button("Later (n)", |s| {
            s.pop_layer();
        });

    let view = OnEventView::new(dialog)
        .on_event('y', move |s| {
            s.pop_layer();
            apply_conf(s, &key_directives);
        })
        .on_event('n', |s| {
            s.pop_layer();
        })
        .on_event(Key::Esc, |s| {
            s.pop_layer();
        });

    siv.add_layer(view);
}

fn build_conf_body(missing: &[Directive]) -> String {
    let mut s = String::from("Add the following to ~/.tmux.conf?\n\n");
    for d in missing {
        s.push_str(&format!("  {}   ({})\n", d.line(), d.label));
    }
    s.push_str("\nWe will patch the live tmux server and append to the conf file.");
    s
}

fn apply_conf(siv: &mut Cursive, directives: &[Directive]) {
    let conf = conf_setup::conf_path();
    if let Err(e) = conf_setup::append_directives(directives, &conf) {
        app::set_status(siv, format!("Failed to update ~/.tmux.conf: {e}"));
        return;
    }
    let applied = conf_setup::apply_directives_to_server(directives);
    let names: Vec<String> = directives.iter().map(|d| d.option.clone()).collect();
    let live_note = if applied {
        " · live tmux server updated"
    } else {
        " · will take effect on next tmux start"
    };
    app::set_status(
        siv,
        format!(
            "Appended {} to {}{}",
            names.join(", "),
            conf.display(),
            live_note
        ),
    );
}

pub fn show_confirm_delete(siv: &mut Cursive, name: String) {
    let name_for_btn = name.clone();
    let title = format!("Delete session '{}'", name);
    let dialog = Dialog::around(TextView::new(format!(
        "Really delete session '{}'?\nThis cannot be undone.",
        name
    )))
    .title(title)
    // Back is added first so it gets the initial focus — same safety
    // posture as the Python `ConfirmDeleteModal`.
    .button("Back", |s| {
        s.pop_layer();
    })
    .button("Delete", move |s| {
        s.pop_layer();
        let r = tmux::kill_session(&name_for_btn);
        if !r.ok() {
            app::set_status(
                s,
                format!(
                    "Failed to delete session '{}': {}",
                    name_for_btn,
                    r.stderr.trim()
                ),
            );
            return;
        }
        app::set_status(s, format!("Session '{}' deleted.", name_for_btn));
        app::refresh_sessions(s);
    });

    let view = OnEventView::new(dialog).on_event(Key::Esc, |s| {
        s.pop_layer();
    });

    siv.add_layer(view);
}
