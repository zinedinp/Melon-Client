# TODO

What is open, per crate. Smaller spots stay in the code as
`// TODO(<area>): …` (`perf`, `parity`, `mods`, `macos`, …), next to what
they are about; this file holds the plans that span modules or crates.

Melon is on majestix-vk 0.2.1 and numerobis 0.6.0.

**Next:** a live check of the post chains on a server with Lumiere, then
the post work in "Post effects and shader packs".

## melon (the client)

### The crate split

The client is ten crates in `crates/` (the layering is in `src/lib.rs`);
the bin keeps `game` and `app`. Code names other crates by their own names
(`melon_world::chunk::Chunk`); no crate re-exports another's items, but
melon-net's `Stream`, which `join_stream` takes. What the cut left behind:

- **Widened visibility.** What was `pub(crate)` in a moved module is `pub`
  now, so the crates above still reach it. `unreachable_pub` catches only
  `pub` in private modules (395: 124 in the bin, 108 in melon-render, 76 in
  melon-world); what a public module exports but no other crate uses needs
  a search across the crates.
- **Test helpers outside tests.** Other crates' tests use
  `util::xorshift`, `Lang::empty`/`from_pairs`, `assets::test_root` and
  `test_post_chains`, `Chunk::new`, `TranslucentMesh::sorted_quads` and
  `mesh::quads_of_index_list`, so they are compiled always; a
  `test-support` feature in dev-dependencies would hide them again.
- **`game` (22k lines) and `app`** stay one crate. `game`'s meshing and
  scene code (`level_meshes`, `block_meshes`, `entities`, `scene`) against
  its loop and input would be the next cut.

### The renderer onto majestix-vk

Both renderers adopt melon-base's device with `Device::adopt` and take
their frame ring (`Frames`) and swapchain (`Presenter`) from majestix-vk:
the game's and the launcher's `Renderer2d`. Through majestix-vk besides:
melon-render's `blur.rs`, the world's per-frame globals (`globals.rs`),
the UI's vertices and indices (melon-base's `gfx/ui_vertices.rs`, shared by
both), the GUI's uniform blocks and their sets (`gfx/gui_uniforms.rs`,
with a copy of the glint of its own), and the pipeline cache
(melon-render's `pipeline_cache.rs`, `cache/pipelines.bin`). About 665
`unsafe` blocks are left: ~450 in melon-render, ~215 in melon-base's `gfx`.

A fixed-size per-frame buffer moves as the globals and the UI vertices did:
a `Mapped` ring, each slot's set or offset fixed up front from
`Frames::slots`. Next of those: melon-render's `pack_outline.rs`, one
buffer for every frame in flight that each frame rewrites while the one
before may still read it; per-slot ranges end that.

Of the rest, the memory modules go first, as melon-base and majestix-vk
would otherwise keep two memory managers. In this order, each waiting on one gap in
majestix-vk (see its section):

1. **Rings that grow.** `gfx/staging.rs` is a mapped buffer per frame in
   flight that grows on demand (`Staging::ensure`) and shrinks after a
   burst (`Staging::trim`); `Mapped` has a fixed length.
2. **Textures.** `gfx/suballoc.rs` (images in shared blocks, a dedicated
   allocation past half a block) is what majestix's blocks do already;
   `gfx/texture.rs` moves to `create_image`, `gfx/texture_update.rs`'s
   uploads to copies from the staging ring inside the frame.
3. **Descriptor pools.** `gfx/descriptor_pools.rs` grows by doubling and
   merges back into one pool on a reload.
4. **Residency.** `gfx/residency.rs` puts meshes in the BAR when the CPU maps
   all of VRAM (`MemoryUse::DeviceMapped`) and keeps new buffers within the
   budget.
5. **Mesh blocks in the BAR** (melon-render's `mesh_arena.rs`). Melon writes VRAM
   it maps where its free list says, while the GPU reads other ranges of
   the same block.

Then the rest, `gfx` before melon-render, on generated wrappers once
numerobis writes them (see its section).

### Post effects and shader packs

Resource packs' chains run as vanilla runs them; shader packs come in the
OptiFine/Iris format, a Melon shader type (with hardware ray tracing) after
that. Plan and vanilla's rules: `notes/post-and-shaderpacks.md` (local).

Done so far (M1, first half):
- Chains run by id: every `post_effect/*.json` of the packs, with
  `end_of_frame`, then the server's list (`ClientboundPostEffectsPacket`).
- The outline chain runs only when an outlined entity is drawn.
- Chains run after the hand and the outline's blit.
- Targets: cleared per run (`clear_color`), `persistent` kept, passes load.
- `Globals` carries the level's `GameTime` and the camera.
- The pack's outline shader reads a `Mapped` ring: one set per frame slot,
  each entity at a dynamic offset, up to 1024 a frame (was 64, and raced
  with the frame in flight).

Open:
- **The rest of post on majestix-vk.** The chains are on it (images, effect
  textures, one loaded `Pass`, pipelines, sets, barriers; the main depth
  and the swapchain views too). Raw still: the glowing-entity draw and the
  pack outline pipeline (built against the raw `color_pass`) and the
  outline blit (inside the scene's raw `resume` pass); they move with the
  scene passes. A graph core for the shader packs' composite passes grows
  from `post/gpu.rs`.
- **The hand's own depth** when chains follow (`integrate3DHudDepth`):
  chains after the hand see only its depth today.
- **The spectated entity's chain** (creeper, spider, enderman): needs
  `ClientboundSetCameraPacket`, which Melon doesn't handle yet.
- **`blur.json`** for the menu blur instead of `blur.rs`.
- **`gl_VertexIndex` in the pack outline pass** counts from the entity's
  place in the mesh arena; vanilla batches the frame's outlined vertices.
  Lumiere's markers read it (`notes/post-and-shaderpacks.md`).
- **Shader packs**: loading and picking (M2), the GLSL translator (M3), the
  pipeline (M4), breadth (M5).

### Frame hitches

Measured per frame in the bench CSV (`notes/render-performance.md`,
"Hitches"). Left on the main thread:

- **A mob texture's first load** (~0.2-0.7 ms each, mostly the pack
  lookup in `texture_with_meta`); a crowd spawning loads dozens.
- **The cloud mesh** (~3.4 ms per cell crossed at cloud range 128): built
  on a worker, it would swap in a frame later.
- **Leaving a singleplayer world** waits for the server's save
  (`IntegratedServer::drop` joins its shutdown): the window freezes.

### Frame time

CPU-bound at ~1.2-1.9 ms a frame (`notes/render-performance.md`, "CPU
frame time"). Next cuts, largest first:

- **GPU-expanded terrain draws** (the notes' plan): `cull sections` and
  `gather`, ~0.4 ms together even on four threads.
- **Translucent sections as multi-draws** per index block: `record scene`
  grows with the water in view (0.25 ms at 0.7 M primitives).
- **Entity hitboxes with F3 open** rebuild their gizmo mesh every frame:
  0.2 ms at 250 mobs.

### Features

- **Opus proximity voice chat** (`audio`): each speaker a `Binaural` /
  `Placed` source at their position.

### Vulkan paths not taken yet

`features::GAME` asks for six capabilities; these others would each add a
path beside the 1.0 one:

- **Memory priorities.** `PageableDeviceLocalMemory` is on (with
  `VK_EXT_memory_priority` beneath it), but every allocation keeps the
  default priority. Render targets high, far mesh blocks low
  (`vkSetDeviceMemoryPriorityEXT`) would decide what pages out first.
- **Dynamic rendering.** majestix-vk's passes take it when the plan has it,
  but Melon's own render passes are raw and `GAME` doesn't ask for it.
- **Host image copy** (`VK_EXT_host_image_copy`): texture uploads without
  the staging buffer and its copy.
- **Push descriptors, descriptor indexing, buffer device addresses**: the
  per-draw sets and bindless (see majestix-vk's "Later").

### Lints, tests, CI

- **`clippy::pedantic`**, as majestix-vk and keinbonus have it. Shared lints
  live in `[workspace.lints]`; pedantic gives 5777 warnings: melon-world
  2856, melon-gui 973, the bin 445, melon-base and melon-render 302 each,
  melon-net 264, melon-core 247, melon-protocol 125, melon-assets 107,
  melon-audio 95, melon-settings 61. ~2000 `must_use_candidate`, ~2200
  casts (mostly game math), the rest docs and style. Fixed in code, not
  allowed, crate by crate from the small ones up; a crate that is clean
  takes its own `[lints]` (the workspace's plus pedantic) until all are.
- **CI.** There is none. The GPU checks (melon-render's `vk_check.rs`) are unit
  tests that need a device; with lavapipe or SwiftShader they could run on
  a runner, as majestix-vk's could.

## melon-base

- **OpenGL and DirectX backends** beside Vulkan, for systems without a
  usable Vulkan driver, behind majestix-vk; they need `gfx` and the
  renderer fully on it first.

## majestix-vk

What Melon's renderer needs from it, most important first.

### For the memory move

- **`Mapped::reserve(len)`**: a larger buffer, the old one retired under the
  current epoch, and a trim after quiet frames (step 1).
- **Pool compaction.** Pools grow by 256 sets and are never destroyed
  before the device; a pool whose sets were all freed could go (step 3).
- **Budget per heap.** `available_vram` looks at the largest device-local
  heap only; the BAR heap has a budget of its own (step 4).
- **Ranges freed per finished epoch** in a mapped block: `Mapped` hands out
  a frame's whole range only (step 5).

### Queries

- **Pipeline statistics.** melon-render's `gpu_timer.rs` begins and ends a
  `PIPELINE_STATISTICS` query around the frame and reads it on the host;
  `create_query_pool` refuses that type (no statistics flags), and there is
  no `begin_query` / `end_query`. Occlusion queries come with them.

### Descriptor sets

- **Sets naming the host's images.** `Descriptor` takes majestix-vk's own
  `ImageView` and `Sampler`, and a set is written whole, once. melon-base's
  textures (views it destroys itself) need a view lent for as long as the
  host says; the GUI's glint works around it with a copy of its own.
- **Texel buffers and immutable samplers.** `create_descriptor_set` panics
  on `*_TEXEL_BUFFER` (no buffer views yet), and `Binding` has no immutable
  samplers.
- **More than four dynamic buffers in a set** panic; the limits are kept
  inline in the `Copy` handle.
- **Layout compatibility at bind.** A set bound with a pipeline layout whose
  set layout differs is the validation layer's to catch.

### Pipelines

- **Specialization constants, tessellation and geometry stages,
  multisampled pipelines with sample shading**: `Graphics` has none.

### Swapchain

- **HDR metadata** (`vkSetHdrMetadataEXT`) for the `Hdr10` output, from
  numerobis's `HdrMetadata`.
- **`VK_EXT_swapchain_maintenance1`** present fences, so a recreate no
  longer waits for the device to go idle.

### Memory

- **Defragmentation.** Blocks are freed when empty, never compacted.

### Later

Melon calls none of these today.

- **Queue family transfers.** Barriers always pass `QUEUE_FAMILY_IGNORED`:
  no async compute or dedicated transfer queue without release and acquire
  pairs.
- **Recording on several threads**: secondary command buffers, or several
  primaries per frame from per-thread pools.
- **Bindless**: descriptor indexing (update-after-bind, partially bound)
  needs sets whose slots change one at a time, freed per epoch; or buffer
  device addresses.
- **Mesh shader draws** (`vkCmdDrawMeshTasksEXT`), routed by the plan.
- **Queue labels** (`vkQueueBeginDebugUtilsLabelEXT`) beside the command
  buffer ones.
- **CI** with lavapipe or SwiftShader for the GPU tests, and a benchmark of
  the liveness check's cost per command.

## numerobis

What Melon uses: byte views (`elem::as_bytes`), memory types
(`Memory::types_for`), shared blocks (`alloc::Block`; 575 live allocations
down to 63 in the bench), the swapchain's and `format::depth`'s choices,
the PCI scan, and through majestix-vk the plan and its barriers.

- **Timeline semaphores**, routed by the plan like the barriers: majestix's
  `Frames` waits on fences, submits signal binary semaphores only.
- **Generated wrappers for the remaining calls**: `scripts/gen-vulkan.py`
  writes them into majestix's `src/generated.rs` from the same `vk.xml` pass
  (each command's parent handle and its `externsync` parameters).
- **The copy bounds** (`format::copy_end`) are data plus logic too and could
  move beside `format::block` once a second host needs them.
- **Image layouts between passes** stay the host's job (correctness, not
  memory safety); a declarative pass list (which pass reads and writes
  what) could derive them later.
