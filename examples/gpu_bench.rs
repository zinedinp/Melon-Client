//! GPU time of the terrain passes on the frozen chunks of `pipeline_bench`,

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use glam::Vec3;
use numerobis::vulkan::swapchain::PresentMode;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

use melon::game::camera;
use melon::game::level_meshes::LevelMeshes;
use melon_base::gfx::quad::DrawList;
use melon_core::math::euler::{Look, Orientation};
use melon_render::{EntityDraw, Frame, RenderLayer, Renderer, SceneFrame, WorldDraw};
use melon_world::block_models::BlockModels;
use melon_world::culling::frustum::Frustum;
use melon_world::culling::{CullSettings, Culler};
use melon_world::fluid::FluidSprites;
use melon_world::level::Level;
use melon_world::lightmap::LightmapParams;
use melon_world::projection::world_projection;

const RADIUS: i32 = 8;
const CHUNKS: &str = "target/pipeline-bench/chunks.bin";
const ROUNDS: usize = 5;
const WARMUP: usize = 20;
const FRAMES: usize = 90;
/// The cloud colour (white, alpha 0.8); the mesh no longer carries it.
const CLOUD_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.8];

struct Bench {
    result: Option<anyhow::Result<()>>,
}

impl ApplicationHandler for Bench {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.result.is_none() {
            self.result = Some(run(event_loop));
            event_loop.exit();
        }
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: WindowEvent) {}
}

fn main() -> anyhow::Result<()> {
    melon_base::platform::drop_cargo_stack();
    let event_loop = EventLoop::new()?;
    let mut bench = Bench { result: None };
    event_loop.run_app(&mut bench)?;
    bench.result.unwrap_or(Ok(()))
}

fn run(event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
    let bytes = std::fs::read(CHUNKS).context("no frozen chunks: run pipeline_bench first")?;
    let (spawn, packets) = read_frozen(&bytes).context("chunks.bin unreadable")?;
    let mut level = Level::default();
    for packet in &packets {
        level.insert(melon_net::play::level::chunk::decode(
            packet,
            melon_world::dimension::Height::OVERWORLD,
        )?);
    }
    println!("{} Chunks", packets.len());

    let window = event_loop.create_window(
        Window::default_attributes()
            .with_title("Melon GPU-Bench")
            .with_inner_size({
                let size = std::env::var("MELON_BENCH_SIZE").ok().and_then(|text| {
                    let (w, h) = text.split_once('x')?;
                    Some((w.parse().ok()?, h.parse().ok()?))
                });
                let (w, h) = size.unwrap_or((1920, 1080));
                PhysicalSize::new(w, h)
            })
            .with_fullscreen(std::env::var_os("MELON_BENCH_FULLSCREEN").map(|_| {
                let largest = event_loop.available_monitors().max_by_key(|m| {
                    let size = m.size();
                    u64::from(size.width) * u64::from(size.height)
                });
                winit::window::Fullscreen::Borderless(largest)
            })),
    )?;
    let mut renderer = Renderer::new(&window, std::env::var_os("MELON_VALIDATION").is_some())?;
    let requested =
        std::env::var("MELON_BENCH_PRESENT")
            .ok()
            .and_then(|name| match name.as_str() {
                "fifo" => Some(PresentMode::Fifo),
                "mailbox" => Some(PresentMode::Mailbox),
                "immediate" => Some(PresentMode::Immediate),
                "latest" => Some(PresentMode::FifoLatestReady),
                _ => None,
            });
    renderer.set_present(false, requested)?;
    println!("Present mode: {}", renderer.present_mode_name());

    let assets = melon_assets::assets::Assets::discover()?;
    let extras = FluidSprites::TEXTURES
        .iter()
        .map(|id| melon_world::block_models::ExtraTexture {
            id: (*id).to_owned(),
            image: None,
        })
        .collect();
    let (models, atlas) = BlockModels::bake(assets.root(), extras, 16384);
    renderer.set_quad_table(models.quad_corners())?;
    let atlas_id = renderer.load_texture_mipmapped(&atlas.image, &atlas.mips)?;
    let sprite = |id: &str| models.sprite(id).unwrap_or([0.0; 4]);
    let fluids = FluidSprites {
        water_still: sprite(FluidSprites::TEXTURES[0]),
        water_flow: sprite(FluidSprites::TEXTURES[1]),
        water_overlay: sprite(FluidSprites::TEXTURES[2]),
        lava_still: sprite(FluidSprites::TEXTURES[3]),
        lava_flow: sprite(FluidSprites::TEXTURES[4]),
    };
    let models = Arc::new(models);

    let mut finished = Vec::new();
    let t = Instant::now();
    loop {
        finished.extend(level.mesh_pending(&models, fluids, spawn.as_dvec3()));
        let stats = level.mesh_stats();
        if stats.waiting == 0 && stats.in_flight == 0 && !finished.is_empty() {
            break;
        }
        if t.elapsed() > Duration::from_secs(120) {
            bail!("meshing not done after 120 s");
        }
        std::thread::sleep(Duration::from_micros(250));
    }
    println!("{} sections meshed", finished.len());
    let mut meshes = LevelMeshes::default();
    meshes.upload(&mut renderer, finished);

    let surface = (-64..320)
        .rev()
        .find(|&y| {
            level
                .block_state(spawn.x.floor() as i32, y, spawn.z.floor() as i32)
                .is_some_and(|s| s != 0)
        })
        .unwrap_or(64);
    let eye_y = surface as f32 + 1.0 + melon_world::player::EYE_HEIGHT;
    let views = [
        ("north", Vec3::new(spawn.x, eye_y, spawn.z), 180.0, 0.0),
        ("east", Vec3::new(spawn.x, eye_y, spawn.z), 270.0, 0.0),
        ("south", Vec3::new(spawn.x, eye_y, spawn.z), 0.0, 0.0),
        ("west", Vec3::new(spawn.x, eye_y, spawn.z), 90.0, 0.0),
        ("down", Vec3::new(spawn.x, eye_y + 30.0, spawn.z), 0.0, 60.0),
        ("underground", Vec3::new(spawn.x, -20.0, spawn.z), 0.0, 0.0),
    ];
    let (min_y, height) = level.height();
    let far = camera::far_plane(RADIUS as u8);
    let (width, heights) = renderer.framebuffer_size();
    let aspect = width as f32 / heights as f32;
    let projection = world_projection(70.0, aspect, far);
    let ui = DrawList::new();

    println!("\n{width}x{heights}, GPU time in ms, median of {ROUNDS} rounds of {FRAMES} frames");
    println!(
        "{:<12} {:>9} {:>9} {:>9} {:>11} {:>11} {:>9} {:>9}",
        "Camera", "Frame", "solid", "cutout", "translucent", "atmosphere", "cpu", "blur"
    );
    let blur = std::env::var_os("MELON_BENCH_BLUR").map(|_| melon_render::Blur {
        radius: 5.0,
        ui_split: 0,
    });
    let rain_on = std::env::var_os("MELON_BENCH_RAIN").is_some();
    let clouds_on = std::env::var_os("MELON_BENCH_CLOUDS").is_some();
    let rain_image = assets.image("minecraft:environment/rain.png")?;
    let rain_texture = renderer.load_texture_tiled(&rain_image)?;
    let rain_template = renderer.upload_mesh(&melon_world::weather::template(
        &melon_world::weather::streaks(&rain_image),
        rain_image.width(),
        rain_image.height(),
    ))?;
    let white = renderer.white_texture();
    let cloud_texture = melon_world::clouds::CloudTexture::load(assets.root());
    for (name, eye, yaw, pitch) in views {
        let eye = eye.as_dvec3();
        let view = Orientation::new(Look::from_melon(yaw, pitch)).view;
        let culler = Culler::new(
            Frustum::from_view_projection(projection * view),
            None,
            eye,
            (min_y, height),
            CullSettings::default(),
        );
        let mask = level.occlusion_walk(culler.section_of(eye), RADIUS, None);
        let culler = culler.with_occlusion(&mask);

        let fog = if std::env::var_os("MELON_BENCH_NOFOG").is_some() {
            melon_world::fog::FogData::NONE
        } else {
            melon_world::fog::FogData {
                color: [0.75, 0.85, 1.0, 1.0],
                environmental_start: 0.0,
                environmental_end: 1024.0,
                render_distance_start: 172.8,
                render_distance_end: 192.0,
                sky_end: 192.0,
                cloud_end: 192.0,
            }
        };
        let mut atmosphere_meshes = Vec::new();
        let mut atmosphere: Vec<EntityDraw> = Vec::new();
        let mut weather: Vec<melon_render::WeatherDraw> = Vec::new();
        if rain_on {
            let height = |x, z| level.motion_blocking_height(x, z);
            let precipitation = |_, _, _| melon_world::weather::Precipitation::Rain;
            let light = |x, y, z| level.light_at(x, y, z);
            let radius = 10;
            let columns = melon_world::weather::columns(melon_world::weather::WeatherQuery {
                intensity: 1.0,
                radius,
                camera: eye,
                game_time: 1000,
                partial_tick: 0.5,
                height: &height,
                precipitation: &precipitation,
                light: &light,
            });
            let min: f32 = std::env::var("MELON_BENCH_RAIN_MIN")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0);
            let columns: Vec<_> = columns
                .into_iter()
                .filter(|c| {
                    let dx = (f64::from(c.x) + 0.5 - eye.x) as f32;
                    let dz = (f64::from(c.z) + 0.5 - eye.z) as f32;
                    dx * dx + dz * dz >= min * min
                })
                .collect();
            let (rain, _) = melon_world::weather::pieces(
                &columns,
                eye,
                radius,
                1.0,
                &LightmapParams::overworld([1.0; 3], 0.0),
                |_, _| true,
            );
            let pieces = renderer.upload_mesh(&rain.vertices)?;
            atmosphere_meshes.push(pieces);
            weather.push(melon_render::WeatherDraw {
                template: rain_template,
                pieces,
                wraps: rain.wraps,
                atlas: rain_texture,
                model: glam::Mat4::from_translation((eye - eye.floor()).as_vec3()),
                origin: eye.floor().as_ivec3(),
            });
        }
        if clouds_on {
            let frame = melon_world::clouds::CloudFrame {
                status: melon_world::clouds::CloudStatus::Fancy,
                color: CLOUD_COLOR,
                bottom_y: 192.0,
                range_chunks: RADIUS as u8,
                camera: eye,
                game_time: 1000,
                partial_tick: 0.5,
            };
            let verts = melon_world::clouds::build_anchored(&cloud_texture, frame);
            let anchor = melon_world::clouds::anchor(&cloud_texture, &frame);
            if !verts.is_empty() {
                let mesh = renderer.upload_mesh(&verts)?;
                atmosphere_meshes.push(mesh);
                atmosphere.push(EntityDraw {
                    scroll: None,
                    alpha: 1.0,
                    mesh,
                    atlas: white,
                    model: glam::Mat4::from_translation((anchor - anchor.floor()).as_vec3()),
                    origin: anchor.floor().as_ivec3(),
                    layer: RenderLayer::Atmosphere,
                    blend: melon_render::Blending::Translucent,
                    light: None,
                    overlay: 0.0,
                    glint: melon_render::Glint::None,
                });
            }
        }
        let mut rounds = Vec::new();
        for _ in 0..ROUNDS {
            let mut frames = Vec::new();
            for frame in 0..WARMUP + FRAMES {
                meshes.upload(&mut renderer, Vec::new());
                let visible = meshes.visible_sections(&culler);
                let scene = SceneFrame {
                    sky: None,
                    world: Some(WorldDraw {
                        chunks: visible.solid,
                        cutout: visible.cutout,
                        translucent: visible.translucent,
                        atlas: atlas_id,
                        view,
                        eye,
                        fov_y_degrees: 70.0,
                        hand_fov_y_degrees: 70.0,
                        far,
                        ambient: [1.0; 3],
                        lightmap: LightmapParams::overworld([1.0; 3], 0.0),
                        fog,
                        cardinal: [1.0; 6],
                        distortion: glam::Mat4::IDENTITY,
                        bob: glam::Mat4::IDENTITY,
                        section_slots: visible.slots,
                    }),
                    entities: Vec::new(),
                    particles: Vec::new(),
                    clouds: atmosphere
                        .first()
                        .cloned()
                        .map(|draw| melon_render::CloudDraw {
                            draw,
                            color: CLOUD_COLOR,
                        }),
                    weather: weather.clone(),
                    lines: Vec::new(),
                    glint_item: None,
                    glint_armor: None,
                    glowing: Vec::new(),
                    post_effects: Vec::new(),
                };
                let started = Instant::now();
                renderer.render(&Frame {
                    scene: Some(&scene),
                    ui: &ui,
                    ui_size: (width as f32, heights as f32),
                    clear_color: [0.5, 0.7, 1.0, 1.0],
                    blur,
                    pictures: &[],
                })?;
                let waits = renderer.last_waits();
                let cpu = started
                    .elapsed()
                    .saturating_sub(waits.gpu + waits.acquire + waits.present);
                if let Some(world) = scene.world {
                    meshes.recycle(world.chunks, world.cutout, world.translucent);
                }
                if frame >= WARMUP {
                    let mut sample = Sample::parse(&renderer.gpu_profile());
                    sample.cpu = cpu.as_secs_f32() * 1000.0;
                    frames.push(sample);
                }
            }
            rounds.push(Sample::median(&frames));
        }
        let m = Sample::median(&rounds);
        let drawn = meshes.visible_sections(&culler);
        println!(
            "{name:<12} {:>9.3} {:>9.3} {:>9.3} {:>11.3} {:>11.3} {:>9.3} {:>9.3}   ({} Sections, {} Draws, {} Vertices)",
            m.total,
            m.phase("solid"),
            m.phase("cutout"),
            m.phase("translucent"),
            m.phase("atmosphere"),
            m.cpu,
            m.phase("blur"),
            drawn.count.drawn,
            drawn.count.draws,
            drawn.count.vertices,
        );
        meshes.recycle(drawn.solid, drawn.cutout, drawn.translucent);
        for mesh in atmosphere_meshes {
            renderer.free_mesh(mesh);
        }
    }
    Ok(())
}

/// One frame's GPU time from F3's lines.
#[derive(Clone, Default)]
struct Sample {
    total: f32,
    /// `Renderer::render` without what it waited on (fences, acquire,
    cpu: f32,
    phases: HashMap<String, f32>,
}

impl Sample {
    fn parse(lines: &[String]) -> Self {
        let mut sample = Self::default();
        for line in lines {
            let ms = |text: &str| text.split_whitespace().next()?.parse::<f32>().ok();
            if let Some(rest) = line.strip_prefix("Frame: ") {
                sample.total = ms(rest).unwrap_or(0.0);
            } else if let Some((name, rest)) = line.trim().split_once(": ")
                && let Some(v) = ms(rest)
            {
                sample.phases.insert(name.to_owned(), v);
            }
        }
        sample
    }

    fn phase(&self, name: &str) -> f32 {
        self.phases.get(name).copied().unwrap_or(0.0)
    }

    /// Per value the median of `samples`.
    fn median(samples: &[Self]) -> Self {
        let median = |mut values: Vec<f32>| {
            values.sort_by(f32::total_cmp);
            values.get(values.len() / 2).copied().unwrap_or(0.0)
        };
        let mut names: Vec<&String> = samples.iter().flat_map(|s| s.phases.keys()).collect();
        names.sort();
        names.dedup();
        Self {
            total: median(samples.iter().map(|s| s.total).collect()),
            cpu: median(samples.iter().map(|s| s.cpu).collect()),
            phases: names
                .into_iter()
                .map(|n| {
                    (
                        n.clone(),
                        median(samples.iter().map(|s| s.phase(n)).collect()),
                    )
                })
                .collect(),
        }
    }
}

fn read_frozen(bytes: &[u8]) -> Option<(Vec3, Vec<&[u8]>)> {
    let mut rest = bytes;
    let mut take = |n: usize| {
        let (head, tail) = rest.split_at_checked(n)?;
        rest = tail;
        Some(head)
    };
    let word = |b: &[u8]| <[u8; 4]>::try_from(b).ok();
    let spawn = Vec3::new(
        f32::from_le_bytes(word(take(4)?)?),
        f32::from_le_bytes(word(take(4)?)?),
        f32::from_le_bytes(word(take(4)?)?),
    );
    let count = u32::from_le_bytes(word(take(4)?)?) as usize;
    let mut packets = Vec::with_capacity(count);
    for _ in 0..count {
        let len = u32::from_le_bytes(word(take(4)?)?) as usize;
        packets.push(take(len)?);
    }
    Some((spawn, packets))
}
