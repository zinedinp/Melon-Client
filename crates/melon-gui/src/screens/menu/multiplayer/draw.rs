//! Drawing one server row (`ServerSelectionList.OnlineServerEntry`).

use super::*;

/// `LoadingDotsWidget`'s frames.
const DOTS: [&str; 4] = ["O o o", "o O o", "o o O", "o O o"];
/// `LoadingDotsWidget`: the dots step every 300 ms.
const DOTS_STEP_MS: u128 = 300;

impl MultiplayerScreen {
    /// `ServerSelectionList.LANHeader`: "Scanning for games on your local
    /// network" over `LoadingDotsWidget`'s dots, centred in the row.
    pub(super) fn draw_lan_header(&self, ui: &mut Ui, rect: Rect) {
        let content = Self::content(rect);
        let middle = content.x + content.w / 2.0;
        let label = ui.tr("lanServer.scanning");
        ui.centered(label, middle, content.y, [1.0; 4]);
        let step = (self.opened.elapsed().as_millis() / DOTS_STEP_MS) as usize % DOTS.len();
        ui.centered(
            DOTS[step],
            middle,
            content.y + LINE_HEIGHT,
            [GREY[0], GREY[1], GREY[2], 1.0],
        );
    }

    /// `ServerSelectionList.NetworkServerEntry`: a world opened to the LAN.
    pub(super) fn draw_lan_row(&self, ui: &mut Ui, index: usize, rect: Rect) {
        let Some(server) = self.lan_servers.get(index) else {
            return;
        };
        let content = Self::content(rect);
        let x = content.x + ICON + 3.0;
        let title = ui.tr("lanServer.title");
        ui.text(title, x, content.y + 1.0, [1.0; 4]);
        let grey = [GREY[0], GREY[1], GREY[2], 1.0];
        ui.text(&server.motd, x, content.y + 12.0, grey);
        ui.text(&server.address, x, content.y + 12.0 + 11.0, grey);
    }

    /// One `ServerSelectionList.OnlineServerEntry`.
    pub(super) fn draw_row(&self, ui: &mut Ui, index: usize, rect: Rect) {
        let server = &self.servers[index];
        let row = &self.rows[index];
        let content = Self::content(rect);
        let sprites = &ui.assets.server_list;

        ui.text(
            &server.name,
            content.x + ICON + 3.0,
            content.y + 1.0,
            [1.0; 4],
        );

        let text_x = content.x + ICON + 3.0;
        match &row.ping {
            Ping::Pinging => {
                let text = StyledText::plain(ui.tr("multiplayer.status.pinging"));
                ui.styled(&text, text_x, content.y + 12.0, GREY);
            }
            Ping::Failed { resolve } => {
                let key = if *resolve {
                    "multiplayer.status.cannot_resolve"
                } else {
                    "multiplayer.status.cannot_connect"
                };
                let text = StyledText::colored(ui.tr(key), PURE_RED);
                ui.styled(&text, text_x, content.y + 12.0, GREY);
            }
            Ping::Done(answer) => {
                if let Some(motd) = &answer.motd {
                    let lines = answer.motd_lines.get_or_init(|| {
                        let text = StyledText::from_component(motd, ui.lang);
                        ui.wrap_styled(&text, content.w - ICON - 2.0)
                            .into_iter()
                            .take(2)
                            .map(|text| MotdLine {
                                sprites: sprite_slots(ui.font(), &text),
                                text,
                            })
                            .collect()
                    });
                    for (
                        line,
                        MotdLine {
                            text,
                            sprites: slots,
                        },
                    ) in lines.iter().enumerate()
                    {
                        let y = content.y + 12.0 + line as f32 * LINE_HEIGHT;
                        ui.styled(text, text_x, y, GREY);
                        for &(dx, sprite) in slots {
                            let texture = row.sprites.as_ref().and_then(|s| s.get(sprite)).copied();
                            if let Some(texture) = texture.flatten() {
                                ui.sprite(texture, Rect::new(text_x + dx, y, 8.0, 8.0));
                            }
                        }
                    }
                }
            }
        }

        let icon = row.icon.unwrap_or(sprites.unknown_server);
        let icon_rect = Rect::new(content.x, content.y, ICON, ICON);
        ui.sprite(icon, icon_rect);
        let hovered = self.list.row_at(self.mouse.0, self.mouse.1) == Some(index);

        let status_x = content.right() - STATUS_W - SPACING_X;
        let sprites = &ui.assets.server_list;
        let status_icon = match &row.ping {
            Ping::Pinging => {
                let step = (self.opened.elapsed().as_millis() / 100 + index as u128 * 2) & 7;
                sprites.pinging[(if step > 4 { 8 - step } else { step }) as usize]
            }
            Ping::Failed { .. } => sprites.unreachable,
            Ping::Done(answer) if !answer.compatible => sprites.incompatible,
            Ping::Done(answer) => sprites.ping[ping_level(answer.status.latency_ms)],
        };
        ui.sprite(
            status_icon,
            Rect::new(status_x, content.y, STATUS_W, STATUS_H),
        );
        if let Ping::Done(answer) = &row.ping {
            let text = if answer.compatible {
                answer.players.clone()
            } else {
                StyledText::colored(version_label(ui, &answer.status), colors::RED)
            };
            let width = crate::gui::text::width(ui.font(), &text);
            ui.styled(&text, status_x - width - SPACING_X, content.y + 1.0, GREY);
        }

        if hovered {
            let zone = self
                .hovered_zone()
                .filter(|(i, _)| *i == index)
                .map(|z| z.1);
            let sprites = &ui.assets.server_list.row;
            row_icon::draw(ui, icon_rect, sprites, zone, self.moves(index));
        }
    }

    /// The tooltip of what the mouse is on.
    pub(super) fn draw_tooltip(&self, ui: &mut Ui) {
        let (mouse_x, mouse_y) = self.mouse;
        let Some(index) = self
            .list
            .row_at(mouse_x, mouse_y)
            .filter(|&i| i < self.servers.len())
        else {
            return;
        };
        let row = &self.rows[index];
        let content = Self::content(self.list.row_rect(index));
        let status_x = content.right() - STATUS_W - SPACING_X;
        let over_bars = mouse_x >= status_x
            && mouse_x <= status_x + STATUS_W
            && mouse_y >= content.y
            && mouse_y <= content.y + STATUS_H;
        if over_bars {
            let text = match &row.ping {
                Ping::Pinging => ui.tr("multiplayer.status.pinging").to_owned(),
                Ping::Failed { .. } => ui.tr("multiplayer.status.no_connection").to_owned(),
                Ping::Done(answer) if !answer.compatible => {
                    ui.tr("multiplayer.status.incompatible").to_owned()
                }
                Ping::Done(answer) => ui.lang.format(
                    "multiplayer.status.ping",
                    &[&answer.status.latency_ms.to_string()],
                ),
            };
            ui.mouse_tooltip(&[StyledText::plain(&text)], self.mouse);
            return;
        }
        let Ping::Done(answer) = &row.ping else {
            return;
        };
        let text_width = if answer.compatible {
            crate::gui::text::width(ui.font(), &answer.players)
        } else {
            ui.text_width(version_label(ui, &answer.status))
        };
        let text_x = status_x - SPACING_X - text_width;
        let over_text = mouse_x >= text_x
            && mouse_x <= text_x + text_width
            && mouse_y >= content.y
            && mouse_y <= content.y + LINE_HEIGHT - 1.0;
        if over_text && !answer.status.sample.is_empty() {
            let mut lines: Vec<StyledText> = answer
                .status
                .sample
                .iter()
                .map(|name| StyledText::plain(name))
                .collect();
            let listed = answer.status.sample.len() as i64;
            if listed < answer.status.online {
                let more = (answer.status.online - listed).to_string();
                lines.push(StyledText::plain(
                    &ui.lang.format("multiplayer.status.and_more", &[&more]),
                ));
            }
            ui.mouse_tooltip(&lines, self.mouse);
        }
    }
}
