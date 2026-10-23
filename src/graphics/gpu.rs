mod atlas;
mod blur;
mod clip;
mod convert;
mod cut;
mod dispatch;
mod encoder;
mod flush;
mod forget;
mod glyph;
mod gradient;
mod group;
mod image;
mod paint;
mod painted;
mod pass;
mod picture;
mod pool;
mod present;
mod quads;
mod readback;
mod shade;
mod shader;
mod shadow;
mod shape;
mod shared;
mod target;
mod texture;
mod wait;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use vello::peniko::ImageData;
use vello::wgpu::{BlendState, CommandEncoder, Device, Queue, Texture, TextureFormat};

use crate::graphics::image::Bitmap;

use pass::Pass;
use present::Presenter;
use shader::Shader;
use shared::Shared;

// Outlives the backend so cached GPU resources are released after every window.
pub(crate) struct Session;

impl Drop for Session {
    fn drop(&mut self) {
        shared::clear();
    }
}

/*
 * the only part of amane that knows how drawing is done,
 * vello draws the shapes and wgpu runs everything else on the gpu
 */
pub struct Gpu {
    device: Device,
    queue: Queue,

    // what shows the canvas in a window, none for a gpu that only draws offscreen
    presenter: Option<Presenter>,

    // one renderer for every window, see shared.rs
    vello: Rc<RefCell<vello::Renderer>>,

    // images already turned into vello's form, keyed by where the loaded image lives
    images: HashMap<usize, ImageData>,

    // images this window keeps copies of, and the ones it drew this frame
    shown: HashMap<usize, Arc<Bitmap>>,
    drawn: HashSet<usize>,

    // letters already drawn into pictures, none for letters without pixels like a space
    glyphs: HashMap<glyph::GlyphKey, Option<glyph::Glyph>>,

    quads: quads::Quads,

    // the shared count of atlas drops, and the one this window last sent its images again after
    atlas_drops: Rc<Cell<u64>>,
    atlas_seen: u64,

    // custom shaders, compiled once and kept by the path they were read from
    shaders: HashMap<PathBuf, Shader>,

    // finished window-sized textures waiting to be reused
    spare: Vec<Texture>,

    // last frame's vello results in the order they were painted, and how far this frame got
    painted: Vec<Option<painted::Painted>>,
    painting: usize,

    // the frame's jobs not mailed yet; none between frames and right after mailing
    pending: RefCell<Option<CommandEncoder>>,

    composite: Pass,
    box_blur: Pass,
    erase: Pass,
}

impl Gpu {
    // the pointers are libwayland's display and surface, which have to outlive the gpu
    pub fn new(display: *mut c_void, surface: *mut c_void) -> Self {
        let instance = shared::instance(display);

        let surface = target::create(&instance, display, surface);

        let shared = shared::get(&instance, Some(&surface));

        let config = target::configure(&surface, &shared.adapter);

        let pass = Pass::new(&shared.device, "composite", config.format, None);

        let presenter = Presenter {
            surface,
            config,
            pass,
        };

        Self::open(shared, Some(presenter))
    }

    // draws only into textures, for pictures of windows taken without a compositor
    pub fn offscreen() -> Self {
        let instance = shared::offscreen_instance();

        let shared = shared::get(&instance, None);

        Self::open(shared, None)
    }

    fn open(shared: Shared, presenter: Option<Presenter>) -> Self {
        let Shared {
            device,
            queue,
            vello,
            atlas_drops,
            ..
        } = shared;

        // the canvas and the surface hold premultiplied colors
        let over = Some(BlendState::PREMULTIPLIED_ALPHA_BLENDING);

        let composite = Pass::new(&device, "composite", TextureFormat::Rgba8Unorm, over);
        let box_blur = Pass::new(&device, "box_blur", TextureFormat::Rgba8Unorm, None);
        let erase = cut::erase(&device);

        let quads = quads::Quads::new(&device, &queue);

        Self {
            device,
            queue,

            presenter,

            vello,

            images: HashMap::new(),
            shown: HashMap::new(),
            drawn: HashSet::new(),
            glyphs: HashMap::new(),
            quads,
            atlas_drops,
            atlas_seen: 0,

            shaders: HashMap::new(),
            spare: Vec::new(),

            painted: Vec::new(),
            painting: 0,

            pending: RefCell::new(None),

            composite,
            box_blur,
            erase,
        }
    }
}
