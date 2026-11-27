//! Where a container screen's panel, slots and labels are.

/// A slot's pitch: 16 pixels of item and a 2-pixel frame.
pub const SLOT_PITCH: f32 = 18.0;
/// The item area of a slot.
pub const SLOT_SIZE: f32 = 16.0;

/// A horizontal strip of the panel's sheet.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strip {
    pub y: f32,
    pub v: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuLayout {
    width: f32,
    height: f32,
    left: f32,
    top: f32,
    /// Top-left of each slot's item area, relative to the panel, by menu slot.
    slots: Vec<(f32, f32)>,
    strips: Vec<Strip>,
    title: (f32, f32),
    /// Whether the title is centred on the panel instead of at `title.0`.
    title_centered: bool,
    /// Whether the screen leaves the title out (the beacon has none).
    title_hidden: bool,
    inventory_label: Option<(f32, f32)>,
    /// The size of the sheet the panel is cut from.
    sheet: (f32, f32),
}

impl MenuLayout {
    /// A `width` x `height` panel drawn from the sheet's top-left.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            left: 0.0,
            top: 0.0,
            slots: Vec::new(),
            strips: vec![Strip {
                y: 0.0,
                v: 0.0,
                height,
            }],
            title: (8.0, 6.0),
            title_centered: false,
            title_hidden: false,
            inventory_label: None,
            sheet: (256.0, 256.0),
        }
    }

    /// Draws the panel from these strips of the sheet instead of one piece.
    pub fn strips(mut self, strips: Vec<Strip>) -> Self {
        self.strips = strips;
        self
    }

    pub fn title_at(mut self, x: f32, y: f32) -> Self {
        self.title = (x, y);
        self
    }

    /// Centres the title on the panel (`(imageWidth - width) / 2`).
    pub fn centered_title(mut self) -> Self {
        self.title_centered = true;
        self
    }

    /// No title: the screen draws its own, or none.
    pub fn without_title(mut self) -> Self {
        self.title_hidden = true;
        self
    }

    /// The panel is cut from a `width` x `height` sheet instead of 256x256.
    pub fn on_sheet(mut self, width: f32, height: f32) -> Self {
        self.sheet = (width, height);
        self
    }

    /// The "Inventory" label at vanilla's `(8, imageHeight - 94)`.
    pub fn inventory_label(self) -> Self {
        self.inventory_label_at(8.0)
    }

    /// The "Inventory" label at `(x, imageHeight - 94)`.
    pub fn inventory_label_at(mut self, x: f32) -> Self {
        self.inventory_label = Some((x, self.height - 94.0));
        self
    }

    /// The next slot.
    pub fn slot(mut self, x: f32, y: f32) -> Self {
        self.slots.push((x, y));
        self
    }

    /// The next `columns` x `rows` slots, row by row from `(x, y)`.
    pub fn grid(mut self, x: f32, y: f32, columns: usize, rows: usize) -> Self {
        for row in 0..rows {
            for column in 0..columns {
                self.slots
                    .push((x + column as f32 * SLOT_PITCH, y + row as f32 * SLOT_PITCH));
            }
        }
        self
    }

    /// `AbstractContainerMenu.addStandardInventorySlots`.
    pub fn player_inventory(self, x: f32, y: f32) -> Self {
        self.grid(x, y, 9, 3).grid(x, y + 58.0, 9, 1)
    }

    /// Centres the panel on a `width` x `height` GUI (`leftPos`, `topPos`).
    pub fn place(&mut self, width: f32, height: f32) {
        self.left = ((width - self.width) / 2.0).floor();
        self.top = ((height - self.height) / 2.0).floor();
    }

    /// [`Self::place`], with the panel's left edge at `left` (a recipe book beside it).
    pub fn place_at(&mut self, left: f32, height: f32) {
        self.left = left;
        self.top = ((height - self.height) / 2.0).floor();
    }

    pub fn left(&self) -> f32 {
        self.left
    }

    pub fn top(&self) -> f32 {
        self.top
    }

    pub fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    pub fn strip_list(&self) -> &[Strip] {
        &self.strips
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The sheet the panel is cut from.
    pub fn sheet_size(&self) -> (f32, f32) {
        self.sheet
    }

    /// Whether the layout shows the container's title itself.
    pub fn shows_title(&self) -> bool {
        !self.title_hidden
    }

    /// Where a title `text_width` wide goes on screen.
    pub fn title_origin(&self, text_width: f32) -> (f32, f32) {
        let x = if self.title_centered {
            ((self.width - text_width) / 2.0).floor()
        } else {
            self.title.0
        };
        (self.left + x, self.top + self.title.1)
    }

    /// Where the "Inventory" label goes on screen, if it has one.
    pub fn inventory_label_origin(&self) -> Option<(f32, f32)> {
        self.inventory_label
            .map(|(x, y)| (self.left + x, self.top + y))
    }

    /// Slot `slot`'s position relative to the panel.
    pub fn slot_position(&self, slot: usize) -> Option<(f32, f32)> {
        self.slots.get(slot).copied()
    }

    /// Top-left of slot `slot`'s item area on screen.
    pub fn slot_origin(&self, slot: usize) -> Option<(f32, f32)> {
        self.slot_position(slot)
            .map(|(x, y)| (self.left + x, self.top + y))
    }

    /// Every slot with its top-left on screen.
    pub fn slot_origins(&self) -> impl Iterator<Item = (usize, (f32, f32))> + '_ {
        (0..self.slots.len()).filter_map(|slot| Some((slot, self.slot_origin(slot)?)))
    }

    /// `AbstractContainerScreen.getHoveredSlot`.
    pub fn slot_at(&self, (mx, my): (f32, f32)) -> Option<usize> {
        self.slot_origins().find_map(|(slot, (x, y))| {
            (mx >= x - 1.0 && mx < x + SLOT_SIZE + 1.0 && my >= y - 1.0 && my < y + SLOT_SIZE + 1.0)
                .then_some(slot)
        })
    }

    /// `hasClickedOutside`: off the panel.
    pub fn outside(&self, (mx, my): (f32, f32)) -> bool {
        mx < self.left
            || my < self.top
            || mx >= self.left + self.width
            || my >= self.top + self.height
    }

    /// The slots whose item area doesn't lie inside the panel.
    pub fn slots_outside_panel(&self) -> Vec<usize> {
        self.slots
            .iter()
            .enumerate()
            .filter(|&(_, &(x, y))| {
                x < 0.0 || y < 0.0 || x + SLOT_SIZE > self.width || y + SLOT_SIZE > self.height
            })
            .map(|(slot, _)| slot)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_standard_inventory_puts_the_hotbar_58_below() {
        let layout = MenuLayout::new(176.0, 166.0).player_inventory(8.0, 84.0);
        assert_eq!(layout.len(), 36);
        assert_eq!(layout.slot_position(0), Some((8.0, 84.0)));
        assert_eq!(layout.slot_position(26), Some((152.0, 120.0)));
        assert_eq!(layout.slot_position(27), Some((8.0, 142.0)));
        assert!(layout.slots_outside_panel().is_empty());
    }

    #[test]
    fn the_mouse_finds_a_slot_by_its_frame() {
        let mut layout = MenuLayout::new(176.0, 166.0).slot(8.0, 8.0);
        layout.place(276.0, 266.0);
        assert_eq!(layout.slot_at((57.0, 57.0)), Some(0));
        assert_eq!(layout.slot_at((74.0, 58.0)), Some(0));
        assert_eq!(layout.slot_at((75.0, 58.0)), None);
        assert!(layout.outside((49.0, 60.0)));
        assert!(!layout.outside((50.0, 50.0)));
    }

    #[test]
    fn a_slot_past_the_edge_is_reported() {
        let layout = MenuLayout::new(176.0, 166.0).player_inventory(8.0, 103.0);
        assert_eq!(layout.slots_outside_panel(), (27..36).collect::<Vec<_>>());
    }
}
