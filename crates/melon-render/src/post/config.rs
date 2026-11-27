//! `PostChainConfig`.

use std::collections::HashSet;

use serde_json::Value;

use super::uniform::UniformValue;

/// `minecraft:main`: the screen's colour and depth.
pub const MAIN: &str = "minecraft:main";
/// `minecraft:entity_outline`: what glowing entities draw into.
pub const ENTITY_OUTLINE: &str = "minecraft:entity_outline";

/// `InternalTarget`.
#[derive(Clone, Debug, PartialEq)]
pub struct InternalTarget {
    pub name: String,
    /// The screen's size where left out.
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Kept between frames, made again only when the size changes.
    pub persistent: bool,
    /// Red, green, blue, alpha it is cleared to.
    pub clear: [f32; 4],
}

/// `Input`: what a pass samples.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    /// A target's colour, or its depth with `depth`.
    Target {
        sampler: String,
        target: String,
        depth: bool,
        bilinear: bool,
    },
    /// `textures/effect/<location>.png`.
    Texture {
        sampler: String,
        location: String,
        width: u32,
        height: u32,
        bilinear: bool,
    },
}

impl Input {
    pub fn sampler(&self) -> &str {
        match self {
            Self::Target { sampler, .. } | Self::Texture { sampler, .. } => sampler,
        }
    }

    pub fn bilinear(&self) -> bool {
        match self {
            Self::Target { bilinear, .. } | Self::Texture { bilinear, .. } => *bilinear,
        }
    }
}

/// `Pass`.
#[derive(Clone, Debug, PartialEq)]
pub struct Pass {
    pub vertex_shader: String,
    pub fragment_shader: String,
    pub inputs: Vec<Input>,
    pub output: String,
    /// Uniform blocks by name, their members in the order listed.
    pub uniforms: Vec<(String, Vec<UniformValue>)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Config {
    pub targets: Vec<InternalTarget>,
    pub passes: Vec<Pass>,
}

/// `namespace:path`, with `minecraft` where the namespace is left out.
pub fn identifier(id: &str) -> String {
    if id.contains(':') {
        id.to_owned()
    } else {
        format!("minecraft:{id}")
    }
}

impl Config {
    /// `PostChainConfig.CODEC`.
    pub fn parse(json: &Value) -> Result<Self, String> {
        let mut config = Self::default();
        if let Some(targets) = json.get("targets") {
            let targets = targets.as_object().ok_or("targets is not an object")?;
            for (name, target) in targets {
                config.targets.push(internal_target(name, target)?);
            }
        }
        if let Some(passes) = json.get("passes") {
            let passes = passes.as_array().ok_or("passes is not a list")?;
            for (i, pass) in passes.iter().enumerate() {
                config
                    .passes
                    .push(parse_pass(pass).map_err(|e| format!("pass {i}: {e}"))?);
            }
        }
        Ok(config)
    }

    /// `PostChain.load`'s check.
    pub fn validate(&self, allowed: &[&str]) -> Result<(), String> {
        let own: HashSet<&str> = self.targets.iter().map(|t| t.name.as_str()).collect();
        let known = |target: &str| own.contains(target) || allowed.contains(&target);
        for pass in &self.passes {
            if !known(&pass.output) {
                return Err(format!("unknown target {}", pass.output));
            }
            for input in &pass.inputs {
                if let Input::Target { target, .. } = input
                    && !known(target)
                {
                    return Err(format!("unknown target {target}"));
                }
            }
        }
        Ok(())
    }

    /// The targets outside the chain the passes use.
    pub fn external_targets(&self) -> HashSet<&str> {
        let own: HashSet<&str> = self.targets.iter().map(|t| t.name.as_str()).collect();
        let mut out = HashSet::new();
        for pass in &self.passes {
            if !own.contains(pass.output.as_str()) {
                out.insert(pass.output.as_str());
            }
            for input in &pass.inputs {
                if let Input::Target { target, .. } = input
                    && !own.contains(target.as_str())
                {
                    out.insert(target.as_str());
                }
            }
        }
        out
    }
}

fn internal_target(name: &str, json: &Value) -> Result<InternalTarget, String> {
    let size = |key: &str| -> Result<Option<u32>, String> {
        match json.get(key) {
            None => Ok(None),
            Some(v) => v
                .as_u64()
                .filter(|&n| n > 0 && n <= u64::from(u32::MAX))
                .map(|n| Some(n as u32))
                .ok_or_else(|| format!("{name}: {key} must be positive")),
        }
    };
    Ok(InternalTarget {
        name: identifier(name),
        width: size("width")?,
        height: size("height")?,
        persistent: json
            .get("persistent")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        clear: json
            .get("clear_color")
            .map_or(Ok([0.0; 4]), argb)
            .map_err(|e| format!("{name}: clear_color {e}"))?,
    })
}

/// `ARGB_COLOR_CODEC`: a packed int, or red, green, blue and alpha floats.
fn argb(value: &Value) -> Result<[f32; 4], String> {
    if let Some(list) = value.as_array() {
        let floats: Vec<f32> = list
            .iter()
            .filter_map(|n| n.as_f64())
            .map(|n| n as f32)
            .collect();
        return <[f32; 4]>::try_from(floats).map_err(|_| "needs 4 numbers".to_owned());
    }
    let packed = value
        .as_i64()
        .map(|n| n as u32)
        .or_else(|| value.as_u64().map(|n| n as u32))
        .ok_or("is not a colour")?;
    let channel = |shift: u32| ((packed >> shift) & 255) as f32 / 255.0;
    Ok([channel(16), channel(8), channel(0), channel(24)])
}

fn parse_pass(json: &Value) -> Result<Pass, String> {
    let text = |key: &str| {
        json.get(key)
            .and_then(Value::as_str)
            .map(identifier)
            .ok_or_else(|| format!("{key} is missing"))
    };
    let mut inputs = Vec::new();
    if let Some(list) = json.get("inputs") {
        for input in list.as_array().ok_or("inputs is not a list")? {
            inputs.push(parse_input(input)?);
        }
    }
    let mut seen = HashSet::new();
    for input in &inputs {
        if !seen.insert(input.sampler()) {
            return Err(format!("repeated sampler name {}", input.sampler()));
        }
    }
    let mut uniforms = Vec::new();
    if let Some(blocks) = json.get("uniforms") {
        for (name, members) in blocks.as_object().ok_or("uniforms is not an object")? {
            let members = members
                .as_array()
                .ok_or_else(|| format!("uniform block {name} is not a list"))?;
            let values = members
                .iter()
                .map(UniformValue::parse)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("uniform block {name}: {e}"))?;
            uniforms.push((name.clone(), values));
        }
    }
    Ok(Pass {
        vertex_shader: text("vertex_shader")?,
        fragment_shader: text("fragment_shader")?,
        inputs,
        output: text("output")?,
        uniforms,
    })
}

/// `Codec.xor(TextureInput, TargetInput)`: exactly one of the two fits.
fn parse_input(json: &Value) -> Result<Input, String> {
    let sampler = json
        .get("sampler_name")
        .and_then(Value::as_str)
        .ok_or("input without sampler_name")?
        .to_owned();
    let flag = |key: &str| json.get(key).and_then(Value::as_bool).unwrap_or(false);
    let size = |key: &str| {
        json.get(key)
            .and_then(Value::as_u64)
            .filter(|&n| n > 0 && n <= u64::from(u32::MAX))
            .map(|n| n as u32)
    };
    let target = json.get("target").and_then(Value::as_str);
    let location = json.get("location").and_then(Value::as_str);
    let texture = location.zip(size("width")).zip(size("height"));
    match (texture, target) {
        (Some(((location, width), height)), None) => Ok(Input::Texture {
            sampler,
            location: identifier(location),
            width,
            height,
            bilinear: flag("bilinear"),
        }),
        (None, Some(target)) => Ok(Input::Target {
            sampler,
            target: identifier(target),
            depth: flag("use_depth_buffer"),
            bilinear: flag("bilinear"),
        }),
        _ => Err(format!("input {sampler} is neither a target nor a texture")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Vanilla 26.3's `entity_outline.json`.
    fn vanilla() -> Value {
        let blur = |dir: [f32; 2], input: &str, output: &str| {
            json!({
                "vertex_shader": "minecraft:core/screenquad",
                "fragment_shader": "minecraft:post/entity_outline_box_blur",
                "inputs": [{"sampler_name": "In", "target": input, "bilinear": true}],
                "output": output,
                "uniforms": {"BlurConfig": [
                    {"name": "BlurDir", "type": "vec2", "value": dir},
                    {"name": "Radius", "type": "float", "value": 2.0}
                ]}
            })
        };
        json!({
            "targets": {"swap": {}},
            "passes": [
                {"vertex_shader": "minecraft:core/screenquad",
                 "fragment_shader": "minecraft:post/entity_sobel",
                 "inputs": [{"sampler_name": "In", "target": "minecraft:entity_outline"}],
                 "output": "swap"},
                blur([1.0, 0.0], "swap", "minecraft:entity_outline"),
                blur([0.0, 1.0], "minecraft:entity_outline", "swap"),
                {"vertex_shader": "minecraft:core/screenquad",
                 "fragment_shader": "minecraft:post/blit",
                 "inputs": [{"sampler_name": "In", "target": "swap"}],
                 "uniforms": {"BlitConfig": [
                     {"name": "ColorModulate", "type": "vec4", "value": [1, 1, 1, 1]}]},
                 "output": "minecraft:entity_outline"}
            ]
        })
    }

    #[test]
    fn vanillas_outline_chain_parses_and_validates() {
        let config = Config::parse(&vanilla()).unwrap();
        assert_eq!(config.targets.len(), 1);
        assert_eq!(config.targets[0].name, "minecraft:swap");
        assert_eq!(config.targets[0].clear, [0.0; 4]);
        assert!(!config.targets[0].persistent);
        assert_eq!(config.passes.len(), 4);
        assert_eq!(config.passes[1].uniforms[0].0, "BlurConfig");
        assert_eq!(
            config.passes[1].uniforms[0].1,
            [UniformValue::Vec2([1.0, 0.0]), UniformValue::Float(2.0)]
        );
        assert!(matches!(
            config.passes[1].inputs[0],
            Input::Target {
                bilinear: true,
                depth: false,
                ..
            }
        ));
        assert_eq!(
            config.external_targets(),
            HashSet::from(["minecraft:entity_outline"])
        );
        config.validate(&[MAIN, ENTITY_OUTLINE]).unwrap();
        assert!(config.validate(&[MAIN]).is_err());
    }

    #[test]
    fn inputs_are_a_target_or_a_texture_never_both_or_neither() {
        let both = json!({"sampler_name": "A", "target": "main", "location": "x",
            "width": 4, "height": 4});
        assert!(parse_input(&both).is_err());
        assert!(parse_input(&json!({"sampler_name": "A"})).is_err());
        let texture = json!({"sampler_name": "A", "location": "lumiere/portal/atlas",
            "width": 512, "height": 512, "bilinear": false});
        assert_eq!(
            parse_input(&texture).unwrap(),
            Input::Texture {
                sampler: "A".into(),
                location: "minecraft:lumiere/portal/atlas".into(),
                width: 512,
                height: 512,
                bilinear: false
            }
        );
        assert!(parse_input(&json!({"sampler_name": "A", "location": "x"})).is_err());
        let depth = parse_input(
            &json!({"sampler_name": "MainDepth", "target": "minecraft:main",
                "use_depth_buffer": true}),
        )
        .unwrap();
        assert!(matches!(depth, Input::Target { depth: true, .. }));
    }

    #[test]
    fn a_pass_with_a_repeated_sampler_or_no_shader_is_refused() {
        let mut chain = vanilla();
        chain["passes"][0]["inputs"] = json!([
            {"sampler_name": "In", "target": "swap"},
            {"sampler_name": "In", "target": "swap"}
        ]);
        assert!(Config::parse(&chain).unwrap_err().contains("repeated"));
        let mut chain = vanilla();
        chain["passes"][2]
            .as_object_mut()
            .unwrap()
            .remove("fragment_shader");
        assert!(Config::parse(&chain).is_err());
    }

    #[test]
    fn target_sizes_colours_and_persistence() {
        let config = Config::parse(&json!({"targets": {
            "table": {"width": 128, "height": 14, "persistent": true},
            "tint": {"clear_color": -16777216},
            "float": {"clear_color": [1.0, 0.5, 0.0, 1.0]}
        }}))
        .unwrap();
        let by = |name: &str| {
            config
                .targets
                .iter()
                .find(|t| t.name.ends_with(name))
                .unwrap()
        };
        assert_eq!(
            (by("table").width, by("table").height),
            (Some(128), Some(14))
        );
        assert!(by("table").persistent);
        assert_eq!(by("tint").clear, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(by("float").clear, [1.0, 0.5, 0.0, 1.0]);
        assert!(Config::parse(&json!({"targets": {"t": {"width": 0}}})).is_err());
    }

    /// Vanilla's chains (or the one in `MELON_PACK_JSON`) parse.
    #[test]
    fn real_chains_parse() {
        let chains = melon_assets::assets::test_post_chains(melon_assets::assets::test_root().as_deref());
        if chains.is_empty() {
            eprintln!("no assets and no MELON_PACK_JSON, test skipped");
            return;
        }
        for (name, json) in chains {
            let config = Config::parse(&json).unwrap_or_else(|e| panic!("{name}: {e:#}"));
            config
                .validate(&[MAIN, ENTITY_OUTLINE])
                .unwrap_or_else(|e| panic!("{name}: {e:#}"));
            assert!(!config.passes.is_empty(), "{name}");
        }
    }
}
