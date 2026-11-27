//! Textures, cube maps and meshes: uploading, updating and freeing them.

use super::{
    CubeMapId, IndexMeshId, MeshId, ParticleMeshId, QuadMeshId, Renderer, texture, texture_update,
};
use ash::vk;
use image::RgbaImage;
use melon_base::gfx::TextureId;
use melon_world::mesh::TerrainQuad;
use melon_world::mesh::WorldVertex;
use melon_world::particle::mesh::ParticleQuad;

/// Bytes of new textures queued for the next frame before they are
/// uploaded at once instead.
const UPLOAD_BUDGET: u64 = 64 << 20;

impl Renderer {
    pub fn white_texture(&self) -> TextureId {
        self.textures.white
    }

    /// The texture `identifier` (`namespace:path` under `textures/`) from the active packs, uploaded.
    pub fn asset_texture(
        &mut self,
        assets: &melon_assets::assets::Assets,
        identifier: &str,
    ) -> anyhow::Result<TextureId> {
        self.load_texture(&assets.image(identifier)?)
    }

    pub fn load_texture(&mut self, rgba: &RgbaImage) -> anyhow::Result<TextureId> {
        // SAFETY: the uploader records into this frame's init commands; the new image isn't in use yet.
        let made = unsafe {
            self.uploader(self.textures.nearest_sampler)
                .image_2d(rgba)?
        };
        Ok(self.put_new(made))
    }

    /// A texture sampled linearly (`blur: true` in its `.mcmeta`).
    pub fn load_texture_linear(&mut self, rgba: &RgbaImage) -> anyhow::Result<TextureId> {
        // SAFETY: the uploader records into this frame's init commands; the new image isn't in use yet.
        let made = unsafe { self.uploader(self.textures.linear_sampler).image_2d(rgba)? };
        Ok(self.put_new(made))
    }

    /// A texture that repeats outside `0..1` (vanilla's `GL_REPEAT`).
    pub fn load_texture_tiled(&mut self, rgba: &RgbaImage) -> anyhow::Result<TextureId> {
        // SAFETY: the uploader records into this frame's init commands; the new image isn't in use yet.
        let made = unsafe { self.uploader(self.textures.repeat_sampler).image_2d(rgba)? };
        Ok(self.put_new(made))
    }

    /// Linear and wrapping, for the enchanted glint (blurred and repeating).
    pub fn load_texture_linear_tiled(&mut self, rgba: &RgbaImage) -> anyhow::Result<TextureId> {
        // SAFETY: the uploader records into this frame's init commands; the new image isn't in use yet.
        let made = unsafe {
            self.uploader(self.textures.linear_repeat_sampler)
                .image_2d(rgba)?
        };
        Ok(self.put_new(made))
    }

    /// The item-glint sampler the GUI shaders read at set 1 binding 3.
    pub fn bind_gui_glint(&mut self, texture: TextureId) {
        let Some(tex) = self.textures.list.get(texture.0) else {
            return;
        };
        // SAFETY: the glint set is rewritten before this frame is recorded; earlier frames keep theirs.
        unsafe {
            self.gui_uniforms.bind_glint(
                &self.device,
                tex.view,
                self.textures.linear_repeat_sampler,
            );
        }
    }

    /// A texture with its smaller levels (`mips`, each half the size of the last).
    pub fn load_texture_mipmapped(
        &mut self,
        rgba: &RgbaImage,
        mips: &[RgbaImage],
    ) -> anyhow::Result<TextureId> {
        // SAFETY: the uploader records into this frame's init commands; the new image isn't in use yet.
        let made = unsafe {
            self.uploader(self.textures.mipmap_sampler)
                .image_2d_mipmapped(rgba, mips)?
        };
        Ok(self.put_new(made))
    }

    /// Rewrites part of mip `level` of a texture with `pixels`, top-left at `(x, y)`.
    pub fn update_texture(
        &mut self,
        texture: TextureId,
        level: u32,
        x: u32,
        y: u32,
        pixels: RgbaImage,
    ) {
        if let Some(target) = self.textures.list.get(texture.0)
            && level < target.mip_levels
            && !target.shared
        {
            self.textures
                .pending_updates
                .push(texture_update::PendingUpdate {
                    image: target.image,
                    level,
                    x,
                    y,
                    pixels,
                });
        }
    }

    /// Queues `made`'s pixels and stores its texture.
    fn put_new(
        &mut self,
        (tex, init): (texture::Texture, texture_update::PendingInit),
    ) -> TextureId {
        self.queue_init(init);
        self.put_texture(tex)
    }

    /// Queues a new texture's pixels for the next frame. Past
    /// [`UPLOAD_BUDGET`] (a loading burst: the block atlas) everything
    /// queued goes up at once, waiting for that one submit, so the frame's
    /// staging buffer stays small.
    fn queue_init(&mut self, init: texture_update::PendingInit) {
        self.textures.pending_init_bytes += init.bytes();
        self.textures.pending_inits.push(init);
        if self.textures.pending_init_bytes <= UPLOAD_BUDGET {
            return;
        }
        let inits = std::mem::take(&mut self.textures.pending_inits);
        self.textures.pending_init_bytes = 0;
        // SAFETY: `submit_now` waits on its own fence, so the copies are done when it returns.
        let result = unsafe {
            self.layer.queue.with_raw(|queue| {
                texture_update::submit_now(
                    &self.device,
                    &self.mem_props,
                    self.frames.command_pool,
                    queue,
                    &inits,
                )
            })
        };
        if let Err(e) = result {
            melon_base::warn!("Texture upload failed: {e:#}");
        }
    }

    /// Stores a new texture.
    pub(super) fn put_texture(&mut self, tex: texture::Texture) -> TextureId {
        let id = if let Some(id) = self.textures.reuse.pop_front() {
            self.retire(id);
            id
        } else if let Some(id) = self.textures.free.pop() {
            id
        } else {
            self.textures.list.push(tex);
            TextureId(self.textures.list.len() - 1)
        };
        self.textures.list[id.0] = tex;
        if let Some(tracked) = &mut self.textures.tracked {
            tracked.push(id);
        }
        id
    }

    /// Puts the white texture in slot `id` and destroys what was there once no frame uses it.
    fn retire(&mut self, id: TextureId) {
        self.retire_picture(id);
        let mut white = self.textures.list[self.textures.white.0];
        white.shared = true;
        let old = std::mem::replace(&mut self.textures.list[id.0], white);
        if !old.shared {
            self.textures.retired.push((self.frames.number, old));
        }
    }

    /// Frees a texture. Its id draws white until an upload takes the slot.
    pub fn free_texture(&mut self, id: TextureId) {
        if id == self.textures.white
            || self.textures.list.get(id.0).is_none_or(|t| t.shared)
            || self.textures.free.contains(&id)
        {
            return;
        }
        self.retire(id);
        self.textures.free.push(id);
    }

    /// The next uploads go into these slots, in order, replacing what they held.
    pub fn begin_texture_reuse(&mut self, ids: Vec<TextureId>) {
        self.textures.reuse = ids.into();
    }

    /// Stops reusing slots.
    pub fn end_texture_reuse(&mut self, free: bool) -> Vec<TextureId> {
        let left: Vec<TextureId> = std::mem::take(&mut self.textures.reuse).into();
        if free {
            for &id in &left {
                self.free_texture(id);
            }
            return Vec::new();
        }
        left
    }

    /// Records the ids of the uploads from now on, until [`Self::take_tracked`].
    pub fn track_textures(&mut self) {
        self.textures.tracked = Some(Vec::new());
        self.tracked_meshes = Some(Vec::new());
    }

    /// The textures and meshes uploaded since [`Self::track_textures`].
    pub fn take_tracked(&mut self) -> (Vec<TextureId>, Vec<MeshId>) {
        (
            self.textures.tracked.take().unwrap_or_default(),
            self.tracked_meshes.take().unwrap_or_default(),
        )
    }

    /// Frees a cube map. Its id must not be drawn again.
    pub fn free_cubemap(&mut self, id: CubeMapId) {
        if let Some(tex) = self.textures.cubemaps.get_mut(id.0)
            && !tex.shared
        {
            let old = *tex;
            tex.shared = true;
            self.textures.retired.push((self.frames.number, old));
        }
    }

    /// Destroys the retired textures no frame in flight can still use.
    pub(super) fn destroy_retired_textures(&mut self) {
        self.destroy_retired(false);
        self.destroy_retired_pictures(false);
    }

    /// With `all`, also the ones retired in the last frames: only once the
    /// device is idle.
    fn destroy_retired(&mut self, all: bool) {
        let oldest_in_flight = self.frames.oldest_in_flight();
        let device = &self.device;
        let pools = &self.textures.descriptor_pools;
        let (inits, updates) = (
            &mut self.textures.pending_inits,
            &mut self.textures.pending_updates,
        );
        self.textures.retired.retain(|(frame, tex)| {
            if !all && *frame >= oldest_in_flight {
                return true;
            }
            // Frames that end before recording (an out-of-date swapchain)
            // still count, so a copy into it may still be queued.
            inits.retain(|init| init.image != tex.image);
            updates.retain(|update| update.image != tex.image);
            // SAFETY: its frame is past `oldest_in_flight`, and its queued copies were dropped just above.
            unsafe {
                pools.free(device, tex.descriptor_set);
                device.destroy_image_view(tex.view, None);
                device.destroy_image(tex.image, None);
                melon_base::gfx::suballoc::release(device, tex.memory);
            }
            false
        });
    }

    /// Moves all texture descriptor sets into one pool again once they
    /// overflowed into more (a large pack, many custom items), or shrinks a
    /// pool far too big for them. Waits for the GPU, so only call it where a
    /// stall is expected: after a resource reload, on leaving a world.
    pub fn compact_textures(&mut self) {
        if !self.textures.descriptor_pools.needs_compaction() {
            return;
        }
        // SAFETY: the device is waited idle first, so nothing in flight uses what is replaced.
        unsafe {
            if let Err(e) = self.device.device_wait_idle() {
                melon_base::warn!("Waiting for the GPU failed: {e}");
                return;
            }
            self.destroy_retired(true);
            self.destroy_retired_pictures(true);
            let super::Textures {
                list,
                cubemaps,
                descriptor_pools,
                ..
            } = &mut self.textures;
            let (post, blur) = (&mut self.post, &mut self.blur);
            let result = descriptor_pools.compact(&self.device, self.pipelines.layout, |swap| {
                for tex in list.iter_mut().chain(cubemaps.iter_mut()) {
                    swap(&mut tex.descriptor_set);
                }
                for tex in post.textures_mut() {
                    swap(&mut tex.descriptor_set);
                }
                for set in blur.sets_mut() {
                    swap(set);
                }
            });
            if let Err(e) = result {
                melon_base::warn!("Texture descriptors not compacted: {e:#}");
            }
        }
    }

    /// Six equal-size faces in Vulkan order (+X, -X, +Y, -Y, +Z, -Z).
    pub fn load_cubemap(&mut self, faces: &[RgbaImage; 6]) -> anyhow::Result<CubeMapId> {
        // SAFETY: the uploader records into this frame's init commands; the new image isn't in use yet.
        let (tex, init) = unsafe { self.uploader(self.textures.linear_sampler).cubemap(faces)? };
        self.queue_init(init);
        self.textures.cubemaps.push(tex);
        Ok(CubeMapId(self.textures.cubemaps.len() - 1))
    }

    /// Uploads a mesh (a list of quads, see [`WorldVertex`]).
    pub fn upload_mesh(&mut self, vertices: &[WorldVertex]) -> anyhow::Result<MeshId> {
        // SAFETY: the uploader records into this frame's init commands; the new image isn't in use yet.
        let id = unsafe {
            self.meshes
                .upload(&self.device, &self.mem_props, &mut self.residency, vertices)?
        };
        if let Some(tracked) = &mut self.tracked_meshes {
            tracked.push(MeshId(id));
        }
        Ok(MeshId(id))
    }

    /// Frees a mesh from [`upload_mesh`](Self::upload_mesh).
    pub fn free_mesh(&mut self, mesh: MeshId) {
        self.meshes.free(mesh.0, self.frames.number);
    }

    /// Uploads a chunk's block faces; drawn like [`upload_mesh`](Self::upload_mesh)'s.
    pub fn upload_quads(&mut self, quads: &[TerrainQuad]) -> anyhow::Result<QuadMeshId> {
        // SAFETY: the uploader records into this frame's init commands; the new image isn't in use yet.
        let id = unsafe {
            let id = self.quad_meshes.upload(
                &self.device,
                &self.mem_props,
                &mut self.residency,
                quads,
            )?;
            if let Err(e) = self.quad_sets.sync(
                &self.device,
                self.pipelines.layouts.table,
                self.quad_meshes.buffers(),
            ) {
                self.quad_meshes.free(id, self.frames.number);
                return Err(e);
            }
            id
        };
        Ok(QuadMeshId(id))
    }

    /// Frees quads from [`upload_quads`](Self::upload_quads).
    pub fn free_quads(&mut self, quads: QuadMeshId) {
        self.quad_meshes.free(quads.0, self.frames.number);
    }

    /// Uploads this frame's particle quads.
    pub fn upload_particles(&mut self, quads: &[ParticleQuad]) -> anyhow::Result<ParticleMeshId> {
        // SAFETY: the uploader records into this frame's init commands; the new image isn't in use yet.
        let id = unsafe {
            self.particle_meshes.upload(
                &self.device,
                &self.mem_props,
                &mut self.residency,
                quads,
            )?
        };
        Ok(ParticleMeshId(id))
    }

    /// Frees quads from [`upload_particles`](Self::upload_particles).
    pub fn free_particles(&mut self, quads: ParticleMeshId) {
        self.particle_meshes.free(quads.0, self.frames.number);
    }

    /// Uploads an index list ([`melon_world::mesh::quad_index_list`]) for a vertex mesh.
    pub fn upload_indices(&mut self, list: &[u32]) -> anyhow::Result<IndexMeshId> {
        // SAFETY: the uploader records into this frame's init commands; the new image isn't in use yet.
        let id = unsafe {
            self.index_lists
                .upload(&self.device, &self.mem_props, &mut self.residency, list)?
        };
        Ok(IndexMeshId(id))
    }

    /// Frees an index list from [`upload_indices`](Self::upload_indices).
    pub fn free_indices(&mut self, list: IndexMeshId) {
        self.index_lists.free(list.0, self.frames.number);
    }

    /// VRAM this process may still allocate, as of the last budget read
    /// (at most a second old) less the mesh blocks allocated since; `None`
    /// without `VK_EXT_memory_budget`.
    pub fn available_vram(&self) -> Option<u64> {
        self.residency.available_vram()
    }

    /// Bytes of mesh memory allocated and in use.
    pub fn mesh_memory(&self) -> (u64, u64) {
        let (a, b) = self.meshes.usage();
        let (c, d) = self.quad_meshes.usage();
        let (e, f) = self.index_lists.usage();
        let (g, h) = self.particle_meshes.usage();
        (a + c + e + g, b + d + f + h)
    }

    pub(super) fn uploader(&self, sampler: vk::Sampler) -> texture::Uploader<'_> {
        texture::Uploader {
            device: &self.device,
            mem_props: &self.mem_props,
            descriptor_pools: &self.textures.descriptor_pools,
            descriptor_set_layout: self.pipelines.layout,
            sampler,
        }
    }
}

impl Renderer {
    /// Draws each program's quads with its shader in `shaders` from now on.
    pub fn set_gui_shaders(
        &mut self,
        shaders: &[super::GuiShader],
    ) -> Vec<(melon_base::gfx::quad::Program, anyhow::Error)> {
        let mut failed = Vec::new();
        // SAFETY: the device is waited idle first, so nothing in flight uses what is replaced.
        unsafe {
            if let Err(e) = self.device.device_wait_idle() {
                melon_base::warn!("Waiting for the GPU failed: {e}");
            }
            self.pipelines.ui.clear_pack(&self.device);
            for shader in shaders {
                let bytes = |words: &[u32]| -> Vec<u8> {
                    words.iter().flat_map(|w| w.to_le_bytes()).collect()
                };
                let (vert, frag) = (bytes(&shader.vert), bytes(&shader.frag));
                let built = super::pipeline::ui::create_all(
                    &self.device,
                    self.passes.clear,
                    self.pipelines.layout,
                    self.gui_uniforms.layout,
                    super::pipeline::ui::Shaders {
                        vert: &vert,
                        frag: &frag,
                        inputs: Some(&shader.inputs),
                    },
                );
                match built {
                    Ok(passes) => self.pipelines.ui.pack[shader.program.index()] = Some(passes),
                    Err(e) => failed.push((shader.program, e)),
                }
            }
        }
        failed
    }
}
