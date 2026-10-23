use std::cell::RefCell;
use std::rc::Rc;

use crate::changes;
use crate::graphics::Area;
use crate::input::focus::{self, FocusHandler};
use crate::input::{Cursor, Handlers, Target, modifiers};
use crate::services::clipboard;
use crate::{Button, Key, Point};

use super::field::Edit;
use super::{TextHandler, TextInput, fields};

thread_local! {
    // the input that asked for a paste, until the clipboard hands the text over
    static PASTING: RefCell<Option<(&'static str, Option<TextHandler>)>> =
        const { RefCell::new(None) };
}

pub fn collect_targets(text_input: &TextInput, area: Area, targets: &mut Vec<Target>) {
    let id = text_input.id;
    let on_key = key_handler(text_input);

    // the handler holds this redraw's on_change and on_submit, so a focused input takes the new one
    if focus::has(id) || text_input.focused {
        focus::set(id, on_key.clone());
    }

    let on_click = move |_: Button, _: Point| {
        focus::set(id, on_key.clone());
    };

    let handlers = Handlers {
        click: Some(Rc::new(on_click)),
        cursor: Some(Cursor::Text),
        ..Handlers::default()
    };

    targets.push(Target::new(area, handlers));
}

fn key_handler(text_input: &TextInput) -> FocusHandler {
    let id = text_input.id;

    let on_change = text_input.on_change.clone();
    let on_submit = text_input.on_submit.clone();
    let password = text_input.password;

    let on_key = move |key: Key| match key {
        Key::Enter => {
            submit(id, on_submit.as_ref(), password);

            true
        }

        Key::Escape => {
            focus::clear();

            true
        }

        Key::Control('v') => {
            PASTING.set(Some((id, on_change.clone())));

            clipboard::ask_paste();

            true
        }

        _ => edit(id, key, on_change.as_ref()),
    };

    Rc::new(on_key)
}

fn submit(id: &'static str, on_submit: Option<&TextHandler>, password: bool) {
    let Some(on_submit) = on_submit else {
        return;
    };

    on_submit(fields::get(id).text);

    if password {
        TextInput::set_text(id, "")
    }
}

/*
 * the text Ctrl+V asked for, put in at the cursor once the clipboard hands
 * it over, as long as the input still has the keyboard; lines are joined,
 * since an input holds one
 */
pub fn paste(text: &str) {
    let Some((id, on_change)) = PASTING.take() else {
        return;
    };

    if !focus::has(id) {
        return;
    }

    let mut field = fields::get(id);

    if !field.insert_text(&one_line(text)) {
        return;
    }

    let text = field.text.clone();

    fields::set(id, field);

    if let Some(on_change) = on_change {
        on_change(text);
    }

    // a paste comes on its own, not with a key, so nothing else draws it
    changes::mark_all();
}

fn one_line(text: &str) -> String {
    text.trim_end_matches(['\n', '\r'])
        .chars()
        .map(|letter| if letter.is_control() { ' ' } else { letter })
        .collect()
}

// returns whether the key meant anything to a text input
fn edit(id: &'static str, key: Key, on_change: Option<&TextHandler>) -> bool {
    let mut field = fields::get(id);

    let edit = field.apply(key, modifiers::modifiers().ctrl);
    let text = field.text.clone();

    // stored before the handler runs, so it reads the new text
    fields::set(id, field);

    if edit == Edit::Changed
        && let Some(on_change) = on_change
    {
        on_change(text);
    }

    edit != Edit::Unused
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pastes_lines_as_one() {
        assert_eq!(one_line("hunter2\n"), "hunter2");
        assert_eq!(one_line("two\nlines\r\n"), "two lines");
        assert_eq!(one_line("a\tb"), "a b");
    }
}
