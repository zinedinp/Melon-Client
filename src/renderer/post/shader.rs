//! A post pass's shaders, compiled for the bindings its pipeline has: the

use std::path::Path;

use super::config::Pass;
use crate::assets::shaders::{self, Interface, Stage};

/// Where the pipeline's three descriptor sets are.
pub const SAMPLER_SET: u32 = 0;
pub const GUI_SET: u32 = 1;
pub const BLOCK_SET: u32 = 2;
/// `SamplerInfo` is the first block of its set.
pub const SAMPLER_INFO_BINDING: u32 = 0;

/// The SPIR-V of a pass's two shaders.
pub struct Compiled {
    pub vertex: Vec<u32>,
    pub fragment: Vec<u32>,
}

/// The names and places a pass offers: `<sampler_name>Sampler` for each
pub fn resources(pass: &Pass) -> Vec<(String, u32, u32)> {
    let mut out = Vec::new();
    for (i, input) in pass.inputs.iter().enumerate() {
        out.push((format!("{}Sampler", input.sampler()), SAMPLER_SET, i as u32));
    }
    for (name, binding) in [("DynamicTransforms", 0), ("Projection", 1), ("Globals", 2)] {
        out.push((name.to_owned(), GUI_SET, binding));
    }
    out.push(("SamplerInfo".to_owned(), BLOCK_SET, SAMPLER_INFO_BINDING));
    for (i, (name, _)) in pass.uniforms.iter().enumerate() {
        out.push((name.clone(), BLOCK_SET, SAMPLER_INFO_BINDING + 1 + i as u32));
    }
    out
}

/// Both of `pass`'s shaders. A shader that can't be found or compiled is
pub fn compile(assets_root: &Path, pass: &Pass) -> anyhow::Result<Compiled> {
    let resources = resources(pass);
    let resources: Vec<(&str, u32, u32)> = resources
        .iter()
        .map(|(name, set, binding)| (name.as_str(), *set, *binding))
        .collect();
    let interface = Interface {
        inputs: &[],
        resources: &resources,
    };
    let one = |id: &str, stage: Stage| -> anyhow::Result<Vec<u32>> {
        let (text, _) = shaders::source(assets_root, id, stage)
            .ok_or_else(|| anyhow::anyhow!("shader {id} not found"))?;
        shaders::compile(assets_root, id, stage, &text, &[], &interface)
    };
    Ok(Compiled {
        vertex: one(&pass.vertex_shader, Stage::Vertex)?,
        fragment: one(&pass.fragment_shader, Stage::Fragment)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::post::config::{Config, ENTITY_OUTLINE, MAIN};

    #[test]
    fn a_pass_offers_its_samplers_then_its_blocks() {
        let config = Config::parse(&serde_json::json!({"passes": [{
            "vertex_shader": "core/screenquad", "fragment_shader": "post/x",
            "inputs": [{"sampler_name": "Data", "target": "minecraft:main"},
                       {"sampler_name": "MainDepth", "target": "minecraft:main",
                        "use_depth_buffer": true}],
            "uniforms": {"BlurConfig": [{"type": "float", "value": 1}]},
            "output": "minecraft:main"}]}))
        .unwrap();
        let offered = resources(&config.passes[0]);
        let find = |name: &str| offered.iter().find(|r| r.0 == name).map(|r| (r.1, r.2));
        assert_eq!(find("DataSampler"), Some((SAMPLER_SET, 0)));
        assert_eq!(find("MainDepthSampler"), Some((SAMPLER_SET, 1)));
        assert_eq!(find("Globals"), Some((GUI_SET, 2)));
        assert_eq!(find("SamplerInfo"), Some((BLOCK_SET, 0)));
        assert_eq!(find("BlurConfig"), Some((BLOCK_SET, 1)));
    }

    /// Every pass of the chain in `MELON_PACK_JSON` compiles, with the
    #[test]
    fn real_chains_shaders_compile() {
        use crate::assets::packs::{self, Pack, ResourceStack};
        let Some(root) = crate::assets::test_root() else {
            eprintln!("no assets (MELON_ASSETS), test skipped");
            return;
        };
        let pack = std::env::var_os("MELON_PACK").map(|path| {
            let pack = Pack::open("test".into(), "test".into(), Path::new(&path)).expect("opens");
            packs::activate(ResourceStack {
                packs: vec![std::sync::Arc::new(pack)],
            });
        });
        let chains = crate::assets::test_post_chains(Some(&root));
        assert!(!chains.is_empty(), "no post_effect/*.json under the assets");
        for (name, json) in chains {
            let config = Config::parse(&json).unwrap_or_else(|e| panic!("{name}: {e:#}"));
            config.validate(&[MAIN, ENTITY_OUTLINE]).unwrap();
            for pass in &config.passes {
                compile(&root, pass)
                    .unwrap_or_else(|e| panic!("{name}, {}: {e:#}", pass.fragment_shader));
            }
        }
        if pack.is_some() {
            packs::activate(ResourceStack::default());
        }
    }
}
