//! Shared sheets for the item icons baked on the CPU (block models, heads,
//! composed banners): vanilla's `GuiRenderer` draws GUI items into one
//! atlas too. Each icon takes a slot of a page; a full page adds another,
//! so new items never add a texture each.

use image::RgbaImage;

use melon_render::{Renderer, TextureId};

/// Edge length of a page, at most the device's limit.
const PAGE_SIDE: u32 = 1024;

/// Where an icon sits: its page and pixel rectangle `[x, y, w, h]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slot {
    page: usize,
    rect: [u32; 4],
}

struct Page {
    texture: TextureId,
    /// Rows of slots: `(y, height, used width)`.
    shelves: Vec<(u32, u32, u32)>,
    /// Where the next shelf starts.
    next_y: u32,
    /// Slots given back, reused by an icon of the same size.
    free: Vec<[u32; 4]>,
}

pub struct IconAtlas {
    side: u32,
    pages: Vec<Page>,
}

impl IconAtlas {
    pub fn new(max_texture_side: u32) -> Self {
        Self {
            side: PAGE_SIDE.min(max_texture_side),
            pages: Vec::new(),
        }
    }

    /// Copies `image` into a free slot: its page's texture, the slot and
    /// its `[u0, v0, u1, v1]`. `None` when it is bigger than a page or the
    /// page could not be made.
    pub fn place(
        &mut self,
        renderer: &mut Renderer,
        image: &RgbaImage,
    ) -> Option<(TextureId, Slot, [f32; 4])> {
        let (w, h) = image.dimensions();
        let slot = self.find(w, h).or_else(|| {
            let texture = renderer
                .load_texture(&RgbaImage::new(self.side, self.side))
                .map_err(|e| melon_base::warn!("Icon page not created: {e:#}"))
                .ok()?;
            self.pages.push(Page {
                texture,
                shelves: Vec::new(),
                next_y: 0,
                free: Vec::new(),
            });
            self.find(w, h)
        })?;
        let texture = self.pages[slot.page].texture;
        renderer.update_texture(texture, 0, slot.rect[0], slot.rect[1], image.clone());
        Some((texture, slot, self.uv(slot)))
    }

    /// The slot's `[u0, v0, u1, v1]` in its page.
    fn uv(&self, slot: Slot) -> [f32; 4] {
        let side = self.side as f32;
        let [x, y, w, h] = slot.rect.map(|v| v as f32);
        [x / side, y / side, (x + w) / side, (y + h) / side]
    }

    /// Room for `w`x`h`: a freed slot of that size, else the first shelf
    /// as tall with width left, else a new shelf.
    fn find(&mut self, w: u32, h: u32) -> Option<Slot> {
        if w > self.side || h > self.side || w == 0 || h == 0 {
            return None;
        }
        let side = self.side;
        for (index, page) in self.pages.iter_mut().enumerate() {
            if let Some(i) = page.free.iter().position(|r| r[2] == w && r[3] == h) {
                let rect = page.free.swap_remove(i);
                return Some(Slot { page: index, rect });
            }
            for shelf in &mut page.shelves {
                let (y, height, used) = *shelf;
                if height == h && used + w <= side {
                    shelf.2 += w;
                    return Some(Slot {
                        page: index,
                        rect: [used, y, w, h],
                    });
                }
            }
            if page.next_y + h <= side {
                let y = page.next_y;
                page.next_y += h;
                page.shelves.push((y, h, w));
                return Some(Slot {
                    page: index,
                    rect: [0, y, w, h],
                });
            }
        }
        None
    }

    /// Gives `slot` back for a later icon of its size.
    pub fn free(&mut self, slot: Slot) {
        if let Some(page) = self.pages.get_mut(slot.page) {
            page.free.push(slot.rect);
        }
    }

    /// Frees every page.
    pub fn release(&mut self, renderer: &mut Renderer) {
        for page in self.pages.drain(..) {
            renderer.free_texture(page.texture);
        }
    }

    #[cfg(test)]
    fn with_side(side: u32) -> Self {
        Self {
            side,
            pages: Vec::new(),
        }
    }

    #[cfg(test)]
    fn add_page(&mut self) {
        self.pages.push(Page {
            texture: TextureId(self.pages.len()),
            shelves: Vec::new(),
            next_y: 0,
            free: Vec::new(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_fill_shelves_then_pages_and_are_reused() {
        let mut atlas = IconAtlas::with_side(128);
        assert_eq!(atlas.find(64, 64), None, "no page yet");
        atlas.add_page();
        let a = atlas.find(64, 64).unwrap();
        let b = atlas.find(64, 64).unwrap();
        let c = atlas.find(16, 16).unwrap();
        assert_eq!(a.rect, [0, 0, 64, 64]);
        assert_eq!(b.rect, [64, 0, 64, 64]);
        assert_eq!(c.rect, [0, 64, 16, 16]);
        assert_eq!(atlas.find(64, 64), None, "no 64-high row fits below");
        assert_eq!(atlas.find(16, 16).unwrap().rect, [16, 64, 16, 16]);
        atlas.free(b);
        assert_eq!(atlas.find(64, 64), Some(b));
        assert_eq!(atlas.find(200, 8), None, "wider than a page");
        atlas.add_page();
        assert_eq!(atlas.find(64, 64).unwrap().page, 1);
        assert_eq!(atlas.uv(a), [0.0, 0.0, 0.5, 0.5]);
    }
}
