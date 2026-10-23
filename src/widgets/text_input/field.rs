use crate::Key;

// what a text input holds: its text, and where the cursor sits in it
#[derive(Clone, Default)]
pub struct Field {
    pub text: String,

    // counts letters, not bytes
    pub cursor: usize,
}

// what a key did to a field
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edit {
    Changed,

    // the cursor moved, or the key had nothing to remove, but it was a key for typing
    Kept,

    // not a key for typing, so the window gets it
    Unused,
}

impl Field {
    /*
     * with Ctrl held, Backspace, Delete and the arrows go a word at a time;
     * Ctrl+U clears what's before the cursor and Ctrl+W the word before it,
     * as in a terminal
     */
    pub fn apply(&mut self, key: Key, ctrl: bool) -> Edit {
        let changed = match key {
            Key::Character(letter) => self.insert(letter),
            Key::Space => self.insert(' '),
            Key::Backspace if ctrl => self.remove(self.word_before(), self.cursor),
            Key::Backspace => self.remove(self.cursor.saturating_sub(1), self.cursor),
            Key::Delete if ctrl => self.remove(self.cursor, self.word_after()),
            Key::Delete => self.remove(self.cursor, self.cursor + 1),
            Key::Control('w') => self.remove(self.word_before(), self.cursor),
            Key::Control('u') => self.remove(0, self.cursor),
            Key::Left if ctrl => self.move_to(self.word_before()),
            Key::Left => self.move_to(self.cursor.saturating_sub(1)),
            Key::Right if ctrl => self.move_to(self.word_after()),
            Key::Right => self.move_to(self.cursor + 1),
            Key::Home => self.move_to(0),
            Key::End => self.move_to(self.length()),
            _ => return Edit::Unused,
        };

        if changed { Edit::Changed } else { Edit::Kept }
    }

    pub fn before_cursor(&self) -> String {
        self.text.chars().take(self.cursor).collect()
    }

    // puts text in at the cursor, which ends up after it
    pub fn insert_text(&mut self, text: &str) -> bool {
        let at = self.byte_index();

        self.text.insert_str(at, text);
        self.cursor += text.chars().count();

        !text.is_empty()
    }

    fn insert(&mut self, letter: char) -> bool {
        let at = self.byte_index();

        self.text.insert(at, letter);
        self.cursor += 1;

        true
    }

    // removes the letters from `start` up to `end`, leaving the cursor where they were
    fn remove(&mut self, start: usize, end: usize) -> bool {
        let end = end.min(self.length());

        if start >= end {
            return false;
        }

        let from = self.byte_at(start);
        let to = self.byte_at(end);

        self.text.replace_range(from..to, "");
        self.cursor = start;

        true
    }

    /*
     * where the word before the cursor starts: past any spaces, then past
     * letters and digits, or past a run of other marks like "./"
     */
    fn word_before(&self) -> usize {
        let letters: Vec<char> = self.text.chars().take(self.cursor).collect();

        let mut at = letters.len();

        while at > 0 && letters[at - 1].is_whitespace() {
            at -= 1;
        }

        let Some(&last) = at.checked_sub(1).and_then(|index| letters.get(index)) else {
            return 0;
        };

        let kind = word_kind(last);

        while at > 0 && word_kind(letters[at - 1]) == kind {
            at -= 1;
        }

        at
    }

    // where the word after the cursor ends, the same way going right
    fn word_after(&self) -> usize {
        let letters: Vec<char> = self.text.chars().collect();

        let mut at = self.cursor.min(letters.len());

        while at < letters.len() && letters[at].is_whitespace() {
            at += 1;
        }

        let Some(&first) = letters.get(at) else {
            return at;
        };

        let kind = word_kind(first);

        while at < letters.len() && word_kind(letters[at]) == kind {
            at += 1;
        }

        at
    }

    fn move_to(&mut self, cursor: usize) -> bool {
        self.cursor = cursor.min(self.length());

        false
    }

    fn length(&self) -> usize {
        self.text.chars().count()
    }

    // a String is indexed by bytes, and one letter can take several
    fn byte_index(&self) -> usize {
        self.byte_at(self.cursor)
    }

    fn byte_at(&self, letter: usize) -> usize {
        match self.text.char_indices().nth(letter) {
            Some((index, _)) => index,
            None => self.text.len(),
        }
    }
}

// letters and digits make words; other marks make words of their own; spaces part them
fn word_kind(letter: char) -> u8 {
    if letter.is_alphanumeric() || letter == '_' {
        1
    } else if letter.is_whitespace() {
        0
    } else {
        2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_by_letter_not_by_byte() {
        let mut field = Field::default();

        for letter in "héllo".chars() {
            field.apply(Key::Character(letter), false);
        }

        field.apply(Key::Left, false);
        field.apply(Key::Left, false);
        field.apply(Key::Left, false);

        // the cursor sits after "hé", two letters but three bytes
        assert_eq!(field.before_cursor(), "hé");

        assert_eq!(field.apply(Key::Backspace, false), Edit::Changed);
        assert_eq!(field.text, "hllo");

        assert_eq!(field.apply(Key::Home, false), Edit::Kept);
        assert_eq!(field.apply(Key::Backspace, false), Edit::Kept);

        field.apply(Key::End, false);
        field.apply(Key::Space, false);

        assert_eq!(field.text, "hllo ");
        assert_eq!(field.cursor, 5);
    }

    fn typed(text: &str) -> Field {
        Field {
            text: String::from(text),
            cursor: text.chars().count(),
        }
    }

    #[test]
    fn ctrl_backspace_takes_the_word_before() {
        let mut field = typed("open ~/Pictures/wallpaper.jpg  ");

        field.apply(Key::Backspace, true);
        assert_eq!(field.text, "open ~/Pictures/wallpaper.");

        field.apply(Key::Backspace, true);
        assert_eq!(field.text, "open ~/Pictures/wallpaper");

        field.apply(Key::Control('w'), false);
        assert_eq!(field.text, "open ~/Pictures/");

        field.apply(Key::Control('u'), false);
        assert_eq!(field.text, "");
        assert_eq!(field.apply(Key::Backspace, true), Edit::Kept);
    }

    #[test]
    fn delete_takes_what_follows() {
        let mut field = typed("fire fox");

        field.apply(Key::Home, false);
        field.apply(Key::Delete, false);
        assert_eq!(field.text, "ire fox");

        field.apply(Key::Delete, true);
        assert_eq!(field.text, " fox");
        assert_eq!(field.cursor, 0);
    }

    #[test]
    fn ctrl_arrows_jump_words() {
        let mut field = typed("héllo wörld");

        field.apply(Key::Left, true);
        assert_eq!(field.cursor, 6);

        field.apply(Key::Left, true);
        assert_eq!(field.cursor, 0);

        field.apply(Key::Right, true);
        assert_eq!(field.cursor, 5);
    }

    #[test]
    fn pastes_at_the_cursor() {
        let mut field = typed("wi network");

        field.apply(Key::Home, false);
        field.apply(Key::Right, false);
        field.apply(Key::Right, false);

        assert!(field.insert_text("-fé"));
        assert_eq!(field.text, "wi-fé network");
        assert_eq!(field.cursor, 5);

        assert!(!field.insert_text(""));
    }

    #[test]
    fn other_shortcuts_go_to_the_window() {
        let mut field = typed("text");

        assert_eq!(field.apply(Key::Control('d'), false), Edit::Unused);
        assert_eq!(field.apply(Key::Up, false), Edit::Unused);
        assert_eq!(field.text, "text");
    }
}
