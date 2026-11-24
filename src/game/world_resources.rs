//! What a joined world draws with, independent of which world.

use crate::world::item_model::HeldUse;
use crate::world::item_model::StackKey;

use super::entity_models::{MobModels, PlayerModels};
use super::held_items::HeldItems;
use crate::assets::Assets;
use crate::gui::hud::HudTextures;
use crate::gui::hud::item_icon::ItemIcons;
use crate::gui::overlay::container::ContainerTextures;
use crate::gui::overlay::creative::CreativeTextures;
use crate::gui::overlay::game_mode_switcher::SwitcherTextures;
use crate::gui::overlay::inventory::InventoryTextures;
use crate::gui::overlay::spectator_menu::SpectatorTextures;
use crate::renderer::{MeshId, Renderer, TextureId};
use crate::world::block_entity;
use crate::world::block_models::{BlockModels, ExtraTexture, mipmap};
use crate::world::clouds::CloudTexture;
use crate::world::colormap::ColorMaps;
use crate::world::fluid::FluidSprites;
use crate::world::head_profile::HeadProfile;
use crate::world::texture_animation::TextureAnimator;
use image::RgbaImage;

pub struct WorldResources {
    /// Every block state's baked model, and the atlas they sample.
    pub models: std::sync::Arc<BlockModels>,
    /// Grass/foliage colour maps, shared with the mesh workers.
    pub colormaps: std::sync::Arc<ColorMaps>,
    pub atlas: TextureId,
    /// The atlas's animated sprites (water, lava, fire...).
    pub animator: TextureAnimator,
    /// Vanilla's `items` atlas, no mipmaps: what held items sample unless
    /// their texture is a block's (see [`crate::world::item_atlas`]).
    pub item_atlas: TextureId,
    pub item_animator: TextureAnimator,
    pub fluids: FluidSprites,
    /// The assets these were loaded from, for what loads on first sight.
    pub root: std::path::PathBuf,
    /// Every particle type's sprites on the block atlas.
    pub particle_sprites: std::sync::Arc<crate::particle::animation::SpriteSets>,
    /// The player models (Steve's wide arms, Alex's slim ones) and the skins.
    pub players: PlayerModels,
    /// The mob models and the mob and armor textures seen so far.
    pub mobs: MobModels,
    pub hud: HudTextures,
    pub effects: crate::gui::effects::EffectTextures,
    pub inventory: InventoryTextures,
    pub creative: CreativeTextures,
    pub switcher: SwitcherTextures,
    pub container: ContainerTextures,
    pub spectator: SpectatorTextures,
    /// The tab list's ping bars.
    pub tab: crate::gui::hud::player_tab::TabTextures,
    pub icons: ItemIcons,
    pub held: HeldItems,
    /// Blocks mobs carry (an enderman's).
    pub blocks: super::block_meshes::BlockMeshes,
    pub sky: SkyResources,
    pub weather: WeatherResources,
    pub glint: GlintTextures,
    pub shadow: TextureId,
    /// `textures/misc/forcefield.png`, the world border's wall, repeating.
    pub forcefield: TextureId,
    /// `destroy_stage_0` to `_9`: the cracks of a block being broken.
    pub destroy_stages: [TextureId; 10],
    /// What [`Self::load`] uploaded, freed or reused by a reload.
    textures: Vec<TextureId>,
    meshes: Vec<MeshId>,
}

/// `environment/celestial/moon/*`, by moon phase.
const MOON_PHASES: [&str; 8] = [
    "full_moon",
    "waning_gibbous",
    "third_quarter",
    "waning_crescent",
    "new_moon",
    "waxing_crescent",
    "first_quarter",
    "waxing_gibbous",
];

/// The sky's textures and meshes, for `InWorld::sky_draws`.
pub struct SkyResources {
    pub sun: TextureId,
    /// By moon phase, the full moon first.
    pub moon: [TextureId; 8],
    pub end_sky: TextureId,
    pub sun_mesh: MeshId,
    pub moon_mesh: MeshId,
    pub star_mesh: MeshId,
    /// The sky's disc, and the dark one below the horizon.
    pub disc_mesh: MeshId,
    pub dark_disc_mesh: MeshId,
    /// The dusk glow's fan.
    pub sunrise_mesh: MeshId,
    pub end_sky_mesh: MeshId,
}

impl SkyResources {
    fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        use crate::world::sky;
        let mut moon = [TextureId(0); 8];
        for (texture, phase) in moon.iter_mut().zip(MOON_PHASES) {
            let id = format!("minecraft:environment/celestial/moon/{phase}.png");
            *texture = assets.texture(renderer, &id)?;
        }
        Ok(Self {
            sun: assets.texture(renderer, "minecraft:environment/celestial/sun.png")?,
            moon,
            end_sky: assets.texture(renderer, "minecraft:environment/end_sky.png")?,
            sun_mesh: renderer.upload_mesh(&sky::sun_quad())?,
            moon_mesh: renderer.upload_mesh(&sky::moon_quad())?,
            star_mesh: renderer.upload_mesh(&sky::star_quads())?,
            disc_mesh: renderer.upload_mesh(&sky::sky_disc(16.0))?,
            dark_disc_mesh: renderer.upload_mesh(&sky::sky_disc(-16.0))?,
            sunrise_mesh: renderer.upload_mesh(&sky::sunrise_fan())?,
            end_sky_mesh: renderer.upload_mesh(&sky::end_sky_box())?,
        })
    }
}

/// Clouds, rain and snow, for `InWorld::atmosphere_draws`.
pub struct WeatherResources {
    /// Packed `clouds.png` cells, CPU-side.
    pub clouds: CloudTexture,
    /// `textures/environment/rain.png`, repeating.
    pub rain: TextureId,
    /// `textures/environment/snow.png`, repeating.
    pub snow: TextureId,
    /// Their drop streaks, for `weather.vert` (`world::weather::template`).
    pub rain_template: MeshId,
    pub snow_template: MeshId,
}

impl WeatherResources {
    fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        let rain = assets.image("minecraft:environment/rain.png")?;
        let snow = assets.image("minecraft:environment/snow.png")?;
        Ok(Self {
            clouds: CloudTexture::load(assets.root()),
            rain: renderer.load_texture_tiled(&rain)?,
            snow: renderer.load_texture_tiled(&snow)?,
            rain_template: upload_template(renderer, &rain)?,
            snow_template: upload_template(renderer, &snow)?,
        })
    }
}

/// `textures/misc/enchanted_glint_{item,armor}.png`.
pub struct GlintTextures {
    pub item: TextureId,
    pub armor: TextureId,
    /// The armor glint's pixels, for the entity previews drawn on the CPU.
    pub armor_image: RgbaImage,
}

impl GlintTextures {
    fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        let item = assets.image("minecraft:misc/enchanted_glint_item.png")?;
        let armor_image = assets.image("minecraft:misc/enchanted_glint_armor.png")?;
        Ok(Self {
            item: renderer.load_texture_linear_tiled(&item)?,
            armor: renderer.load_texture_linear_tiled(&armor_image)?,
            armor_image,
        })
    }
}

impl WorldResources {
    pub fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        renderer.track_textures();
        let result = Self::build(renderer, assets);
        let (textures, meshes) = renderer.take_tracked();
        match result {
            Ok(mut loaded) => {
                loaded.textures = textures;
                loaded.meshes = meshes;
                renderer.bind_gui_glint(loaded.glint.item);
                renderer.compact_textures();
                Ok(loaded)
            }
            Err(e) => {
                textures.into_iter().for_each(|t| renderer.free_texture(t));
                meshes.into_iter().for_each(|m| renderer.free_mesh(m));
                Err(e)
            }
        }
    }

    /// Loads everything again from the current resource packs.
    pub fn reload(
        mut self,
        renderer: &mut Renderer,
        assets: &Assets,
    ) -> Result<Self, Box<(Self, anyhow::Error)>> {
        renderer.begin_texture_reuse(self.textures.clone());
        renderer.track_textures();
        let result = Self::build(renderer, assets);
        let (textures, meshes) = renderer.take_tracked();
        match result {
            Ok(mut loaded) => {
                renderer.end_texture_reuse(true);
                loaded.textures = textures;
                loaded.meshes = meshes;
                loaded.players.adopt_fetched(&mut self.players);
                self.textures.clear();
                self.release(renderer);
                renderer.bind_gui_glint(loaded.glint.item);
                Ok(loaded)
            }
            Err(e) => {
                renderer.end_texture_reuse(false);
                meshes.into_iter().for_each(|m| renderer.free_mesh(m));
                Err(Box::new((self, e)))
            }
        }
    }

    /// Frees every texture and mesh, then lets the renderer merge the
    /// texture descriptor pools that overflowed meanwhile (many custom
    /// held items) back into one. A reload ends here too, with the new
    /// textures already in place.
    pub fn release(mut self, renderer: &mut Renderer) {
        self.icons.release(renderer);
        self.held.release(renderer);
        self.blocks.release(renderer);
        self.players.release_fetched(renderer);
        self.mobs.release(renderer);
        for texture in self.textures.drain(..) {
            renderer.free_texture(texture);
        }
        for mesh in self.meshes.drain(..) {
            renderer.free_mesh(mesh);
        }
        renderer.compact_textures();
    }

    fn build(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        let players = PlayerModels::load(renderer, assets)?;
        let mobs = MobModels::load(renderer, assets)?;
        let mut extras: Vec<ExtraTexture> = FluidSprites::TEXTURES
            .iter()
            .map(|id| ExtraTexture {
                id: (*id).to_owned(),
                image: None,
            })
            .collect();
        extras.extend(block_entity::atlas_textures(assets.root()));
        extras.extend(
            crate::particle::animation::SpriteSets::texture_ids(assets.root())
                .into_iter()
                .map(|id| ExtraTexture { id, image: None }),
        );
        let (models, atlas) = BlockModels::bake(assets.root(), extras, renderer.max_texture_side());
        renderer.set_quad_table(models.quad_corners())?;
        let sprite = |id: &str| models.sprite(id).unwrap_or([0.0; 4]);
        let fluids = FluidSprites {
            water_still: sprite(FluidSprites::TEXTURES[0]),
            water_flow: sprite(FluidSprites::TEXTURES[1]),
            water_overlay: sprite(FluidSprites::TEXTURES[2]),
            lava_still: sprite(FluidSprites::TEXTURES[3]),
            lava_flow: sprite(FluidSprites::TEXTURES[4]),
        };
        let particle_sprites = crate::particle::animation::SpriteSets::load(
            assets.root(),
            |id| models.sprite(id),
            sprite("minecraft:particle/generic_0"),
        );
        let models = std::sync::Arc::new(models);
        let block_atlas = renderer.load_texture_mipmapped(&atlas.image, &atlas.mips)?;
        let items = crate::world::block_models::pack_sprites(
            assets.root(),
            crate::world::item_atlas::texture_ids(assets.root()),
            renderer.max_texture_side(),
        );
        let item_atlas = renderer.load_texture(&items.image)?;
        let item_sprites = std::sync::Arc::new(items.sprites);
        let item_sheet = crate::gui::hud::item_icon::ItemSheet {
            texture: item_atlas,
            sprites: item_sprites.clone(),
        };
        let sheets = super::held_items::Sheets {
            items: item_atlas,
            item_sprites,
            blocks: block_atlas,
            models: models.clone(),
            item_side: items.image.width(),
            block_side: atlas.image.width(),
        };
        Ok(Self {
            fluids,
            particle_sprites: std::sync::Arc::new(particle_sprites),
            root: assets.root().to_path_buf(),
            animator: atlas.animator,
            atlas: block_atlas,
            item_atlas,
            item_animator: items.animator,
            models,
            colormaps: std::sync::Arc::new(ColorMaps::load(assets.root())),
            players,
            mobs,
            hud: HudTextures::load(renderer, assets)?,
            effects: crate::gui::effects::EffectTextures::load(renderer, assets)?,
            inventory: InventoryTextures::load(renderer, assets)?,
            creative: CreativeTextures::load(renderer, assets)?,
            switcher: SwitcherTextures::load(renderer, assets)?,
            container: ContainerTextures::load(renderer, assets)?,
            spectator: SpectatorTextures::load(renderer, assets)?,
            tab: crate::gui::hud::player_tab::TabTextures::load(renderer, assets)?,
            icons: ItemIcons::new(assets.root(), item_sheet, renderer.max_texture_side()),
            held: HeldItems::new(assets.root(), sheets),
            blocks: Default::default(),
            sky: SkyResources::load(renderer, assets)?,
            weather: WeatherResources::load(renderer, assets)?,
            glint: GlintTextures::load(renderer, assets)?,
            shadow: assets.texture(renderer, "minecraft:misc/shadow.png")?,
            forcefield: renderer
                .load_texture_tiled(&assets.image("minecraft:misc/forcefield.png")?)?,
            destroy_stages: {
                let mut stages = [TextureId(0); 10];
                for (i, stage) in stages.iter_mut().enumerate() {
                    let id = format!("minecraft:block/destroy_stage_{i}.png");
                    *stage = assets.texture(renderer, &id)?;
                }
                stages
            },
            textures: Vec::new(),
            meshes: Vec::new(),
        })
    }

    /// The nether portal's sprite in the block atlas.
    pub fn nether_portal_sprite(&self) -> Option<crate::gui::effects::AtlasSprite> {
        self.models
            .sprite("minecraft:block/nether_portal")
            .map(|uv| crate::gui::effects::AtlasSprite {
                atlas: self.atlas,
                uv,
            })
    }

    /// Runs `ticks` client ticks of the atlas animations and uploads the changed frames.
    pub fn animate(&mut self, renderer: &mut Renderer, ticks: u32) {
        let mut latest = std::collections::HashMap::new();
        for _ in 0..ticks {
            for (x, y, image) in self.animator.tick() {
                latest.insert((x, y), image);
            }
        }
        for ((x, y), mut image) in latest {
            let holes = image.pixels().any(|p| p.0[3] == 0);
            if holes {
                mipmap::solidify(&mut image);
            }
            for (level, mip) in (1..).zip(mipmap::chain(&image, holes)) {
                renderer.update_texture(self.atlas, level, x >> level, y >> level, mip);
            }
            renderer.update_texture(self.atlas, 0, x, y, image);
        }
        let mut latest = std::collections::HashMap::new();
        for _ in 0..ticks {
            for (x, y, image) in self.item_animator.tick() {
                latest.insert((x, y), image);
            }
        }
        for ((x, y), image) in latest {
            renderer.update_texture(self.item_atlas, 0, x, y, image);
        }
    }

    /// Makes sure `stack` can be drawn in hand.
    pub fn prepare_held(
        &mut self,
        renderer: &mut Renderer,
        stack: &impl StackKey,
        using: impl Into<HeldUse>,
    ) {
        let using = using.into();
        self.held.prepare(renderer, stack, using);
        if !stack.has_head() {
            return;
        }
        let stack = stack.stack();
        if let Some(profile) = HeadProfile::of_stack(stack) {
            self.players.request_head(&profile);
            let own = self.players.has_head_skin(&profile);
            let skin = self.players.for_head(Some(&profile)).image;
            self.held.prepare_head(
                renderer,
                stack,
                &profile,
                (skin, own),
                using == HeldUse::Using,
            );
        }
    }

    /// Bakes the icons of the stacks about to be drawn.
    pub fn prepare_icons(
        &mut self,
        renderer: &mut Renderer,
        stacks: impl FnOnce(&mut dyn FnMut(&dyn StackKey) -> bool),
    ) {
        let mut heads = Vec::new();
        self.icons.prepare_batch(renderer, |visit| {
            stacks(&mut |stack| {
                visit(stack);
                if stack.has_head() {
                    heads.push(stack.stack().clone());
                }
                true
            });
        });
        for stack in heads {
            let Some(profile) = HeadProfile::of_stack(&stack) else {
                continue;
            };
            self.players.request_head(&profile);
            let own = self.players.has_head_skin(&profile);
            let skin = self.players.for_head(Some(&profile)).image;
            self.icons
                .prepare_head(renderer, &stack, &profile, skin, own);
        }
    }
}

/// A weather texture's drop streaks as the template `weather.vert` places.
fn upload_template(renderer: &mut Renderer, image: &image::RgbaImage) -> anyhow::Result<MeshId> {
    use crate::world::weather;
    let (width, height) = image.dimensions();
    renderer.upload_mesh(&weather::template(&weather::streaks(image), width, height))
}
