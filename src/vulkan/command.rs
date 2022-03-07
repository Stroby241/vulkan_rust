use super::{VulkanApp, swapchain::SwapchainProperties, device::*, FRAMES_IN_FLIGHT};

use ash::{vk::{self, Image, RenderPass, Framebuffer, CommandBuffer}, Device, };
use imgui::DrawData;
use imgui_rs_vulkan_renderer::Renderer;

impl VulkanApp{

    pub fn create_command_pool(
        device: &Device,
        queue_families_indices: QueueFamiliesIndices,
        create_flags: vk::CommandPoolCreateFlags,
    ) -> vk::CommandPool {
        let command_pool_info = vk::CommandPoolCreateInfo::builder()
            .queue_family_index(queue_families_indices.graphics_index)
            .flags(create_flags)
            .build();

        unsafe {
            device
                .create_command_pool(&command_pool_info, None)
                .unwrap()
        }
    }

    /// Create a one time use command buffer and pass it to `executor`.
    pub fn execute_one_time_commands<F: FnOnce(vk::CommandBuffer)>(
        device: &Device,
        command_pool: &vk::CommandPool,
        queue: &vk::Queue,
        executor: F,
    ) {
        let command_buffer = {
            let alloc_info = vk::CommandBufferAllocateInfo::builder()
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_pool(command_pool.clone())
                .command_buffer_count(1)
                .build();

            unsafe { device.allocate_command_buffers(&alloc_info).unwrap()[0] }
        };
        let command_buffers = [command_buffer];

        // Begin recording
        {
            let begin_info = vk::CommandBufferBeginInfo::builder()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
                .build();
            unsafe {
                device
                    .begin_command_buffer(command_buffer, &begin_info)
                    .unwrap()
            };
        }

        // Execute user function
        executor(command_buffer);

        // End recording
        unsafe { device.end_command_buffer(command_buffer).unwrap() };

        // Submit and wait
        {
            let submit_info = vk::SubmitInfo::builder()
                .command_buffers(&command_buffers)
                .build();
            let submit_infos = [submit_info];
            unsafe {
                device
                    .queue_submit(queue.clone(), &submit_infos, vk::Fence::null())
                    .unwrap();
                device.queue_wait_idle(queue.clone()).unwrap();
            };
        }

        // Free
        unsafe { device.free_command_buffers(command_pool.clone(), &command_buffers) };
    }

    /// Find a memory type in `mem_properties` that is suitable
    /// for `requirements` and supports `required_properties`.
    ///
    /// # Returns
    ///
    /// The index of the memory type from `mem_properties`.
    pub fn find_memory_type(
        requirements: vk::MemoryRequirements,
        mem_properties: vk::PhysicalDeviceMemoryProperties,
        required_properties: vk::MemoryPropertyFlags,
    ) -> u32 {
        for i in 0..mem_properties.memory_type_count {
            if requirements.memory_type_bits & (1 << i) != 0
                && mem_properties.memory_types[i as usize]
                    .property_flags
                    .contains(required_properties)
            {
                return i;
            }
        }
        panic!("Failed to find suitable memory type.")
    }

    pub fn create_and_register_command_buffers(
        device: &Device,
        pool: &vk::CommandPool,
    ) -> Vec<vk::CommandBuffer> {
        let allocate_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(pool.clone())
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(FRAMES_IN_FLIGHT + 1)
            .build();

        let buffers = unsafe { device.allocate_command_buffers(&allocate_info).unwrap() };

        buffers
    }

    pub fn updating_command_buffer(
        i: usize,
        buffer: &CommandBuffer,  
        device: &Device,
        pool: &vk::CommandPool,
        pipeline_layout: vk::PipelineLayout,
        descriptor_sets: &[vk::DescriptorSet],
        compute_pipeline: vk::Pipeline,
        images: &Vec<Image>,
        render_pass: RenderPass,
        framebuffers: &Vec<Framebuffer>,
        properties: SwapchainProperties,
        renderer: &mut Renderer,
        draw_data: &DrawData,     
    ){
        let buffer = *buffer;
        let framebuffer = framebuffers[i].clone();

        unsafe { device.reset_command_pool(pool.clone(), vk::CommandPoolResetFlags::empty()).expect("command pool reset") };

        // begin command buffer
        {
            let command_buffer_begin_info = vk::CommandBufferBeginInfo::builder()
                //.flags(vk::CommandBufferUsageFlags::)
                //.inheritance_info() null since it's a primary command buffer
                .build();
            unsafe {
                device
                    .begin_command_buffer(buffer, &command_buffer_begin_info)
                    .unwrap()
            };
        }

        // Bind pipeline
        unsafe {
            device.cmd_bind_pipeline(buffer, vk::PipelineBindPoint::COMPUTE, compute_pipeline.clone())
        };

        // Bind descriptor set
        unsafe {
            let null = [];
            device.cmd_bind_descriptor_sets(
                buffer,
                vk::PipelineBindPoint::COMPUTE,
                pipeline_layout.clone(),
                0,
                &descriptor_sets[i..=i],
                &null,
            )
        };

        Self::transition_image_layout_with_command_buffer(
            device,
            images[i],
            properties.format.format,
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::ImageLayout::GENERAL,
            buffer,
        );

        unsafe { device.cmd_dispatch(buffer, (properties.extent.width / 32) + 1, (properties.extent.height / 32) + 1, 1) };

            // begin render pass
            {
            let render_pass_begin_info = vk::RenderPassBeginInfo::builder()
                .render_pass(render_pass.clone())
                .framebuffer(framebuffer)
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: properties.extent,
                })
                .build();

            unsafe {
                device.cmd_begin_render_pass(
                    buffer,
                    &render_pass_begin_info,
                    vk::SubpassContents::INLINE,
                )
            };
        }

        renderer.cmd_draw(buffer, draw_data).expect("Imgui render failed");

        // End render pass
        unsafe { device.cmd_end_render_pass(buffer) };
        
        // End command buffer
        unsafe { device.end_command_buffer(buffer).unwrap() };
    }
}