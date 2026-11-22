//! A biome's ambient particles (`minecraft:visual/ambient_particles`):

use serde_json::Value;

use super::super::Kind;

/// The particles of a biome's `attributes`, as `(type, chance)`.
pub fn from_attributes(attributes: Option<&Value>) -> Vec<(Kind, f32)> {
    let Some(entry) = attributes.and_then(|a| a.get("minecraft:visual/ambient_particles")) else {
        return Vec::new();
    };
    let list = entry.get("argument").unwrap_or(entry);
    list.as_array()
        .into_iter()
        .flatten()
        .filter_map(|particle| {
            let options = particle.get("particle")?;
            let name = options
                .get("type")
                .and_then(Value::as_str)
                .or(options.as_str())?;
            let kind = Kind::from_name(name.strip_prefix("minecraft:").unwrap_or(name))?;
            if super::super::options::has_data(kind) {
                return None;
            }
            let chance = particle.get("probability")?.as_f64()? as f32;
            Some((kind, chance))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_basalt_deltas_drop_white_ash() {
        let attributes = serde_json::json!({
            "minecraft:visual/ambient_particles": {
                "argument": [{"particle": {"type": "minecraft:white_ash"}, "probability": 0.118}],
                "modifier": "append"
            }
        });
        let particles = from_attributes(Some(&attributes));
        assert_eq!(particles, vec![(Kind::WhiteAsh, 0.118)]);
        assert!(from_attributes(None).is_empty());
    }
}
