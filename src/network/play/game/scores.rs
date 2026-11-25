//! The packets of the scoreboard and the tab list.

use crate::component::Component;
use crate::network::protocol::packet::Reader;
use crate::world::scoreboard::{
    CollisionRule, DisplaySlot, NumberFormat, Objective, RenderType, Score, ScoreUpdate,
    TeamAction, TeamColor, TeamLook, Visibility,
};
use anyhow::Result;
use pumpkin_data::packet::clientbound::play as cb;

/// What one of these packets says.
#[derive(Clone, Debug, PartialEq)]
pub enum ScorePacket {
    /// `ClientboundTabListPacket`: empty text clears a part.
    TabList {
        header: Component,
        footer: Component,
    },
    Scoreboard(ScoreUpdate),
}

/// Decodes `pid` if it is one of these packets.
pub fn decode(pid: i32, body: &[u8]) -> Result<Option<ScorePacket>> {
    let mut r = Reader::new(body);
    let update = match pid {
        p if p == cb::TAB_LIST.0 => {
            return Ok(Some(ScorePacket::TabList {
                header: r.component()?,
                footer: r.component()?,
            }));
        }
        p if p == cb::SET_OBJECTIVE.0 => {
            let name = r.string()?.into();
            match r.u8()? {
                1 => ScoreUpdate::RemoveObjective(name),
                method => ScoreUpdate::SetObjective {
                    name,
                    objective: Objective {
                        display_name: r.component()?,
                        render_type: RenderType::from_id(r.var_int()?),
                        number_format: optional_number_format(&mut r)?,
                    },
                    add: method == 0,
                },
            }
        }
        p if p == cb::SET_DISPLAY_OBJECTIVE.0 => {
            let slot = DisplaySlot::from_id(r.var_int()?);
            let name = r.string()?;
            ScoreUpdate::Display {
                slot,
                objective: (!name.is_empty()).then(|| name.into()),
            }
        }
        p if p == cb::SET_SCORE.0 => ScoreUpdate::SetScore {
            holder: r.string()?.into(),
            objective: r.string()?.into(),
            score: Score {
                value: r.var_int()?,
                display: if r.bool()? {
                    Some(r.component()?)
                } else {
                    None
                },
                number_format: optional_number_format(&mut r)?,
            },
        },
        p if p == cb::RESET_SCORE.0 => ScoreUpdate::ResetScore {
            holder: r.string()?.into(),
            objective: if r.bool()? {
                Some(r.string()?.into())
            } else {
                None
            },
        },
        p if p == cb::SET_PLAYER_TEAM.0 => {
            let name = r.string()?.into();
            let action = match r.u8()? {
                0 => TeamAction::Add {
                    look: Box::new(team_look(&mut r)?),
                    players: players(&mut r)?,
                },
                1 => TeamAction::Remove,
                2 => TeamAction::Change(Box::new(team_look(&mut r)?)),
                3 => TeamAction::Join(players(&mut r)?),
                4 => TeamAction::Leave(players(&mut r)?),
                method => anyhow::bail!("Team-Methode {method}"),
            };
            ScoreUpdate::Team { name, action }
        }
        _ => return Ok(None),
    };
    Ok(Some(ScorePacket::Scoreboard(update)))
}

/// `NumberFormatTypes.OPTIONAL_STREAM_CODEC`.
fn optional_number_format(r: &mut Reader) -> Result<Option<NumberFormat>> {
    if !r.bool()? {
        return Ok(None);
    }
    Ok(Some(match r.var_int()? {
        0 => NumberFormat::Blank,
        1 => NumberFormat::Styled(Box::new(r.style()?)),
        2 => NumberFormat::Fixed(Box::new(r.component()?)),
        kind => anyhow::bail!("Zahlenformat {kind}"),
    }))
}

/// `ClientboundSetPlayerTeamPacket.Parameters`.
fn team_look(r: &mut Reader) -> Result<TeamLook> {
    Ok(TeamLook {
        display_name: r.component()?,
        prefix: r.component()?,
        suffix: r.component()?,
        name_tags: Visibility::from_id(r.var_int()?),
        collision: CollisionRule::from_id(r.var_int()?),
        color: if r.bool()? {
            Some(TeamColor::from_id(r.var_int()?))
        } else {
            None
        },
        options: r.u8()?,
    })
}

fn players(r: &mut Reader) -> Result<Vec<Box<str>>> {
    let count = r.list_count()?;
    (0..count).map(|_| Ok(r.string()?.into())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::protocol::reader::write_string;
    use crate::network::protocol::varint::write_var_int;

    #[test]
    fn reads_a_team_with_its_members() {
        let mut body = Vec::new();
        write_string(&mut body, "red").unwrap();
        body.push(0);
        for _ in 0..3 {
            body.extend([8, 0, 0]);
        }
        write_var_int(&mut body, 2).unwrap();
        write_var_int(&mut body, 1).unwrap();
        body.push(1);
        write_var_int(&mut body, 12).unwrap();
        body.push(3);
        write_var_int(&mut body, 1).unwrap();
        write_string(&mut body, "Alex").unwrap();
        let Some(ScorePacket::Scoreboard(ScoreUpdate::Team { name, action })) =
            decode(cb::SET_PLAYER_TEAM.0, &body).unwrap()
        else {
            panic!("not a team");
        };
        assert_eq!(&*name, "red");
        let TeamAction::Add { look, players } = action else {
            panic!("not an add");
        };
        assert_eq!(look.name_tags, Visibility::HideForOtherTeams);
        assert_eq!(look.collision, CollisionRule::Never);
        assert_eq!(look.color, Some(TeamColor::from_id(12)));
        assert!(look.friendly_fire() && look.sees_friendly_invisibles());
        assert_eq!(players, vec![Box::from("Alex")]);
    }

    #[test]
    fn an_empty_name_clears_the_slot() {
        let mut body = Vec::new();
        write_var_int(&mut body, 1).unwrap();
        write_string(&mut body, "").unwrap();
        assert_eq!(
            decode(cb::SET_DISPLAY_OBJECTIVE.0, &body).unwrap(),
            Some(ScorePacket::Scoreboard(ScoreUpdate::Display {
                slot: DisplaySlot::SIDEBAR,
                objective: None,
            }))
        );
    }

    #[test]
    fn reads_a_score_with_a_fixed_format() {
        let mut body = Vec::new();
        write_string(&mut body, "Steve").unwrap();
        write_string(&mut body, "kills").unwrap();
        write_var_int(&mut body, 5).unwrap();
        body.push(0);
        body.push(1);
        write_var_int(&mut body, 2).unwrap();
        body.extend([8, 0, 1, b'x']);
        let Some(ScorePacket::Scoreboard(ScoreUpdate::SetScore { score, .. })) =
            decode(cb::SET_SCORE.0, &body).unwrap()
        else {
            panic!("not a score");
        };
        assert_eq!(score.value, 5);
        assert_eq!(
            score.number_format.map(|f| f.format(5).plain_text()),
            Some("x".to_owned())
        );
    }
}
