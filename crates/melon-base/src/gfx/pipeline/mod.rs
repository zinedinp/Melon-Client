//! Pipeline building shared by the passes, and the GUI's [`ui`] pipeline.

pub mod ui;

use ash::vk;

/// Built pipeline plus its layout.
pub struct Pass {
    pub layout: vk::PipelineLayout,
    pub pipeline: vk::Pipeline,
}

impl Pass {
    /// Destroys the pipeline and its layout.
    ///
    /// # Safety
    ///
    /// `device` must be live and done with the pipeline.
    pub unsafe fn destroy(&self, device: &ash::Device) {
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe {
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_pipeline_layout(self.layout, None);
        }
    }
}

/// The texture set's layout: one combined image sampler at binding 0.
///
/// # Safety
///
/// `device` must be live.
pub unsafe fn create_descriptor_set_layout(
    device: &ash::Device,
) -> anyhow::Result<vk::DescriptorSetLayout> {
    let bindings = [vk::DescriptorSetLayoutBinding::default()
        .binding(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(1)
        .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)];
    // SAFETY: `device` is live, and the infos name only its own live handles.
    Ok(unsafe {
        device.create_descriptor_set_layout(
            &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
            None,
        )
    }?)
}

/// A shader module from SPIR-V bytes.
///
/// # Safety
///
/// `device` must be live; `spirv` must be valid SPIR-V for it.
pub unsafe fn create_shader_module(
    device: &ash::Device,
    spirv: &[u8],
) -> anyhow::Result<vk::ShaderModule> {
    let words = ash::util::read_spv(&mut std::io::Cursor::new(spirv))?;
    // SAFETY: `device` is live, and the infos name only its own live handles.
    Ok(unsafe {
        device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(&words), None)
    }?)
}

/// Viewport and scissor are dynamic in all pipelines (window size).
const DYNAMIC_STATES: [vk::DynamicState; 2] =
    [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];

/// What sets one pipeline apart from the others.
pub struct PipelineDesc<'a> {
    pub vert: &'a [u8],
    pub frag: &'a [u8],
    pub push_constant_size: u32,
    /// The stages that read the push constants.
    pub push_stages: vk::ShaderStageFlags,
    pub vertex_stride: u32,
    /// Format and offset per attribute, at locations `0..`.
    pub attributes: &'a [(vk::Format, u32)],
    /// A second binding read per instance: its stride and its attributes,
    /// at the locations after [`Self::attributes`]'.
    pub instance: Option<(u32, &'a [(vk::Format, u32)])>,
    /// The locations the vertex shader reads, when not all of them.
    pub used_locations: Option<&'a [u32]>,
    pub cull: vk::CullModeFlags,
    pub depth: vk::PipelineDepthStencilStateCreateInfo<'a>,
    pub blend: vk::PipelineColorBlendAttachmentState,
    /// The layouts of the sets after the texture's (set 0): sets 1, 2, ...
    pub extra_sets: &'a [vk::DescriptorSetLayout],
    /// Depth bias, constant and by slope (vanilla's `DepthStencilState`).
    pub depth_bias: Option<(f32, f32)>,
}

impl PipelineDesc<'_> {
    /// The pipeline and its layout.
    ///
    /// # Safety
    ///
    /// `device` must be live, `render_pass` and `descriptor_set_layout` made on it.
    pub unsafe fn build(
        &self,
        device: &ash::Device,
        render_pass: vk::RenderPass,
        descriptor_set_layout: vk::DescriptorSetLayout,
    ) -> anyhow::Result<Pass> {
        let mut set_layouts = vec![descriptor_set_layout];
        set_layouts.extend_from_slice(self.extra_sets);
        let push_constant_ranges = [vk::PushConstantRange::default()
            .stage_flags(self.push_stages)
            .offset(0)
            .size(self.push_constant_size)];
        let ranges: &[vk::PushConstantRange] = if self.push_constant_size == 0 {
            &[]
        } else {
            &push_constant_ranges
        };
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let layout = unsafe {
            device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&set_layouts)
                    .push_constant_ranges(ranges),
                None,
            )
        }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let pipeline = unsafe { self.build_pipeline(device, render_pass, layout) };
        if pipeline.is_err() {
            // SAFETY: made by this device, and no frame in flight uses it any more.
            unsafe { device.destroy_pipeline_layout(layout, None) };
        }
        Ok(Pass {
            layout,
            pipeline: pipeline?,
        })
    }

    unsafe fn build_pipeline(
        &self,
        device: &ash::Device,
        render_pass: vk::RenderPass,
        layout: vk::PipelineLayout,
    ) -> anyhow::Result<vk::Pipeline> {
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let vert_module = unsafe { create_shader_module(device, self.vert) }?;
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let frag_module = match unsafe { create_shader_module(device, self.frag) } {
            Ok(module) => module,
            Err(e) => {
                // SAFETY: made by this device, and no frame in flight uses it any more.
                unsafe { device.destroy_shader_module(vert_module, None) };
                return Err(e);
            }
        };
        let entry_point = c"main";
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vert_module)
                .name(entry_point),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(frag_module)
                .name(entry_point),
        ];
        let mut bindings = Vec::new();
        if !self.attributes.is_empty() {
            bindings.push(
                vk::VertexInputBindingDescription::default()
                    .binding(0)
                    .stride(self.vertex_stride)
                    .input_rate(vk::VertexInputRate::VERTEX),
            );
        }
        if let Some((stride, _)) = self.instance {
            bindings.push(
                vk::VertexInputBindingDescription::default()
                    .binding(1)
                    .stride(stride)
                    .input_rate(vk::VertexInputRate::INSTANCE),
            );
        }
        let mut attr_desc: Vec<_> = self
            .attributes
            .iter()
            .enumerate()
            .filter(|&(location, _)| {
                self.used_locations
                    .is_none_or(|used| used.contains(&(location as u32)))
            })
            .map(|(location, &(format, offset))| {
                vk::VertexInputAttributeDescription::default()
                    .location(location as u32)
                    .binding(0)
                    .format(format)
                    .offset(offset)
            })
            .collect();
        if let Some((_, attributes)) = self.instance {
            let first = self.attributes.len() as u32;
            attr_desc.extend(attributes.iter().zip(first..).map(
                |(&(format, offset), location)| {
                    vk::VertexInputAttributeDescription::default()
                        .location(location)
                        .binding(1)
                        .format(format)
                        .offset(offset)
                },
            ));
        }
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_binding_descriptions(&bindings)
            .vertex_attribute_descriptions(&attr_desc);
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);
        let (bias_constant, bias_slope) = self.depth_bias.unwrap_or_default();
        let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(self.cull)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0)
            .depth_bias_enable(self.depth_bias.is_some())
            .depth_bias_constant_factor(bias_constant)
            .depth_bias_slope_factor(bias_slope);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let attachments = [self.blend];
        let color_blend =
            vk::PipelineColorBlendStateCreateInfo::default().attachments(&attachments);
        let dynamic_state =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&DYNAMIC_STATES);
        let pipeline_info = vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization)
            .multisample_state(&multisample)
            .depth_stencil_state(&self.depth)
            .color_blend_state(&color_blend)
            .dynamic_state(&dynamic_state)
            .layout(layout)
            .render_pass(render_pass)
            .subpass(0);
        // SAFETY: `device` is live, and the infos name only its own live handles.
        let pipeline = unsafe {
            device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
        }
        .map_err(|(_, e)| e);
        // SAFETY: made by this device, and no frame in flight uses it any more.
        unsafe {
            device.destroy_shader_module(vert_module, None);
            device.destroy_shader_module(frag_module, None);
        }
        Ok(pipeline?[0])
    }
}

/// `sky`/`ui` don't test or write depth.
pub fn depth_disabled<'a>() -> vk::PipelineDepthStencilStateCreateInfo<'a> {
    vk::PipelineDepthStencilStateCreateInfo::default()
        .depth_test_enable(false)
        .depth_write_enable(false)
}

/// `data` as push-constant bytes.
pub fn push_constant_bytes(data: &[f32], size: u32) -> &[u8] {
    let bytes = numerobis::elem::as_bytes(data);
    assert_eq!(bytes.len(), size as usize);
    bytes
}
