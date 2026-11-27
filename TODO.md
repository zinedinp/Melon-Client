# TODO

What is open, per crate. Smaller spots stay in the code as
`// TODO(<area>): …` (`perf`, `parity`, `mods`, `macos`, …), next to what
they are about; this file holds the plans that span modules or crates.

Melon is on majestix-vk 0.2.1 and numerobis 0.6.0.

## melon (the client)

### The renderer onto majestix-vk

Both renderers adopt melon-base's device with `Device::adopt` and take
their frame ring (`Frames`) and swapchain (`Presenter`) from majestix-vk:
the game's and the launcher's `Renderer2d`. Through majestix-vk besides:
`renderer/blur.rs`, the world's per-frame globals (`renderer/globals.rs`),
the UI's vertices and indices (melon-base's `gfx/ui_vertices.rs`, shared by
both), and the pipeline cache (`renderer/pipeline_cache.rs`,
`cache/pipelines.bin`). About 775 `unsafe` blocks are left: ~545 in
`src/renderer`, ~230 in melon-base's `gfx`.

A fixed-size per-frame buffer moves as the globals and the UI vertices did:
a `Mapped` ring, each slot's set or offset fixed up front from
`Frames::slots`. Next of those: melon-base's `gfx/gui_uniforms.rs`.

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
5. **Mesh blocks in the BAR** (`renderer/mesh_arena.rs`). Melon writes VRAM
   it maps where its free list says, while the GPU reads other ranges of
   the same block.

Then the rest, `gfx` before `renderer`, on generated wrappers once
numerobis writes them (see its section).

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
  live in `[workspace.lints]` now, but turning pedantic on gives 6244
  warnings (313 in melon-base): ~2000 `must_use`, ~2400 casts (mostly game
  math), the rest docs and style. Fixed in code, not allowed: melon-base
  first, then the client module by module.
- **CI.** There is none. The GPU checks (`renderer/vk_check.rs`) are unit
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

- **Pipeline statistics.** `renderer/gpu_timer.rs` begins and ends a
  `PIPELINE_STATISTICS` query around the frame and reads it on the host;
  `create_query_pool` refuses that type (no statistics flags), and there is
  no `begin_query` / `end_query`. Occlusion queries come with them.

### Descriptor sets

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
