//! Reading the chains a game uses from the resource packs.

use std::collections::HashMap;
use std::path::Path;

use super::config::{Config, ENTITY_OUTLINE, Input, MAIN, Pass};
use super::gpu::ChainSource;
use super::shader::{self, Compiled};

/// The chain `GameRenderer` asks for every frame with a player.
pub const END_OF_FRAME: &str = "minecraft:end_of_frame";

/// Every chain the packs have (`post_effect/*.json` in any namespace) that can be used, and
/// the outline blit's shaders. Servers name the ones to run (`ClientboundPostEffectsPacket`),
/// so none is left out.
pub fn load(assets_root: &Path) -> (Vec<ChainSource>, Option<Compiled>) {
    let stack = melon_assets::assets::packs::active();
    let mut chains = Vec::new();
    for namespace in stack.namespaces(assets_root) {
        for path in stack.list(assets_root, &namespace, "post_effect") {
            let Some(name) = path
                .strip_prefix("post_effect/")
                .and_then(|p| p.strip_suffix(".json"))
            else {
                continue;
            };
            let id = format!("{namespace}:{name}");
            match load_one(assets_root, &id) {
                Ok(Some(chain)) => chains.push(chain),
                Ok(None) => {}
                Err(e) => melon_base::warn!("Post effect {id}: {e:#}"),
            }
        }
    }
    (chains, blit(assets_root))
}

fn load_one(assets_root: &Path, id: &str) -> anyhow::Result<Option<ChainSource>> {
    let Some(bytes) = melon_assets::assets::resource(assets_root, "post_effect", id, "json") else {
        return Ok(None);
    };
    let json: serde_json::Value =
        serde_json::from_slice(melon_assets::assets::packs::meta::strip_bom(&bytes))?;
    let config = Config::parse(&json).map_err(|e| anyhow::anyhow!("{e}"))?;
    let allowed: &[&str] = if id == ENTITY_OUTLINE_CHAIN {
        &[MAIN, ENTITY_OUTLINE]
    } else {
        &[MAIN]
    };
    config
        .validate(allowed)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let mut shaders = Vec::new();
    let mut textures = HashMap::new();
    for pass in &config.passes {
        shaders.push(shader::compile(assets_root, pass)?);
        for input in &pass.inputs {
            if let Input::Texture { location, .. } = input
                && !textures.contains_key(location)
            {
                let path = location.replacen(':', ":effect/", 1);
                match melon_assets::assets::texture_image(assets_root, &path) {
                    Some(image) => {
                        textures.insert(location.clone(), image);
                    }
                    None => melon_base::warn!("Post effect {id}: texture {location} missing"),
                }
            }
        }
    }
    Ok(Some(ChainSource {
        id: id.to_owned(),
        config,
        shaders,
        textures,
    }))
}

const ENTITY_OUTLINE_CHAIN: &str = "minecraft:entity_outline";

/// `ENTITY_OUTLINE_BLIT`'s shaders: `core/screenquad` and `core/blit_screen`.
fn blit(assets_root: &Path) -> Option<Compiled> {
    let pass = Pass {
        vertex_shader: "minecraft:core/screenquad".to_owned(),
        fragment_shader: "minecraft:core/blit_screen".to_owned(),
        inputs: vec![Input::Target {
            sampler: "In".to_owned(),
            target: ENTITY_OUTLINE.to_owned(),
            depth: false,
            bilinear: false,
        }],
        output: MAIN.to_owned(),
        uniforms: Vec::new(),
    };
    match shader::compile(assets_root, &pass) {
        Ok(compiled) => Some(compiled),
        Err(e) => {
            melon_base::warn!("Outline blit: {e:#}");
            None
        }
    }
}
