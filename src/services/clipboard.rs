use std::collections::hash_map::DefaultHasher;
use std::env;
use std::fs::{self, DirBuilder};
use std::hash::{Hash, Hasher};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

use crate::graphics::image;
use crate::{Service, changes};

// the most clips kept, and of those the most pictures, which take far more room
const KEPT: usize = 100;
const PICTURES_KEPT: usize = 20;

// a copy bigger than this is left out of the history, but still pastes in other programs
const TEXT_LIMIT: usize = 1 << 20;
const PICTURE_LIMIT: usize = 16 << 20;

// marks what the shell put on the clipboard itself, so it isn't kept a second time
pub(crate) const OURS: &str = "application/x-amane-clipboard";

// password managers mark what they copy with this, and it is never kept
const SECRET: &str = "x-kde-passwordManagerHint";

// the names programs give plain text, the best first
const TEXT: [&str; 5] = [
    "text/plain;charset=utf-8",
    "UTF8_STRING",
    "text/plain",
    "STRING",
    "TEXT",
];

const PNG: &str = "image/png";
const JPEG: &str = "image/jpeg";

// set once something reads the history, before that nothing copied is looked at
static RECORDING: AtomicBool = AtomicBool::new(false);

// what was on the clipboard before recording began is kept too, once the event loop sees this
static STARTED: AtomicBool = AtomicBool::new(false);

// what to put on the clipboard next, taken by the event loop when it wakes
static OFFERING: Mutex<Option<Offering>> = Mutex::new(None);

/*
 * a paste a text input asked for, and the text once the program that copied
 * it has handed it over; it arrives on a later wake
 */
static PASTE_ASKED: AtomicBool = AtomicBool::new(false);
static PASTED: Mutex<Option<String>> = Mutex::new(None);

/*
 * what was copied lately, newest first: text, and pictures, which are kept
 * as files in the runtime directory. Nothing is kept until something reads
 * the history, and what password managers copy never is
 */
pub struct Clipboard {
    clips: Vec<Clip>,
    next: u64,

    // the clip the clipboard holds right now, none when it holds something not kept
    current: Option<u64>,

    // counts what was copied, so a slow read of an older copy doesn't claim to be current
    copies: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    id: u64,
    kind: ClipKind,
    copied: SystemTime,

    // tells a picture copied again from a new one, without comparing every byte
    hash: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClipKind {
    Text(String),

    // a png or jpeg file, and its size in pixels
    Image {
        path: PathBuf,
        width: u32,
        height: u32,
    },
}

// what the shell offers the clipboard: the names it goes by, and the bytes under every one
pub(crate) struct Offering {
    pub types: Vec<&'static str>,
    pub bytes: Arc<[u8]>,
}

// changes through the compositor rather than polling
impl Service for Clipboard {
    fn new() -> Self {
        // pictures from a shell that ran before went with it
        let _ = fs::remove_dir_all(folder());

        RECORDING.store(true, Ordering::Relaxed);
        STARTED.store(true, Ordering::Relaxed);

        changes::wake();

        Self {
            clips: Vec::new(),
            next: 1,
            current: None,
            copies: 0,
        }
    }

    fn listen() {}
}

impl Clipboard {
    pub fn clips(&self) -> &[Clip] {
        &self.clips
    }

    // the clip the clipboard holds now, if it's one that was kept
    pub fn current(&self) -> Option<u64> {
        self.current
    }

    // puts a clip back on the clipboard, and at the top of the history
    pub fn copy(id: u64) {
        let mut clipboard = Self::write();

        let Some(index) = clipboard.clips.iter().position(|clip| clip.id == id) else {
            return;
        };

        let mut clip = clipboard.clips.remove(index);

        clip.copied = SystemTime::now();

        let offering = offering(&clip.kind);

        clipboard.clips.insert(0, clip);
        clipboard.current = Some(id);

        // a picture whose file went missing is left out rather than offered empty
        if let Some(offering) = offering {
            offer(offering);
        }
    }

    // copies text from the shell itself, which is kept like any other copy
    pub fn copy_text(text: &str) {
        let id = Self::write().keep(ClipKind::Text(String::from(text)), hash(text.as_bytes()));

        Self::copy(id);
    }

    pub fn remove(id: u64) {
        let mut clipboard = Self::write();

        let Some(index) = clipboard.clips.iter().position(|clip| clip.id == id) else {
            return;
        };

        forget(&clipboard.clips.remove(index));

        // the clipboard still holds it, but it isn't kept any more
        if clipboard.current == Some(id) {
            clipboard.current = None;
        }
    }

    // empties the history; what the clipboard holds stays there
    pub fn clear() {
        let mut clipboard = Self::write();

        for clip in clipboard.clips.drain(..) {
            forget(&clip);
        }

        clipboard.current = None;
    }

    pub(crate) fn recording() -> bool {
        RECORDING.load(Ordering::Relaxed)
    }

    pub(crate) fn take_start() -> bool {
        STARTED.swap(false, Ordering::Relaxed)
    }

    pub(crate) fn take_offering() -> Option<Offering> {
        OFFERING
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }

    /*
     * something new is on the clipboard; gives back its number for
     * record(). What the shell put there is already current
     */
    pub(crate) fn selected(ours: bool) -> u64 {
        let mut clipboard = Self::write();

        clipboard.copies += 1;

        if !ours {
            clipboard.current = None;
        }

        clipboard.copies
    }

    /*
     * the program that copied the current clip quit, which empties the
     * clipboard; the clip is offered again so it still pastes. Not after
     * a copy that wasn't kept, like a password a manager has since cleared
     */
    pub(crate) fn emptied() {
        let mut clipboard = Self::write();

        clipboard.copies += 1;

        let current = clipboard.current;

        let kept = clipboard.clips.iter().find(|clip| Some(clip.id) == current);

        if let Some(offering) = kept.and_then(|clip| offering(&clip.kind)) {
            offer(offering);
        }

        clipboard.quiet();
    }

    /*
     * keeps what was read of a copy, given the type it was read as; runs on
     * the thread that read it, so writing a picture's file never stalls drawing
     */
    pub(crate) fn record(copy: u64, kind: &str, bytes: Vec<u8>) {
        let hash = hash(&bytes);

        let clip = if TEXT.contains(&kind) {
            let Ok(text) = String::from_utf8(bytes) else {
                return;
            };

            if text.trim().is_empty() {
                return;
            }

            ClipKind::Text(text)
        } else {
            let Some((width, height)) = image::dimensions(&bytes) else {
                return;
            };

            let mut clipboard = Self::write();

            // a picture copied again keeps its file, it only moves up
            if let Some(id) = clipboard.repeat(hash) {
                clipboard.claim(copy, id);

                return;
            }

            let id = clipboard.take_id();

            drop(clipboard);

            let extension = if kind == JPEG { "jpg" } else { "png" };
            let path = folder().join(format!("{id}.{extension}"));

            if save(&path, &bytes).is_err() {
                return;
            }

            let mut clipboard = Self::write();

            clipboard.insert(Clip {
                id,
                kind: ClipKind::Image {
                    path,
                    width,
                    height,
                },
                copied: SystemTime::now(),
                hash,
            });

            clipboard.claim(copy, id);

            return;
        };

        let mut clipboard = Self::write();

        let id = clipboard.keep(clip, hash);

        clipboard.claim(copy, id);
    }

    // the clip the history already has with these contents, moved to the top, or a new one
    fn keep(&mut self, kind: ClipKind, hash: u64) -> u64 {
        if let Some(id) = self.repeat(hash) {
            return id;
        }

        let id = self.take_id();

        self.insert(Clip {
            id,
            kind,
            copied: SystemTime::now(),
            hash,
        });

        id
    }

    fn repeat(&mut self, hash: u64) -> Option<u64> {
        let index = self.clips.iter().position(|clip| clip.hash == hash)?;

        let mut clip = self.clips.remove(index);

        clip.copied = SystemTime::now();

        let id = clip.id;

        self.clips.insert(0, clip);

        Some(id)
    }

    fn take_id(&mut self) -> u64 {
        let id = self.next;

        self.next += 1;

        id
    }

    // at the top, dropping the oldest past the limits
    fn insert(&mut self, clip: Clip) {
        self.clips.insert(0, clip);

        let mut pictures = 0;

        for clip in std::mem::take(&mut self.clips) {
            let picture = matches!(clip.kind, ClipKind::Image { .. });

            pictures += usize::from(picture);

            if self.clips.len() < KEPT && (!picture || pictures <= PICTURES_KEPT) {
                self.clips.push(clip);
            } else {
                forget(&clip);
            }
        }
    }

    // the clip is what the clipboard holds, unless something newer was copied while it was read
    fn claim(&mut self, copy: u64, id: u64) {
        if copy == self.copies {
            self.current = Some(id);
        }
    }
}

impl Clip {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn kind(&self) -> &ClipKind {
        &self.kind
    }

    pub fn copied(&self) -> SystemTime {
        self.copied
    }

    pub fn text(&self) -> Option<&str> {
        match &self.kind {
            ClipKind::Text(text) => Some(text),
            ClipKind::Image { .. } => None,
        }
    }
}

/*
 * which type to read a copy as for the history: text first, then a picture.
 * None for what a password manager copied, for the shell's own copies, and
 * for anything else
 */
pub(crate) fn pick(types: &[String]) -> Option<&'static str> {
    let has = |wanted: &str| types.iter().any(|kind| kind == wanted);

    if has(SECRET) || has(OURS) {
        return None;
    }

    TEXT.into_iter().chain([PNG, JPEG]).find(|kind| has(kind))
}

// which type to read a copy as for pasting into a text input, secrets included
pub(crate) fn text_type(types: &[String]) -> Option<&'static str> {
    TEXT.into_iter()
        .find(|wanted| types.iter().any(|kind| kind == wanted))
}

pub(crate) fn limit(kind: &str) -> usize {
    if TEXT.contains(&kind) {
        TEXT_LIMIT
    } else {
        PICTURE_LIMIT
    }
}

// Ctrl+V in a text input: the text is read from the clipboard and arrives on a later wake
pub(crate) fn ask_paste() {
    PASTE_ASKED.store(true, Ordering::Relaxed);

    changes::wake();
}

pub(crate) fn take_paste_asked() -> bool {
    PASTE_ASKED.swap(false, Ordering::Relaxed)
}

pub(crate) fn pasted(text: String) {
    *PASTED.lock().unwrap_or_else(PoisonError::into_inner) = Some(text);

    changes::wake();
}

pub(crate) fn take_pasted() -> Option<String> {
    PASTED.lock().unwrap_or_else(PoisonError::into_inner).take()
}

fn offer(offering: Offering) {
    *OFFERING.lock().unwrap_or_else(PoisonError::into_inner) = Some(offering);

    changes::wake();
}

fn offering(kind: &ClipKind) -> Option<Offering> {
    match kind {
        ClipKind::Text(text) => Some(Offering {
            types: TEXT.to_vec(),
            bytes: Arc::from(text.as_bytes()),
        }),

        ClipKind::Image { path, .. } => {
            let bytes = fs::read(path).ok()?;

            let kind = if path.extension().is_some_and(|extension| extension == "jpg") {
                JPEG
            } else {
                PNG
            };

            Some(Offering {
                types: vec![kind],
                bytes: Arc::from(bytes),
            })
        }
    }
}

// a dropped picture takes its file with it
fn forget(clip: &Clip) {
    if let ClipKind::Image { path, .. } = &clip.kind {
        let _ = fs::remove_file(path);
    }
}

fn hash(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();

    bytes.hash(&mut hasher);

    hasher.finish()
}

// only this user can read the pictures, which may well be private
fn save(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder())?;

    fs::write(path, bytes)
}

fn folder() -> PathBuf {
    let runtime = env::var_os("XDG_RUNTIME_DIR").map_or_else(env::temp_dir, PathBuf::from);

    runtime.join("amane").join("clipboard")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| String::from(*name)).collect()
    }

    #[test]
    fn leaves_out_what_password_managers_copy() {
        let password = types(&["text/plain;charset=utf-8", "x-kde-passwordManagerHint"]);

        assert_eq!(pick(&password), None);

        // a paste the user asked for still gets it
        assert_eq!(text_type(&password), Some("text/plain;charset=utf-8"));
    }

    #[test]
    fn leaves_out_its_own_copies() {
        assert_eq!(pick(&types(&["UTF8_STRING", OURS])), None);
    }

    #[test]
    fn reads_text_before_pictures() {
        let both = types(&["image/png", "text/html", "STRING", "UTF8_STRING"]);

        assert_eq!(pick(&both), Some("UTF8_STRING"));
        assert_eq!(
            pick(&types(&["image/jpeg", "image/png"])),
            Some("image/png")
        );
        assert_eq!(pick(&types(&["text/html", "application/x-other"])), None);
    }

    #[test]
    fn keeps_a_repeat_once_at_the_top() {
        let mut clipboard = Clipboard {
            clips: Vec::new(),
            next: 1,
            current: None,
            copies: 0,
        };

        let first = clipboard.keep(ClipKind::Text(String::from("one")), hash(b"one"));

        clipboard.keep(ClipKind::Text(String::from("two")), hash(b"two"));

        let again = clipboard.keep(ClipKind::Text(String::from("one")), hash(b"one"));

        assert_eq!(again, first);
        assert_eq!(clipboard.clips.len(), 2);
        assert_eq!(clipboard.clips[0].text(), Some("one"));
    }

    #[test]
    fn drops_the_oldest_past_the_limit() {
        let mut clipboard = Clipboard {
            clips: Vec::new(),
            next: 1,
            current: None,
            copies: 0,
        };

        for number in 0..=KEPT {
            let text = number.to_string();

            clipboard.keep(ClipKind::Text(text.clone()), hash(text.as_bytes()));
        }

        assert_eq!(clipboard.clips.len(), KEPT);
        assert_eq!(clipboard.clips[0].text(), Some(KEPT.to_string().as_str()));
        assert_eq!(clipboard.clips[KEPT - 1].text(), Some("1"));
    }
}
