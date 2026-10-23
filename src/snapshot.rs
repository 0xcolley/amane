/*
 * a picture of one window without a compositor: `amane snapshot` starts the
 * shell with AMANE_SNAPSHOT set, and instead of connecting to wayland the
 * shell runs the asked ipc calls, lets its views settle, draws the window
 * offscreen and writes it to a png
 */

use std::env;
use std::ffi::c_int;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use png::{BitDepth, ColorType, Encoder};

use crate::changes;
use crate::frame;
use crate::graphics::image::{self, Bitmap};
use crate::graphics::{Backdrop, Gpu};
use crate::ipc::{IpcCall, IpcHandlers};
use crate::layer_window::{Horizontal, Vertical, WindowSize};
use crate::scale::ScaleFactor;
use crate::{LayerWindow, Monitor};

const REQUEST: &str = "AMANE_SNAPSHOT";

// nothing moving or changing for this long counts as settled
const CALM: Duration = Duration::from_millis(400);

// a view that never settles, like one with a shader that runs on time, is drawn anyway after this
const LONGEST: Duration = Duration::from_secs(5);

// how far glass blurs what's behind it, in logical pixels, close to niri's blur
const BLUR: f32 = 12.0;

// the blur runs on a copy this many times smaller, which looks the same and takes a fraction of the time
const BLUR_SHRINK: u32 = 4;

unsafe extern "C" {
    fn _exit(status: c_int) -> !;
}

/// True while the shell runs only to take a snapshot, so it can skip writing files or starting
/// helpers.
pub fn headless() -> bool {
    env::var_os(REQUEST).is_some()
}

// what to take a picture of, one `key value` per line of AMANE_SNAPSHOT
pub(crate) struct Request {
    // a layer window's namespace, or "lock" for the lock screen
    window: String,
    out: PathBuf,
    scale: f32,

    // the monitor per-monitor views and full-size windows are drawn for, in widget units
    monitor: Monitor,

    calls: Vec<IpcCall>,
    over: Option<Over>,
}

// what the window is shown over, since nothing is behind it without a compositor
enum Over {
    Color([u8; 3]),
    Picture(PathBuf),
}

// the views of the shell, as App holds them
pub(crate) struct Views<'a> {
    pub windows: &'a [fn() -> LayerWindow],
    pub per_monitor: &'a [fn(&Monitor) -> LayerWindow],
    pub lock: Option<fn(&Monitor) -> LayerWindow>,
}

#[derive(Clone, Copy)]
enum Found {
    Plain(fn() -> LayerWindow),
    Monitor(fn(&Monitor) -> LayerWindow),
}

impl Found {
    fn run(self, monitor: &Monitor) -> LayerWindow {
        match self {
            Found::Plain(view) => view(),
            Found::Monitor(view) => view(monitor),
        }
    }
}

pub(crate) fn request() -> Option<Result<Request, String>> {
    let text = env::var(REQUEST).ok()?;

    Some(parse(&text))
}

fn parse(text: &str) -> Result<Request, String> {
    let mut request = Request {
        window: String::new(),
        out: PathBuf::from("snapshot.png"),
        scale: 1.0,
        monitor: Monitor {
            name: String::from("snapshot"),
            width: 1440,
            height: 960,
        },
        calls: Vec::new(),
        over: None,
    };

    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        let value = value.trim();

        match key {
            "window" => request.window = String::from(value),

            "out" => request.out = PathBuf::from(value),

            "scale" => {
                request.scale = value
                    .parse()
                    .ok()
                    .filter(|scale: &f32| scale.is_finite() && *scale > 0.0)
                    .ok_or_else(|| format!("failed to read the scale {value:?}"))?;
            }

            "size" => {
                let (width, height) = value
                    .split_once('x')
                    .and_then(|(width, height)| Some((width.parse().ok()?, height.parse().ok()?)))
                    .filter(|&(width, height): &(u32, u32)| width > 0 && height > 0)
                    .ok_or_else(|| {
                        format!("failed to read the size {value:?}, expected WIDTHxHEIGHT")
                    })?;

                request.monitor.width = width;
                request.monitor.height = height;
            }

            "monitor" => request.monitor.name = String::from(value),

            "call" => {
                let mut words = value.split_whitespace();

                let name = words.next().ok_or("failed to read a call: no name")?;
                let arguments: Vec<String> = words.map(String::from).collect();

                request.calls.push(IpcCall::new(name, &arguments));
            }

            "over" => {
                request.over = Some(match value.strip_prefix('#') {
                    Some(hex) => Over::Color(
                        hex_color(hex)
                            .ok_or_else(|| format!("failed to read the color {value:?}"))?,
                    ),
                    None => Over::Picture(PathBuf::from(value)),
                });
            }

            _ => return Err(format!("failed to read {line:?}: unknown key {key:?}")),
        }
    }

    if request.window.is_empty() {
        return Err(String::from("failed to take a snapshot: no window named"));
    }

    Ok(request)
}

fn hex_color(hex: &str) -> Option<[u8; 3]> {
    if hex.len() != 6 {
        return None;
    }

    let channel = |at: usize| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok();

    Some([channel(0)?, channel(2)?, channel(4)?])
}

/*
 * ends the process without tearing anything down: service threads like
 * pulseaudio's are still running, and freeing what they use under them
 * crashes on the way out
 */
pub(crate) fn leave(code: c_int) -> ! {
    let _ = io::stdout().flush();
    let _ = io::stderr().flush();

    // nothing is left to finish, the picture is already written
    unsafe { _exit(code) }
}

// the path the picture went to
pub(crate) fn take(
    request: &Request,
    views: &Views,
    handlers: &IpcHandlers,
    scale_factor: ScaleFactor,
) -> Result<PathBuf, String> {
    for call in &request.calls {
        let reply = handlers.run(call);

        if !reply.trim().is_empty() {
            eprintln!("amane: {}: {}", call.name, reply.trim_end());
        }
    }

    let monitor = &request.monitor;

    let found = find(&request.window, views, monitor, scale_factor)?;

    let started = Instant::now();
    let mut quiet = Instant::now();

    // the monitor's size until the view says its own
    let mut size = (
        scale_factor.pixels(monitor.width as f32),
        scale_factor.pixels(monitor.height as f32),
    );

    let (window, frame) = loop {
        let (mut window, _) = frame::run_view(|| found.run(monitor), size.0, size.1, scale_factor);

        size = window_size(&window, monitor, scale_factor);

        let Some(root) = window.root.take() else {
            return Err(format!("{} has no child to draw", window.namespace));
        };

        let frame = frame::build(root.as_ref(), size.0, size.1, request.scale, scale_factor);

        // none means everything changed
        let changed = changes::take().is_none_or(|services| !services.is_empty());

        if frame.moving || changed {
            quiet = Instant::now();
        }

        if quiet.elapsed() >= CALM || started.elapsed() >= LONGEST {
            break (window, frame);
        }

        thread::sleep(Duration::from_millis(16));
    };

    if !window.visible {
        return Err(format!(
            "{} is hidden, open it first with --ipc, like --ipc \"control\"",
            window.namespace
        ));
    }

    let width = real_pixels(size.0, request.scale);
    let height = real_pixels(size.1, request.scale);

    let mut renderer = frame.renderer;

    let backdrops = renderer.take_backdrops();

    let mut gpu = Gpu::offscreen();

    let pixels = gpu
        .picture(renderer.finish(), width, height)
        .ok_or("failed to read the picture back from the gpu")?;

    drop(gpu);

    let pixels = match &request.over {
        None => unpremultiply(pixels),

        Some(over) => {
            let place = place(&window, size, monitor, scale_factor, request.scale);

            let behind = behind(over, place, request.scale)?;

            composite(&pixels, &behind, &backdrops)
        }
    };

    write_png(&request.out, width, height, &pixels)?;

    Ok(request.out.clone())
}

// the view whose window has the asked namespace, or the lock screen
fn find(
    name: &str,
    views: &Views,
    monitor: &Monitor,
    scale_factor: ScaleFactor,
) -> Result<Found, String> {
    if name == "lock" {
        return views
            .lock
            .map(Found::Monitor)
            .ok_or_else(|| String::from("the shell has no lock screen, see App::lock"));
    }

    let candidates = views
        .windows
        .iter()
        .map(|view| Found::Plain(*view))
        .chain(views.per_monitor.iter().map(|view| Found::Monitor(*view)));

    let mut names = Vec::new();

    for found in candidates {
        let width = scale_factor.pixels(monitor.width as f32);
        let height = scale_factor.pixels(monitor.height as f32);

        let (window, _) = frame::run_view(|| found.run(monitor), width, height, scale_factor);

        if window.namespace == name {
            return Ok(found);
        }

        names.push(window.namespace);
    }

    Err(format!(
        "no window named {name}, the shell has: {}, lock",
        names.join(", ")
    ))
}

// in surface units, as the compositor would configure it
fn window_size(window: &LayerWindow, monitor: &Monitor, scale_factor: ScaleFactor) -> (u32, u32) {
    let side = |size: WindowSize, full: u32| match size {
        WindowSize::Full => scale_factor.pixels(full as f32),
        WindowSize::Fixed(pixels) => scale_factor.pixels(pixels),
    };

    (
        side(window.width, monitor.width),
        side(window.height, monitor.height),
    )
}

fn real_pixels(size: u32, scale: f32) -> u32 {
    ((size as f32 * scale).round() as u32).max(1)
}

// where the window sits on the monitor, in real pixels, from its anchors and margins
#[derive(Clone, Copy)]
struct Place {
    x: i64,
    y: i64,
    width: u32,
    height: u32,

    monitor_width: u32,
    monitor_height: u32,
}

fn place(
    window: &LayerWindow,
    size: (u32, u32),
    monitor: &Monitor,
    scale_factor: ScaleFactor,
    scale: f32,
) -> Place {
    let monitor_width = scale_factor.pixels(monitor.width as f32);
    let monitor_height = scale_factor.pixels(monitor.height as f32);

    let margin = window.margin;

    let x = match window.horizontal {
        Horizontal::Left => scale_factor.coordinate(margin.left),
        Horizontal::Right => {
            monitor_width as i32 - size.0 as i32 - scale_factor.coordinate(margin.right)
        }
        Horizontal::Middle => (monitor_width as i32 - size.0 as i32) / 2,
    };

    let y = match window.vertical {
        Vertical::Top => scale_factor.coordinate(margin.top),
        Vertical::Bottom => {
            monitor_height as i32 - size.1 as i32 - scale_factor.coordinate(margin.bottom)
        }
        Vertical::Middle => (monitor_height as i32 - size.1 as i32) / 2,
    };

    Place {
        x: (x as f32 * scale).round() as i64,
        y: (y as f32 * scale).round() as i64,
        width: real_pixels(size.0, scale),
        height: real_pixels(size.1, scale),
        monitor_width: real_pixels(monitor_width, scale),
        monitor_height: real_pixels(monitor_height, scale),
    }
}

// what shows through the window, sharp and blurred, each the window's size
struct Behind {
    sharp: Bitmap,
    blurred: Bitmap,
}

fn behind(over: &Over, place: Place, scale: f32) -> Result<Behind, String> {
    let (width, height) = (place.width, place.height);

    let sharp = match over {
        Over::Color(color) => {
            let pixel = [color[0], color[1], color[2], 255];

            Bitmap::new(width, height, pixel.repeat((width * height) as usize))
        }

        Over::Picture(path) => cut_out(path, place)?,
    };

    // a plain color looks the same blurred
    let blurred = match over {
        Over::Color(_) => Bitmap::new(width, height, sharp.pixels.to_vec()),

        Over::Picture(_) => {
            let small = image::to_cover(
                Bitmap::new(width, height, sharp.pixels.to_vec()),
                (width / BLUR_SHRINK).max(1),
                (height / BLUR_SHRINK).max(1),
            );

            let radius = (BLUR * scale / BLUR_SHRINK as f32).round().max(1.0) as u32;

            image::soften(small, radius)
        }
    };

    Ok(Behind { sharp, blurred })
}

// the part of the picture under the window, with the picture covering the whole monitor like a wallpaper
fn cut_out(path: &Path, place: Place) -> Result<Bitmap, String> {
    let picture = image::read(path)
        .ok_or_else(|| format!("failed to read the picture {}", path.display()))?;

    let picture = image::to_cover(picture, place.monitor_width, place.monitor_height);

    // the cover is centered on the monitor, a picture smaller than it is stretched
    let stretch = f32::max(
        place.monitor_width as f32 / picture.width() as f32,
        place.monitor_height as f32 / picture.height() as f32,
    )
    .max(1.0);

    let left = (picture.width() as f32 * stretch - place.monitor_width as f32) / 2.0;
    let top = (picture.height() as f32 * stretch - place.monitor_height as f32) / 2.0;

    let mut pixels = Vec::with_capacity((place.width * place.height * 4) as usize);

    for y in 0..place.height {
        for x in 0..place.width {
            let source_x = ((left + (place.x + i64::from(x)) as f32) / stretch) as i64;
            let source_y = ((top + (place.y + i64::from(y)) as f32) / stretch) as i64;

            let source_x = source_x.clamp(0, i64::from(picture.width()) - 1) as usize;
            let source_y = source_y.clamp(0, i64::from(picture.height()) - 1) as usize;

            let at = (source_y * picture.width() as usize + source_x) * 4;

            pixels.extend_from_slice(&picture.pixels[at..at + 4]);
        }
    }

    Ok(Bitmap::new(place.width, place.height, pixels))
}

// the window over what's behind it, blurred inside its backdrops as the compositor would
fn composite(window: &[u8], behind: &Behind, backdrops: &[Backdrop]) -> Vec<u8> {
    let width = behind.sharp.width();

    let mut pixels = Vec::with_capacity(window.len());

    for (index, front) in window.as_chunks::<4>().0.iter().enumerate() {
        let x = (index as u32 % width) as f32 + 0.5;
        let y = (index as u32 / width) as f32 + 0.5;

        let back = if backdrops.iter().any(|backdrop| inside(backdrop, x, y)) {
            sample(
                &behind.blurred,
                x / width as f32,
                y / behind.sharp.height() as f32,
            )
        } else {
            let at = index * 4;

            [
                behind.sharp.pixels[at],
                behind.sharp.pixels[at + 1],
                behind.sharp.pixels[at + 2],
            ]
        };

        // the window's colors are premultiplied, so its alpha says how much of the back still shows
        let through = 255 - u32::from(front[3]);

        for channel in 0..3 {
            let mixed =
                u32::from(front[channel]) + (u32::from(back[channel]) * through + 127) / 255;

            pixels.push(mixed.min(255) as u8);
        }

        pixels.push(255);
    }

    pixels
}

// whether a point in real pixels is inside the backdrop's rounded rectangle
fn inside(backdrop: &Backdrop, x: f32, y: f32) -> bool {
    let area = backdrop.area;

    let (left, top) = backdrop.transform.map(area.x, area.y);
    let (right, bottom) = backdrop
        .transform
        .map(area.x + area.width, area.y + area.height);

    if x < left || x > right || y < top || y > bottom {
        return false;
    }

    let factor = backdrop.transform.sx;
    let radius = backdrop.radius;

    // the corner the point is nearest, and its radius in real pixels
    let (corner_x, corner_y, radius) = match (x < (left + right) / 2.0, y < (top + bottom) / 2.0) {
        (true, true) => (left, top, radius.top_left),
        (false, true) => (right, top, radius.top_right),
        (true, false) => (left, bottom, radius.bottom_left),
        (false, false) => (right, bottom, radius.bottom_right),
    };

    let radius = radius * factor;

    let center_x = if corner_x == left {
        left + radius
    } else {
        right - radius
    };
    let center_y = if corner_y == top {
        top + radius
    } else {
        bottom - radius
    };

    let beyond_x = if corner_x == left {
        x < center_x
    } else {
        x > center_x
    };
    let beyond_y = if corner_y == top {
        y < center_y
    } else {
        y > center_y
    };

    if !(beyond_x && beyond_y) {
        return true;
    }

    let (dx, dy) = (x - center_x, y - center_y);

    dx * dx + dy * dy <= radius * radius
}

// the color at a spot given as a share of the image's width and height, between its four nearest pixels
fn sample(image: &Bitmap, across: f32, down: f32) -> [u8; 3] {
    let width = image.width() as usize;
    let height = image.height() as usize;

    let x = (across * width as f32 - 0.5).clamp(0.0, (width - 1) as f32);
    let y = (down * height as f32 - 0.5).clamp(0.0, (height - 1) as f32);

    let (left, top) = (x.floor() as usize, y.floor() as usize);
    let (right, bottom) = ((left + 1).min(width - 1), (top + 1).min(height - 1));

    let (along, below) = (x - left as f32, y - top as f32);

    let pixel =
        |x: usize, y: usize, channel: usize| f32::from(image.pixels[(y * width + x) * 4 + channel]);

    let mut color = [0; 3];

    for (channel, value) in color.iter_mut().enumerate() {
        let upper = pixel(left, top, channel) * (1.0 - along) + pixel(right, top, channel) * along;
        let lower =
            pixel(left, bottom, channel) * (1.0 - along) + pixel(right, bottom, channel) * along;

        *value = (upper * (1.0 - below) + lower * below).round() as u8;
    }

    color
}

// png keeps plain alpha, the gpu gives premultiplied
fn unpremultiply(mut pixels: Vec<u8>) -> Vec<u8> {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);

        if alpha == 0 || alpha == 255 {
            continue;
        }

        for channel in &mut pixel[..3] {
            *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }

    pixels
}

fn write_png(path: &Path, width: u32, height: u32, pixels: &[u8]) -> Result<(), String> {
    let failed =
        |error: &dyn std::fmt::Display| format!("failed to write {}: {error}", path.display());

    let file = File::create(path).map_err(|error| failed(&error))?;

    let mut encoder = Encoder::new(BufWriter::new(file), width, height);

    encoder.set_color(ColorType::Rgba);
    encoder.set_depth(BitDepth::Eight);

    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(pixels))
        .map_err(|error| failed(&error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphics::{Area, Corners, Transform};

    #[test]
    fn reads_a_request() {
        let request = parse(
            "window amane-control\nout /tmp/control.png\nscale 2\nsize 1280x800\ncall control wifi\nover #1e2030\n",
        )
        .expect("failed to read the test request");

        assert_eq!(request.window, "amane-control");
        assert_eq!(request.out, PathBuf::from("/tmp/control.png"));
        assert_eq!(request.scale, 2.0);
        assert_eq!((request.monitor.width, request.monitor.height), (1280, 800));
        assert_eq!(request.calls.len(), 1);
        assert_eq!(request.calls[0].name, "control");
        assert_eq!(request.calls[0].arguments, vec![String::from("wifi")]);
        assert!(matches!(
            request.over,
            Some(Over::Color([0x1e, 0x20, 0x30]))
        ));
    }

    #[test]
    fn refuses_a_request_without_a_window() {
        assert!(parse("scale 2").is_err());
        assert!(parse("window bar\nscale -1").is_err());
        assert!(parse("window bar\nsize 12").is_err());
        assert!(parse("window bar\nshade blue").is_err());
    }

    #[test]
    fn rounded_corners_are_outside() {
        let backdrop = Backdrop {
            area: Area::new(0.0, 0.0, 10.0, 10.0),
            radius: Corners::from(5.0),
            transform: Transform::from_scale(2.0, 2.0),
        };

        // the middle and the edges' middles are in, the very corners are cut off
        assert!(inside(&backdrop, 10.0, 10.0));
        assert!(inside(&backdrop, 0.5, 10.0));
        assert!(!inside(&backdrop, 0.5, 0.5));
        assert!(!inside(&backdrop, 19.5, 19.5));
        assert!(!inside(&backdrop, 21.0, 10.0));
    }

    #[test]
    fn premultiplied_colors_come_back_plain() {
        assert_eq!(unpremultiply(vec![64, 0, 32, 128]), vec![128, 0, 64, 128]);
        assert_eq!(unpremultiply(vec![0, 0, 0, 0]), vec![0, 0, 0, 0]);
    }

    #[test]
    fn a_window_over_a_color() {
        let behind = Behind {
            sharp: Bitmap::new(1, 1, vec![0, 0, 200, 255]),
            blurred: Bitmap::new(1, 1, vec![0, 0, 200, 255]),
        };

        // half transparent red over blue
        assert_eq!(
            composite(&[128, 0, 0, 128], &behind, &[]),
            vec![128, 0, 100, 255]
        );
    }
}
