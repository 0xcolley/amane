use std::rc::Rc;

use crate::graphics::Area;
use crate::input::focus::{self, FocusHandler};
use crate::input::{Cursor, Handlers, Target, modifiers};
use crate::{Button, Key, Point};

use super::field::Edit;
use super::{TextHandler, TextInput, fields};

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
