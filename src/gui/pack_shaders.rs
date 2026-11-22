//! Resource packs' replacements for vanilla's GUI shaders.

use std::path::Path;

use crate::assets::shaders::{self, Interface, Stage};
use crate::renderer::GuiShader;
use crate::renderer::quad::Program;

/// One GUI pipeline as vanilla declares it.
struct Pipeline {
    program: Program,
    /// `shaders/core/<shader>.vsh` and `.fsh`.
    shader: &'static str,
    defines: &'static [&'static str],
    interface: Interface<'static>,
}

/// `DefaultVertexFormat.POSITION_TEX_COLOR`'s elements, at the locations Melon's pipelines use.
const POSITION_TEX_COLOR: &[(&str, u32)] = &[("Position", 0), ("UV0", 1), ("Color", 2)];
/// `DefaultVertexFormat.POSITION_COLOR`.
const POSITION_COLOR: &[(&str, u32)] = &[("Position", 0), ("Color", 2)];
/// The texture's set, then the uniform blocks' (`renderer::gui_uniforms`).
const TEXTURED: &[(&str, u32, u32)] = &[
    ("Sampler0", 0, 0),
    ("DynamicTransforms", 1, 0),
    ("Projection", 1, 1),
    ("Globals", 1, 2),
];
const UNTEXTURED: &[(&str, u32, u32)] = &[
    ("DynamicTransforms", 1, 0),
    ("Projection", 1, 1),
    ("Globals", 1, 2),
];

const PIPELINES: [Pipeline; 3] = [
    Pipeline {
        program: Program::Sprite,
        shader: "position_tex_color",
        defines: &[],
        interface: Interface {
            inputs: POSITION_TEX_COLOR,
            resources: TEXTURED,
        },
    },
    Pipeline {
        program: Program::Fill,
        shader: "gui",
        defines: &[],
        interface: Interface {
            inputs: POSITION_COLOR,
            resources: UNTEXTURED,
        },
    },
    Pipeline {
        program: Program::Text,
        shader: "text",
        defines: &["IS_GUI"],
        interface: Interface {
            inputs: POSITION_TEX_COLOR,
            resources: TEXTURED,
        },
    },
];

/// The GUI shaders the active resource packs replace, compiled.
pub fn compile(assets_root: &Path) -> Vec<GuiShader> {
    PIPELINES
        .iter()
        .filter_map(|pipeline| match compile_one(assets_root, pipeline) {
            Ok(shader) => shader,
            Err(e) => {
                crate::warn!(
                    "Shader core/{} from a resource pack dropped: {e:#}",
                    pipeline.shader
                );
                None
            }
        })
        .collect()
}

/// Every GUI pipeline's shaders, replaced or not.
#[cfg(test)]
pub(crate) fn compile_all(assets_root: &Path) -> anyhow::Result<Vec<GuiShader>> {
    PIPELINES
        .iter()
        .map(|pipeline| {
            let source = |stage| {
                shaders::core_source(assets_root, pipeline.shader, stage)
                    .map(|(text, _)| text)
                    .ok_or_else(|| anyhow::anyhow!("core/{} fehlt", pipeline.shader))
            };
            let compile = |stage| -> anyhow::Result<Vec<u32>> {
                shaders::compile(
                    assets_root,
                    pipeline.shader,
                    stage,
                    &source(stage)?,
                    pipeline.defines,
                    &pipeline.interface,
                )
            };
            let vert = compile(Stage::Vertex)?;
            Ok(GuiShader {
                program: pipeline.program,
                inputs: shaders::input_locations(&vert),
                vert,
                frag: compile(Stage::Fragment)?,
            })
        })
        .collect()
}

/// `None` when no pack replaces the pipeline's shaders.
fn compile_one(assets_root: &Path, pipeline: &Pipeline) -> anyhow::Result<Option<GuiShader>> {
    let vert = shaders::core_source(assets_root, pipeline.shader, Stage::Vertex);
    let frag = shaders::core_source(assets_root, pipeline.shader, Stage::Fragment);
    let from_pack =
        |source: &Option<(String, bool)>| source.as_ref().is_some_and(|(_, pack)| *pack);
    if !from_pack(&vert) && !from_pack(&frag) {
        return Ok(None);
    }
    let missing = |stage: &str| {
        anyhow::anyhow!(
            "{stage} fehlt (auch in assets/minecraft/shaders: Launcher: Prepare ausfuehren)"
        )
    };
    let (vert, _) = vert.ok_or_else(|| missing("der Vertex-Shader"))?;
    let (frag, _) = frag.ok_or_else(|| missing("der Fragment-Shader"))?;
    let compile = |stage, source: &str| {
        shaders::compile(
            assets_root,
            &format!("core/{}", pipeline.shader),
            stage,
            source,
            pipeline.defines,
            &pipeline.interface,
        )
    };
    let vert = compile(Stage::Vertex, &vert)?;
    Ok(Some(GuiShader {
        program: pipeline.program,
        inputs: shaders::input_locations(&vert),
        vert,
        frag: compile(Stage::Fragment, &frag)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vanilla's three GUI shaders, if `assets/` has them, compile against Melon's pipelines.
    #[test]
    fn vanilla_gui_shaders_fit_their_pipelines() {
        let root = crate::assets::default_root();
        if !root.join("minecraft/shaders/core/text.vsh").is_file() {
            eprintln!("assets/minecraft/shaders fehlt, Test uebersprungen");
            return;
        }
        compile_all(&root).unwrap_or_else(|e| panic!("{e:#}"));
    }

    /// A real pack's GUI shaders (`MELON_PACK`, a zip or folder) compile.
    #[test]
    fn a_real_packs_gui_shaders_compile() {
        use crate::assets::packs::{self, Pack, ResourceStack};
        let (Some(root), Some(path)) = (crate::assets::test_root(), std::env::var_os("MELON_PACK"))
        else {
            eprintln!("no pack in MELON_PACK (or no assets), test skipped");
            return;
        };
        let pack = Pack::open("test".into(), "test".into(), Path::new(&path)).expect("opens");
        packs::activate(ResourceStack {
            packs: vec![std::sync::Arc::new(pack)],
        });
        for pipeline in &PIPELINES {
            match compile_one(&root, pipeline) {
                Ok(Some(_)) => eprintln!("{}: compiled", pipeline.shader),
                Ok(None) => eprintln!("{}: not replaced", pipeline.shader),
                Err(e) => panic!("{}: {e:#}", pipeline.shader),
            }
        }
        packs::activate(ResourceStack::default());
    }
}
