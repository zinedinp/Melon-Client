//! Entity models on the GPU: each part of a baked

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};

use glam::Mat4;
use image::RgbaImage;

use crate::assets::Assets;
use crate::auth::skin::DownloadedSkin;
use crate::renderer::{EntityDraw, Glint, MeshId, RenderLayer, Renderer, TextureId};
use crate::world::block_entity::models::SkullType;
use crate::world::entity::mobs::villager::Hat;
use crate::world::entity::mobs::{MobModel, ModelId};
use crate::world::entity::model::Model;
use crate::world::entity::player::{
    DEFAULT_SKINS, PlayerModel, PlayerModelType, default_skin, default_skin_texture,
};
use crate::world::entity::skull::SkullModel;
use crate::world::head_profile::HeadProfile;

/// A model's parts on the GPU, by part index; `None` for parts without
pub struct ModelMeshes {
    parts: Vec<Option<MeshId>>,
}

impl ModelMeshes {
    pub fn upload(renderer: &mut Renderer, model: &Model) -> anyhow::Result<Self> {
        let parts = (0..model.len())
            .map(|i| {
                let vertices = model.part_vertices(crate::world::entity::model::PartId(i));
                if vertices.is_empty() {
                    Ok(None)
                } else {
                    renderer.upload_mesh(&vertices).map(Some)
                }
            })
            .collect::<anyhow::Result<_>>()?;
        Ok(Self { parts })
    }

    /// A draw for every part with boxes and a matrix; the matrices are
    pub fn draws(
        &self,
        matrices: &[Option<Mat4>],
        origin: glam::IVec3,
        style: DrawStyle,
        out: &mut Vec<EntityDraw>,
    ) {
        self.draws_filtered(matrices, origin, style, false, None, out);
    }

    /// Overlay parts of a player skin (`entityTranslucent`): drawn after
    pub fn draws_overlay(
        &self,
        matrices: &[Option<Mat4>],
        origin: glam::IVec3,
        style: DrawStyle,
        overlay: &[crate::world::entity::model::PartId],
        out: &mut Vec<EntityDraw>,
    ) {
        self.draws_filtered(matrices, origin, style, true, Some(overlay), out);
    }

    fn draws_filtered(
        &self,
        matrices: &[Option<Mat4>],
        origin: glam::IVec3,
        style: DrawStyle,
        translucent: bool,
        only: Option<&[crate::world::entity::model::PartId]>,
        out: &mut Vec<EntityDraw>,
    ) {
        out.extend(
            self.parts
                .iter()
                .enumerate()
                .zip(matrices)
                .filter(|((index, _), _)| {
                    only.is_none_or(|parts| parts.iter().any(|p| p.0 == *index))
                })
                .filter_map(|((_, mesh), matrix)| Some(((*mesh)?, (*matrix)?)))
                .map(|(mesh, model)| EntityDraw {
                    scroll: None,
                    alpha: 1.0,
                    mesh,
                    atlas: style.texture,
                    model,
                    origin,
                    layer: style.layer,
                    blend: crate::renderer::Blending::of(translucent),
                    light: style.light,
                    overlay: style.overlay,
                    glint: style.glint,
                }),
        );
    }

    /// Inner body: every part except `skip` (the skin's outer layer).
    pub fn draws_inner(
        &self,
        matrices: &[Option<Mat4>],
        origin: glam::IVec3,
        style: DrawStyle,
        skip: &[crate::world::entity::model::PartId],
        out: &mut Vec<EntityDraw>,
    ) {
        out.extend(
            self.parts
                .iter()
                .enumerate()
                .zip(matrices)
                .filter(|((index, _), _)| skip.iter().all(|p| p.0 != *index))
                .filter_map(|((_, mesh), matrix)| Some(((*mesh)?, (*matrix)?)))
                .map(|(mesh, model)| EntityDraw {
                    scroll: None,
                    alpha: 1.0,
                    mesh,
                    atlas: style.texture,
                    model,
                    origin,
                    layer: style.layer,
                    blend: crate::renderer::Blending::Opaque,
                    light: style.light,
                    overlay: style.overlay,
                    glint: style.glint,
                }),
        );
    }
}

/// How a model's parts are drawn: texture, layer, light and overlay.
#[derive(Clone, Copy, Debug)]
pub struct DrawStyle {
    pub texture: TextureId,
    pub layer: RenderLayer,
    /// `None`: the scene's ambient light.
    pub light: Option<[f32; 3]>,
    pub overlay: f32,
    pub glint: Glint,
}

impl DrawStyle {
    pub fn new(texture: TextureId, layer: RenderLayer) -> Self {
        Self {
            texture,
            layer,
            light: None,
            overlay: 0.0,
            glint: Glint::None,
        }
    }
}

/// A player model and its meshes.
pub struct PlayerModelMeshes {
    pub model: PlayerModel,
    pub meshes: ModelMeshes,
}

impl PlayerModelMeshes {
    fn load(renderer: &mut Renderer, kind: PlayerModelType) -> anyhow::Result<Self> {
        let model = PlayerModel::new(kind);
        let meshes = ModelMeshes::upload(renderer, &model.model)?;
        Ok(Self { model, meshes })
    }
}

/// Both player models, the eighteen default skins, and the skins fetched
pub struct PlayerModels {
    pub wide: PlayerModelMeshes,
    pub slim: PlayerModelMeshes,
    /// A player head's model (`SkullModel.createHumanoidHeadLayer`).
    pub head: HeadMeshes,
    /// [`DEFAULT_SKINS`]' textures and images, in its order.
    skins: Vec<(TextureId, RgbaImage)>,
    /// Skins of profiles asked for: `None` while on the way or when the
    fetched: HashMap<uuid::Uuid, Option<FetchedSkin>>,
    sender: Sender<(uuid::Uuid, Option<DownloadedSkin>)>,
    arrived: Receiver<(uuid::Uuid, Option<DownloadedSkin>)>,
}

struct FetchedSkin {
    texture: TextureId,
    image: RgbaImage,
    model: PlayerModelType,
}

/// What a player is drawn with: the skin (on the GPU and as an image, for
#[derive(Clone, Copy)]
pub struct PlayerLook<'a> {
    pub texture: TextureId,
    pub image: &'a RgbaImage,
    pub model: &'a PlayerModelMeshes,
}

impl PlayerModels {
    pub fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        let skins = (0..DEFAULT_SKINS.len())
            .map(|i| {
                let image = assets.image(&default_skin_texture(i))?;
                Ok((renderer.load_texture(&image)?, image))
            })
            .collect::<anyhow::Result<_>>()?;
        let (sender, arrived) = mpsc::channel();
        Ok(Self {
            wide: PlayerModelMeshes::load(renderer, PlayerModelType::Wide)?,
            slim: PlayerModelMeshes::load(renderer, PlayerModelType::Slim)?,
            head: HeadMeshes::load(renderer)?,
            skins,
            fetched: HashMap::new(),
            sender,
            arrived,
        })
    }

    /// Keeps the skins `old` fetched, so a reload doesn't ask Mojang again.
    pub fn adopt_fetched(&mut self, old: &mut PlayerModels) {
        self.fetched = std::mem::take(&mut old.fetched);
        self.fetched.retain(|_, skin| skin.is_some());
    }

    /// Frees the fetched skins (the default ones are the caller's).
    pub fn release_fetched(&mut self, renderer: &mut Renderer) {
        for skin in self.fetched.drain().filter_map(|(_, s)| s) {
            renderer.free_texture(skin.texture);
        }
    }

    fn model(&self, kind: PlayerModelType) -> &PlayerModelMeshes {
        match kind {
            PlayerModelType::Wide => &self.wide,
            PlayerModelType::Slim => &self.slim,
        }
    }

    /// Fetches `uuid`'s own skin in the background, once: the one the
    pub fn request(&mut self, uuid: uuid::Uuid, textures: Option<&str>) {
        if (textures.is_none() && uuid.get_version_num() != 4) || self.fetched.contains_key(&uuid) {
            return;
        }
        self.fetched.insert(uuid, None);
        let sender = self.sender.clone();
        let textures = textures.map(str::to_owned);
        std::thread::spawn(move || {
            let skin = match textures {
                Some(textures) => crate::auth::skin::skin_from_textures(&textures),
                None => crate::auth::skin::fetch_skin(&uuid.simple().to_string()),
            };
            let skin = skin
                .map_err(|e| crate::warn!("Skin of {uuid} not loaded: {e:#}"))
                .ok();
            let _ = sender.send((uuid, skin));
        });
    }

    /// Uploads the skins that arrived since the last call.
    pub fn poll(&mut self, renderer: &mut Renderer) {
        while let Ok((uuid, skin)) = self.arrived.try_recv() {
            let Some(skin) = skin else {
                continue;
            };
            match renderer.load_texture(&skin.image) {
                Ok(texture) => {
                    let model = if skin.slim {
                        PlayerModelType::Slim
                    } else {
                        PlayerModelType::Wide
                    };
                    self.fetched.insert(
                        uuid,
                        Some(FetchedSkin {
                            texture,
                            image: skin.image,
                            model,
                        }),
                    );
                }
                Err(e) => crate::warn!("Skin of {uuid} not uploaded: {e:#}"),
            }
        }
    }

    /// Fetches the skin a player head's profile names, once: by its
    pub fn request_head(&mut self, profile: &HeadProfile) {
        let key = profile.skin_key();
        match (&profile.textures, profile.id, &profile.name) {
            (Some(textures), ..) => self.request(key, Some(textures)),
            (None, Some(id), _) => self.request(id, None),
            (None, None, Some(name)) => {
                if self.fetched.contains_key(&key) {
                    return;
                }
                self.fetched.insert(key, None);
                let sender = self.sender.clone();
                let name = name.to_string();
                std::thread::spawn(move || {
                    let skin = crate::auth::skin::fetch_skin_by_name(&name)
                        .map_err(|e| crate::warn!("Skin of {name} not loaded: {e:#}"))
                        .ok();
                    let _ = sender.send((key, skin));
                });
            }
            (None, None, None) => {}
        }
    }

    /// The skin a player head shows: its profile's once fetched, else the
    pub fn for_head(&self, profile: Option<&HeadProfile>) -> PlayerLook<'_> {
        let Some(profile) = profile else {
            return self.default_look(STEVE);
        };
        let key = profile.skin_key();
        if let Some(Some(_)) = self.fetched.get(&key) {
            return self.for_profile(key);
        }
        match profile.id {
            Some(id) => self.for_profile(id),
            None => self.default_look(STEVE),
        }
    }

    /// Whether `profile`'s own skin has arrived.
    pub fn has_head_skin(&self, profile: &HeadProfile) -> bool {
        matches!(self.fetched.get(&profile.skin_key()), Some(Some(_)))
    }

    fn default_look(&self, index: usize) -> PlayerLook<'_> {
        let (texture, image) = &self.skins[index];
        PlayerLook {
            texture: *texture,
            image,
            model: self.model(DEFAULT_SKINS[index].1),
        }
    }

    /// The profile's own skin once fetched, else the default skin it gets
    pub fn for_profile(&self, uuid: uuid::Uuid) -> PlayerLook<'_> {
        if let Some(Some(skin)) = self.fetched.get(&uuid) {
            return PlayerLook {
                texture: skin.texture,
                image: &skin.image,
                model: self.model(skin.model),
            };
        }
        self.default_look(default_skin(uuid))
    }
}

impl PlayerModels {
    /// `ClientMannequin.getSkin`: its profile's skin once fetched, else the
    /// default one by the profile's id, with the patch's body and arms.
    pub fn for_avatar(&self, avatar: &crate::world::head_profile::AvatarProfile) -> PlayerLook<'_> {
        let mut look = match &avatar.profile {
            Some(profile) if self.has_head_skin(profile) => self.for_profile(profile.skin_key()),
            _ => self.default_look(default_skin(avatar.default_skin_id())),
        };
        // `PlayerSkin.Patch.body`: a default skin's texture is all this
        // draws; another asset keeps the profile's.
        if let Some(body) = avatar.patch.body.as_deref() {
            let path = body.strip_prefix("minecraft:").unwrap_or(body);
            if let Some(index) = (0..DEFAULT_SKINS.len()).find(|&i| {
                let (name, model) = DEFAULT_SKINS[i];
                path == format!("entity/player/{}/{name}", model.folder())
            }) {
                let (texture, image) = &self.skins[index];
                look.texture = *texture;
                look.image = image;
            }
        }
        if let Some(slim) = avatar.patch.slim {
            look.model = self.model(if slim {
                PlayerModelType::Slim
            } else {
                PlayerModelType::Wide
            });
        }
        look
    }
}

/// Steve with wide arms, in [`DEFAULT_SKINS`].
const STEVE: usize = 15;

/// Every kind of head on the GPU ([`SkullModel`]), in model pixels around
pub struct HeadMeshes {
    /// By [`SkullType::ALL`].
    skulls: Vec<(SkullModel, ModelMeshes)>,
}

impl HeadMeshes {
    fn load(renderer: &mut Renderer) -> anyhow::Result<Self> {
        let skulls = SkullType::ALL
            .into_iter()
            .map(|kind| {
                let skull = SkullModel::new(kind);
                let meshes = ModelMeshes::upload(renderer, skull.model())?;
                Ok((skull, meshes))
            })
            .collect::<anyhow::Result<_>>()?;
        Ok(Self { skulls })
    }

    /// The model of head `kind`, for a picture drawn on the CPU.
    pub fn model(&self, kind: SkullType) -> &SkullModel {
        &self.get(kind).0
    }

    fn get(&self, kind: SkullType) -> &(SkullModel, ModelMeshes) {
        let index = SkullType::ALL.iter().position(|&k| k == kind).unwrap_or(0);
        &self.skulls[index]
    }

    /// Head `kind` at `animation` ([`crate::world::entity::skull::joints`]), placed by `at`
    pub fn draws(
        &self,
        kind: SkullType,
        animation: f32,
        at: Mat4,
        origin: glam::IVec3,
        style: DrawStyle,
        out: &mut Vec<EntityDraw>,
    ) {
        let (skull, meshes) = self.get(kind);
        let matrices = skull.matrices(animation, at);
        match skull.overlay() {
            Some(hat) => {
                let hat = [hat];
                meshes.draws_inner(&matrices, origin, style, &hat, out);
                meshes.draws_overlay(&matrices, origin, style, &hat, out);
            }
            None => meshes.draws(&matrices, origin, style, out),
        }
    }
}

/// Every mob model ([`ModelId::ALL`]) baked and uploaded, and the mob and
pub struct MobModels {
    assets_root: std::path::PathBuf,
    models: Vec<(MobModel, ModelMeshes)>,
    /// By asset id; `None` for one that isn't there.
    textures: HashMap<String, Option<TextureId>>,
    /// The `villager.hat` of the textures that say one.
    hats: HashMap<String, Hat>,
}

impl MobModels {
    pub fn load(renderer: &mut Renderer, assets: &Assets) -> anyhow::Result<Self> {
        let models = ModelId::ALL
            .into_iter()
            .map(|id| {
                let mob = MobModel::new(id);
                let meshes = ModelMeshes::upload(renderer, &mob.model)?;
                Ok((mob, meshes))
            })
            .collect::<anyhow::Result<_>>()?;
        Ok(Self {
            assets_root: assets.root().to_path_buf(),
            models,
            textures: HashMap::new(),
            hats: HashMap::new(),
        })
    }

    pub fn get(&self, id: ModelId) -> (&MobModel, &ModelMeshes) {
        let (mob, meshes) = &self.models[id.index()];
        (mob, meshes)
    }

    /// Loads the texture `id` (an asset id without `.png`) if it hasn't
    pub fn prepare_texture(&mut self, renderer: &mut Renderer, id: &str) {
        if self.textures.contains_key(id) {
            return;
        }
        let found = crate::assets::texture_with_meta(&self.assets_root, id);
        if let Some(hat) = found
            .as_ref()
            .and_then(|(_, meta)| meta.as_ref())
            .map(Hat::from_meta)
            && hat != Hat::None
        {
            self.hats.insert(id.to_owned(), hat);
        }
        let image = found
            .and_then(|(bytes, _)| crate::assets::decode_png(&bytes))
            .or_else(|| crate::assets::palette::resolve(&self.assets_root, id));
        let texture = image.and_then(|image| {
            renderer
                .load_texture(&image)
                .map_err(|e| crate::warn!("Texture {id} not uploaded: {e:#}"))
                .ok()
        });
        self.textures.insert(id.to_owned(), texture);
    }

    /// The texture `id`, decoded, for a picture drawn on the CPU.
    pub fn image(&self, id: &str) -> Option<RgbaImage> {
        crate::assets::texture_image(&self.assets_root, id)
    }

    /// How much of the hat the clothes texture `id` covers.
    pub fn hat(&self, id: &str) -> Hat {
        self.hats.get(id).copied().unwrap_or_default()
    }

    /// The texture `id`, once [`Self::prepare_texture`] loaded it.
    pub fn texture(&self, id: &str) -> Option<TextureId> {
        self.textures.get(id).copied().flatten()
    }

    /// Frees the textures loaded so far (the meshes are the caller's).
    pub fn release(&mut self, renderer: &mut Renderer) {
        self.hats.clear();
        for texture in self.textures.drain().filter_map(|(_, t)| t) {
            renderer.free_texture(texture);
        }
    }
}
