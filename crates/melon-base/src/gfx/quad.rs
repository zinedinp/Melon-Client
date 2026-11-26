use super::TextureId;

/// 2D affine (scale, rotate, translate). Splash text is the only rotated draw.
#[derive(Clone, Copy)]
pub struct Transform2D {
    pub translate: [f32; 2],
    /// Sine and cosine of the rotation.
    pub sin_cos: (f32, f32),
    pub scale: f32,
}

impl Transform2D {
    pub fn apply(&self, p: [f32; 2]) -> [f32; 2] {
        let (s, c) = self.sin_cos;
        let x = p[0] * self.scale;
        let y = p[1] * self.scale;
        [
            x * c - y * s + self.translate[0],
            x * s + y * c + self.translate[1],
        ]
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Vertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    /// RGBA as unsigned normalized bytes, as vanilla's `ARGB` colours.
    pub color: [u8; 4],
}

const _: () = assert!(std::mem::size_of::<Vertex>() == 20);
// SAFETY: `repr(C)`, every field `Pod`, and the size above is the fields' sum: no padding.
unsafe impl numerobis::elem::Pod for Vertex {}

/// `0..=1` as a byte, rounded to the nearest; `+ 0.5` and a cast, as x86-64
/// has no inline `round` before SSE4.1.
fn unorm8(color: [f32; 4]) -> [u8; 4] {
    color.map(|c| (c.clamp(0.0, 1.0) * 255.0 + 0.5) as u8)
}

/// Which blend function a draw call wants. Vanilla picks a `RenderPipeline`
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum BlendMode {
    /// Straight alpha-over, `BlendFunction.TRANSLUCENT`.
    #[default]
    Alpha,
    /// `BlendFunction.INVERT`: `src * (1 - dst) + dst * (1 - src)`.
    Invert,
    /// `BlendFunction.ADDITIVE`: `src + dst` (`GUI_TEXT_HIGHLIGHT`).
    Additive,
    /// The vignette's (`RenderPipelines.VIGNETTE`).
    Vignette,
}

/// Which of vanilla's GUI shaders a draw call stands for, so a resource pack's can replace it.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Program {
    /// Sprites and textures: `core/position_tex_color`.
    #[default]
    Sprite,
    /// Solid fills and gradients: `core/gui`.
    Fill,
    /// Glyphs and their underlines: `core/text` with `IS_GUI`.
    Text,
    /// Enchanted item icons: the baked sprite plus the scrolling glint.
    Glint,
}

impl Program {
    pub const ALL: [Program; 3] = [Program::Sprite, Program::Fill, Program::Text];

    pub fn index(self) -> usize {
        self as usize
    }
}

pub struct DrawCall {
    pub texture: TextureId,
    /// The first index and the index count.
    pub first: u32,
    pub count: u32,
    /// `[x, y, w, h]` in GUI coords, or `None` for the full window.
    pub scissor: Option<[f32; 4]>,
    pub blend: BlendMode,
    pub program: Program,
}

/// Immediate-mode quad list, rebuilt each frame.
pub struct DrawList {
    pub vertices: Vec<Vertex>,
    pub calls: Vec<DrawCall>,
    current_scissor: Option<[f32; 4]>,
    current_blend: BlendMode,
    current_program: Program,
    /// Calls before this index are never merged into.
    sealed: usize,
}

/// `[x, y, w, h]`'s corners: top left, bottom left, bottom right, top right.
fn corners([x, y, w, h]: [f32; 4]) -> [[f32; 2]; 4] {
    [[x, y], [x, y + h], [x + w, y + h], [x + w, y]]
}

/// `[u0, v0, u1, v1]` in [`corners`]' order.
fn rect_uvs([u0, v0, u1, v1]: [f32; 4]) -> [[f32; 2]; 4] {
    [[u0, v0], [u0, v1], [u1, v1], [u1, v0]]
}

impl DrawList {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
            calls: Vec::new(),
            current_scissor: None,
            current_blend: BlendMode::Alpha,
            current_program: Program::Sprite,
            sealed: 0,
        }
    }

    /// The index of the next call, which starts a call of its own.
    pub fn seal(&mut self) -> usize {
        self.sealed = self.calls.len();
        self.sealed
    }

    /// Empties the list for the next frame, keeping its memory.
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.calls.clear();
        self.current_scissor = None;
        self.current_blend = BlendMode::Alpha;
        self.current_program = Program::Sprite;
        self.sealed = 0;
    }

    /// Where the next vertex goes, for [`Self::scale_since`].
    pub fn vertices_len(&self) -> usize {
        self.vertices.len()
    }

    /// Scales every vertex pushed since `start` about the origin.
    pub fn scale_since(&mut self, start: usize, scale: f32) {
        for vertex in &mut self.vertices[start..] {
            vertex.pos = vertex.pos.map(|p| p * scale);
        }
    }

    /// Moves every vertex pushed since `start` by `by`.
    pub fn offset_since(&mut self, start: usize, by: [f32; 2]) {
        for vertex in &mut self.vertices[start..] {
            vertex.pos = [vertex.pos[0] + by[0], vertex.pos[1] + by[1]];
        }
    }

    /// Clip later quads to `[x, y, w, h]` GUI coords (`None` = unclipped).
    pub fn set_scissor(&mut self, scissor: Option<[f32; 4]>) {
        self.current_scissor = scissor;
    }

    /// The clip rectangle in force, to restore after a nested one.
    pub fn scissor(&self) -> Option<[f32; 4]> {
        self.current_scissor
    }

    /// Blend function for later quads.
    pub fn set_blend(&mut self, blend: BlendMode) {
        self.current_blend = blend;
    }

    /// Which GUI shader later quads stand for; returns the one before, to restore it.
    pub fn set_program(&mut self, program: Program) -> Program {
        std::mem::replace(&mut self.current_program, program)
    }

    /// A whole texture, untinted (vanilla `blitSprite` of a plain sprite).
    pub fn push_sprite(&mut self, texture: TextureId, x: f32, y: f32, w: f32, h: f32) {
        self.push_rect(texture, [x, y, w, h], [0.0, 0.0, 1.0, 1.0], [1.0; 4]);
    }

    /// A solid rectangle (vanilla `fill`), drawn with the 1x1 white texture.
    pub fn push_fill(&mut self, white: TextureId, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        let program = self.fill_program();
        self.push_corners(
            white,
            corners([x, y, w, h]),
            [[0.0, 0.0]; 4],
            [color; 4],
            program,
        );
    }

    /// [`Program::Fill`], unless text is being drawn.
    fn fill_program(&self) -> Program {
        if self.current_program == Program::Text {
            Program::Text
        } else {
            Program::Fill
        }
    }

    /// One quad: `corners` top left, bottom left, bottom right, top right.
    fn push_corners(
        &mut self,
        texture: TextureId,
        corners: [[f32; 2]; 4],
        uvs: [[f32; 2]; 4],
        colors: [[f32; 4]; 4],
        program: Program,
    ) {
        let first = (self.vertices.len() / 4 * 6) as u32;
        self.vertices.extend((0..4).map(|i| Vertex {
            pos: corners[i],
            uv: uvs[i],
            color: unorm8(colors[i]),
        }));
        self.push_call(texture, first, 6, program);
    }

    /// `rect` is `[x, y, w, h]`, `uv` `[u0, v0, u1, v1]`.
    pub fn push_rect(&mut self, texture: TextureId, rect: [f32; 4], uv: [f32; 4], color: [f32; 4]) {
        self.push_corners(
            texture,
            corners(rect),
            rect_uvs(uv),
            [color; 4],
            self.current_program,
        );
    }

    /// A quad of any four corners, in the order they go round, each with its UV.
    pub fn push_quad(
        &mut self,
        texture: TextureId,
        corners: [[f32; 2]; 4],
        uvs: [[f32; 2]; 4],
        color: [f32; 4],
    ) {
        self.push_corners(texture, corners, uvs, [color; 4], self.current_program);
    }

    /// `push_rect` after `transform` (splash text).
    pub fn push_rect_transformed(
        &mut self,
        texture: TextureId,
        transform: &Transform2D,
        rect: [f32; 4],
        uv: [f32; 4],
        color: [f32; 4],
    ) {
        let corners = corners(rect).map(|p| transform.apply(p));
        self.push_corners(
            texture,
            corners,
            rect_uvs(uv),
            [color; 4],
            self.current_program,
        );
    }

    /// `push_rect` as a parallelogram: `shear` moves the top and bottom edge along x.
    pub fn push_rect_sheared(
        &mut self,
        texture: TextureId,
        [x, y, w, h]: [f32; 4],
        [top, bottom]: [f32; 2],
        uv: [f32; 4],
        color: [f32; 4],
    ) {
        let corners = [
            [x + top, y],
            [x + bottom, y + h],
            [x + w + bottom, y + h],
            [x + w + top, y],
        ];
        self.push_corners(
            texture,
            corners,
            rect_uvs(uv),
            [color; 4],
            self.current_program,
        );
    }

    /// Repeats `texture` (native width `tile_w`) across the rect instead of stretching it.
    pub fn push_rect_tiled_x(
        &mut self,
        texture: TextureId,
        rect: [f32; 4],
        tile_w: f32,
        color: [f32; 4],
    ) {
        let w = rect[2];
        if w > 0.0 {
            self.push_rect(texture, rect, [0.0, 0.0, w / tile_w, 1.0], color);
        }
    }

    /// Solid vertical gradient on the white texture.
    pub fn push_vgradient(
        &mut self,
        texture: TextureId,
        rect: [f32; 4],
        top: [f32; 4],
        bottom: [f32; 4],
    ) {
        let uvs = [[0.0, 0.0], [0.0, 1.0], [1.0, 1.0], [1.0, 0.0]];
        let program = self.fill_program();
        self.push_corners(
            texture,
            corners(rect),
            uvs,
            [top, bottom, bottom, top],
            program,
        );
    }

    fn push_call(&mut self, texture: TextureId, first: u32, count: u32, program: Program) {
        if self.calls.len() > self.sealed
            && let Some(last) = self.calls.last_mut()
            && last.texture.0 == texture.0
            && last.first + last.count == first
            && last.scissor == self.current_scissor
            && last.blend == self.current_blend
            && last.program == program
        {
            last.count += count;
            return;
        }
        self.calls.push(DrawCall {
            texture,
            first,
            count,
            scissor: self.current_scissor,
            blend: self.current_blend,
            program,
        });
    }
}

impl Default for DrawList {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_merge_until_sealed() {
        let mut list = DrawList::new();
        let white = TextureId(0);
        list.push_fill(white, 0.0, 0.0, 1.0, 1.0, [1.0; 4]);
        list.push_fill(white, 0.0, 0.0, 1.0, 1.0, [1.0; 4]);
        assert_eq!(list.calls.len(), 1);

        let split = list.seal();
        list.push_fill(white, 0.0, 0.0, 1.0, 1.0, [1.0; 4]);
        list.push_fill(white, 0.0, 0.0, 1.0, 1.0, [1.0; 4]);
        assert_eq!((split, list.calls.len()), (1, 2));
        assert_eq!(list.calls[1].count, 12);

        list.clear();
        list.push_fill(white, 0.0, 0.0, 1.0, 1.0, [1.0; 4]);
        list.push_fill(white, 0.0, 0.0, 1.0, 1.0, [1.0; 4]);
        assert_eq!(list.calls.len(), 1);
    }
}
