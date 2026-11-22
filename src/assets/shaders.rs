//! Resource pack shaders, compiled the way vanilla 26.3's `GlslCompiler`

use std::collections::HashMap;
use std::path::Path;

use super::packs;

/// The two programmable stages a core shader has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Vertex,
    Fragment,
}

impl Stage {
    /// `ShaderType`'s file extension.
    fn extension(self) -> &'static str {
        match self {
            Stage::Vertex => "vsh",
            Stage::Fragment => "fsh",
        }
    }
}

/// `shaders/core/<name>.<vsh|fsh>` from the resource packs, and whether a pack replaced it.
pub fn core_source(assets_root: &Path, name: &str, stage: Stage) -> Option<(String, bool)> {
    let path = format!("shaders/core/{name}.{}", stage.extension());
    let (bytes, layer) = packs::active().find(assets_root, "minecraft", &path)?;
    let text = String::from_utf8_lossy(packs::meta::strip_bom(&bytes)).into_owned();
    Some((text, layer > 0))
}

/// The shader `id` (`minecraft:post/blit`, `core/screenquad`) for `stage`:
pub fn source(assets_root: &Path, id: &str, stage: Stage) -> Option<(String, bool)> {
    let (namespace, path) = id.split_once(':').unwrap_or(("minecraft", id));
    let file = format!("shaders/{path}.{}", stage.extension());
    let (bytes, layer) = packs::active().find(assets_root, namespace, &file)?;
    let text = String::from_utf8_lossy(packs::meta::strip_bom(&bytes)).into_owned();
    Some((text, layer > 0))
}

/// What a pipeline offers a shader, by the names vanilla gives them.
pub struct Interface<'a> {
    /// Vertex attributes.
    pub inputs: &'a [(&'a str, u32)],
    /// Uniform blocks and samplers: name, descriptor set and binding.
    pub resources: &'a [(&'a str, u32, u32)],
}

/// `source` (named `name` in messages) compiled for `stage` with `defines`
pub fn compile(
    assets_root: &Path,
    name: &str,
    stage: Stage,
    source: &str,
    defines: &[&str],
    interface: &Interface,
) -> anyhow::Result<Vec<u32>> {
    let compiler = shaderc::Compiler::new()?;
    let mut options = shaderc::CompileOptions::new()?;
    options.set_target_env(
        shaderc::TargetEnv::Vulkan,
        shaderc::EnvVersion::Vulkan1_1 as u32,
    );
    options.set_auto_bind_uniforms(true);
    options.set_generate_debug_info();
    options.set_optimization_level(shaderc::OptimizationLevel::Zero);
    options.add_macro_definition("RENDERPEARL_DEPTH_IS_ZERO_TO_ONE", None);
    for define in defines {
        options.add_macro_definition(define, None);
    }
    options.set_include_callback(|requested, _kind, _from, _depth| include(assets_root, requested));
    let kind = match stage {
        Stage::Vertex => shaderc::ShaderKind::Vertex,
        Stage::Fragment => shaderc::ShaderKind::Fragment,
    };
    let file = format!("{name}.{}", stage.extension());
    let artifact = compiler
        .compile_into_spirv(source, kind, &file, "main", Some(&options))
        .map_err(|e| anyhow::anyhow!("{file}: {e}"))?;
    let mut words = artifact.as_binary().to_vec();
    rebind(&mut words, stage, interface).map_err(|e| anyhow::anyhow!("{file}: {e}"))?;
    Ok(words)
}

/// `ShaderManager`'s includes.
fn include(assets_root: &Path, requested: &str) -> shaderc::IncludeCallbackResult {
    let (namespace, path) = match requested.split_once(':') {
        Some((namespace, path)) => (namespace, path),
        None => ("minecraft", requested),
    };
    let file = format!("shaders/include/{path}");
    let (bytes, _) = packs::active()
        .find(assets_root, namespace, &file)
        .ok_or_else(|| format!("{requested}: not found"))?;
    Ok(shaderc::ResolvedInclude {
        resolved_name: format!("{namespace}:{path}"),
        content: String::from_utf8_lossy(packs::meta::strip_bom(&bytes)).into_owned(),
    })
}

const OP_NAME: u32 = 5;
const OP_TYPE_POINTER: u32 = 32;
const OP_VARIABLE: u32 = 59;
const OP_DECORATE: u32 = 71;
const DECORATION_BUILT_IN: u32 = 11;
const DECORATION_LOCATION: u32 = 30;
const DECORATION_BINDING: u32 = 33;
const DECORATION_SET: u32 = 34;
const STORAGE_UNIFORM_CONSTANT: u32 = 0;
const STORAGE_INPUT: u32 = 1;
const STORAGE_UNIFORM: u32 = 2;
const STORAGE_PUSH_CONSTANT: u32 = 9;
const STORAGE_BUFFER: u32 = 12;

/// Gives every uniform, sampler and (for a vertex shader) input the set, binding or location
/// Melon's pipelines use.
pub fn rebind(words: &mut [u32], stage: Stage, interface: &Interface) -> anyhow::Result<()> {
    anyhow::ensure!(words.len() > 5 && words[0] == 0x0723_0203, "not SPIR-V");
    let mut names: HashMap<u32, String> = HashMap::new();
    let mut pointees: HashMap<u32, u32> = HashMap::new();
    let mut decorations: HashMap<(u32, u32), usize> = HashMap::new();
    let mut variables: Vec<(u32, u32, u32)> = Vec::new();
    let mut at = 5;
    while at < words.len() {
        let count = (words[at] >> 16) as usize;
        let opcode = words[at] & 0xffff;
        anyhow::ensure!(count > 0 && at + count <= words.len(), "SPIR-V cut short");
        let operands = &words[at + 1..at + count];
        match opcode {
            OP_NAME if !operands.is_empty() => {
                names.insert(operands[0], literal_string(&operands[1..]));
            }
            OP_TYPE_POINTER if operands.len() >= 3 => {
                pointees.insert(operands[0], operands[2]);
            }
            OP_VARIABLE if operands.len() >= 3 => {
                variables.push((operands[1], operands[0], operands[2]));
            }
            OP_DECORATE if operands.len() >= 2 => {
                decorations.insert((operands[0], operands[1]), at + 3);
            }
            _ => {}
        }
        at += count;
    }
    for (id, pointer_type, storage) in variables {
        let name = match names.get(&id).filter(|n| !n.is_empty()) {
            Some(name) => name.clone(),
            None => pointees
                .get(&pointer_type)
                .and_then(|block| names.get(block))
                .cloned()
                .unwrap_or_default(),
        };
        match storage {
            STORAGE_INPUT if stage == Stage::Vertex => {
                if decorations.contains_key(&(id, DECORATION_BUILT_IN)) {
                    continue;
                }
                let &(_, location) = interface
                    .inputs
                    .iter()
                    .find(|(input, _)| *input == name)
                    .ok_or_else(|| anyhow::anyhow!("no vertex element {name:?}"))?;
                let slot = decorations
                    .get(&(id, DECORATION_LOCATION))
                    .ok_or_else(|| anyhow::anyhow!("input {name:?} has no location"))?;
                words[*slot] = location;
            }
            STORAGE_UNIFORM | STORAGE_UNIFORM_CONSTANT | STORAGE_BUFFER => {
                let &(_, set, binding) = interface
                    .resources
                    .iter()
                    .find(|(resource, _, _)| *resource == name)
                    .ok_or_else(|| anyhow::anyhow!("unknown uniform {name:?}"))?;
                let set_slot = decorations.get(&(id, DECORATION_SET));
                let binding_slot = decorations.get(&(id, DECORATION_BINDING));
                let (Some(&set_slot), Some(&binding_slot)) = (set_slot, binding_slot) else {
                    anyhow::bail!("uniform {name:?} has no binding");
                };
                words[set_slot] = set;
                words[binding_slot] = binding;
            }
            STORAGE_PUSH_CONSTANT => anyhow::bail!("push constants are not offered"),
            _ => {}
        }
    }
    Ok(())
}

/// The locations of a vertex shader's inputs (its built-ins left out).
pub fn input_locations(words: &[u32]) -> Vec<u32> {
    let mut inputs = Vec::new();
    let mut locations = HashMap::new();
    let mut built_in = std::collections::HashSet::new();
    let mut at = 5;
    while at < words.len() {
        let count = (words[at] >> 16) as usize;
        if count == 0 || at + count > words.len() {
            break;
        }
        let operands = &words[at + 1..at + count];
        match words[at] & 0xffff {
            OP_VARIABLE if operands.len() >= 3 && operands[2] == STORAGE_INPUT => {
                inputs.push(operands[1]);
            }
            OP_DECORATE if operands.len() >= 3 && operands[1] == DECORATION_LOCATION => {
                locations.insert(operands[0], operands[2]);
            }
            OP_DECORATE if operands.len() >= 2 && operands[1] == DECORATION_BUILT_IN => {
                built_in.insert(operands[0]);
            }
            _ => {}
        }
        at += count;
    }
    let mut out: Vec<u32> = inputs
        .iter()
        .filter(|id| !built_in.contains(id))
        .filter_map(|id| locations.get(id).copied())
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// A SPIR-V literal string: UTF-8, four bytes to a word, NUL-terminated.
fn literal_string(words: &[u32]) -> String {
    let bytes: Vec<u8> = words
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .take_while(|&b| b != 0)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERFACE: Interface = Interface {
        inputs: &[("Position", 0), ("UV0", 1), ("Color", 2)],
        resources: &[
            ("Sampler0", 0, 0),
            ("DynamicTransforms", 1, 0),
            ("Projection", 1, 1),
            ("Globals", 1, 2),
        ],
    };

    /// The location or binding decoration of the variable or block named `name`.
    fn decoration(words: &[u32], name: &str, decoration: u32) -> Option<u32> {
        let mut ids = Vec::new();
        let mut at = 5;
        while at < words.len() {
            let count = (words[at] >> 16) as usize;
            if words[at] & 0xffff == OP_NAME && literal_string(&words[at + 2..at + count]) == name {
                ids.push(words[at + 1]);
            }
            at += count;
        }
        let mut at = 5;
        let mut pointers = Vec::new();
        while at < words.len() {
            let count = (words[at] >> 16) as usize;
            let op = words[at] & 0xffff;
            if op == OP_TYPE_POINTER && ids.contains(&words[at + 3]) {
                pointers.push(words[at + 1]);
            }
            if op == OP_VARIABLE && pointers.contains(&words[at + 1]) {
                ids.push(words[at + 2]);
            }
            at += count;
        }
        let mut at = 5;
        while at < words.len() {
            let count = (words[at] >> 16) as usize;
            if words[at] & 0xffff == OP_DECORATE
                && ids.contains(&words[at + 1])
                && words[at + 2] == decoration
            {
                return Some(words[at + 3]);
            }
            at += count;
        }
        None
    }

    fn compile_source(source: &str, stage: Stage) -> anyhow::Result<Vec<u32>> {
        compile(
            Path::new("/nonexistent"),
            "test",
            stage,
            source,
            &[],
            &INTERFACE,
        )
    }

    #[test]
    fn a_vanilla_style_shader_gets_melons_bindings() {
        let vert = "#version 330
            #extension GL_ARB_separate_shader_objects : require
            layout(std140) uniform Projection { mat4 ProjMat; };
            layout(std140) uniform DynamicTransforms {
                mat4 ModelViewMat; mat4 TextureMat; vec4 ColorModulator; vec3 ModelOffset;
            };
            layout(location = 3) in vec2 UV0;
            layout(location = 4) in vec4 Color;
            layout(location = 5) in vec3 Position;
            layout(location = 0) out vec2 texCoord0;
            layout(location = 1) out vec4 vertexColor;
            layout(location = 2) out vec3 corner;
            void main() {
                gl_Position = ProjMat * ModelViewMat * vec4(Position, 1.0);
                texCoord0 = UV0;
                vertexColor = Color;
                corner = vec3(float(gl_VertexIndex % 4), 0.0, 1.0);
            }";
        let words = compile_source(vert, Stage::Vertex).expect("compiles");
        assert_eq!(decoration(&words, "Position", DECORATION_LOCATION), Some(0));
        assert_eq!(decoration(&words, "UV0", DECORATION_LOCATION), Some(1));
        assert_eq!(decoration(&words, "Color", DECORATION_LOCATION), Some(2));
        assert_eq!(
            decoration(&words, "Projection", DECORATION_BINDING),
            Some(1)
        );
        assert_eq!(decoration(&words, "Projection", DECORATION_SET), Some(1));
        assert_eq!(
            decoration(&words, "DynamicTransforms", DECORATION_BINDING),
            Some(0)
        );

        let frag = "#version 330
            #extension GL_ARB_separate_shader_objects : require
            uniform sampler2D Sampler0;
            layout(location = 0) in vec2 texCoord0;
            layout(location = 0) out vec4 fragColor;
            void main() { fragColor = texture(Sampler0, texCoord0); }";
        let words = compile_source(frag, Stage::Fragment).expect("compiles");
        assert_eq!(decoration(&words, "Sampler0", DECORATION_BINDING), Some(0));
        assert_eq!(decoration(&words, "Sampler0", DECORATION_SET), Some(0));
    }

    #[test]
    fn unknown_uniforms_and_inputs_are_refused() {
        let frag = "#version 330
            uniform sampler2D Sampler7;
            layout(location = 0) out vec4 fragColor;
            void main() { fragColor = texture(Sampler7, vec2(0.0)); }";
        let error = compile_source(frag, Stage::Fragment).unwrap_err();
        assert!(error.to_string().contains("Sampler7"), "{error}");
        let vert = "#version 330
            layout(location = 0) in vec3 Normal;
            void main() { gl_Position = vec4(Normal, 1.0); }";
        let error = compile_source(vert, Stage::Vertex).unwrap_err();
        assert!(error.to_string().contains("Normal"), "{error}");
    }
}
