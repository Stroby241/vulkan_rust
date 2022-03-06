mod instance;
mod device;
mod swapchain;
mod context;
mod render_pass;
mod texture;
mod fs;
mod math;
mod debug;
mod descriptor;
mod pipeline;
mod image;
mod shader;
mod framebuffers;
mod command;
mod sync;


use crate::{vulkan::{context::VkContext, debug::*, swapchain::*}};

use ash::{extensions::khr::{Surface, Swapchain}, vk::{ImageView, CommandPool, Queue}};

use ash::{vk, Entry};
use winit::window::Window;

use self::{device::QueueFamiliesIndices, sync::InFlightFrames};


const FRAMES_IN_FLIGHT: u32 = 2;


/*
pub const UNKNOWN: Self = Self(0);
pub const INSTANCE: Self = Self(1);
pub const PHYSICAL_DEVICE: Self = Self(2);
pub const DEVICE: Self = Self(3);
pub const QUEUE: Self = Self(4);
pub const SEMAPHORE: Self = Self(5);
pub const COMMAND_BUFFER: Self = Self(6);
pub const FENCE: Self = Self(7);
pub const DEVICE_MEMORY: Self = Self(8);
pub const BUFFER: Self = Self(9);
pub const IMAGE: Self = Self(10);
pub const EVENT: Self = Self(11);
pub const QUERY_POOL: Self = Self(12);
pub const BUFFER_VIEW: Self = Self(13);
pub const IMAGE_VIEW: Self = Self(14);
pub const SHADER_MODULE: Self = Self(15);
pub const PIPELINE_CACHE: Self = Self(16);
pub const PIPELINE_LAYOUT: Self = Self(17);
pub const RENDER_PASS: Self = Self(18);
pub const PIPELINE: Self = Self(19);
pub const DESCRIPTOR_SET_LAYOUT: Self = Self(20);
pub const SAMPLER: Self = Self(21);
pub const DESCRIPTOR_POOL: Self = Self(22);
pub const DESCRIPTOR_SET: Self = Self(23);
pub const FRAMEBUFFER: Self = Self(24);
pub const COMMAND_POOL: Self = Self(25);

*/

pub struct VulkanApp {
    vk_context: VkContext,
    setup: Vulkan_Setup,
    size_dependent: Size_Dependent,
}

pub struct Vulkan_Setup {
    queue_families_indices: QueueFamiliesIndices,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,
    command_pool: vk::CommandPool,
    in_flight_frames: InFlightFrames,
}

pub struct Size_Dependent {
    dimensions: [u32; 2],
    swapchain: Swapchain,
    swapchain_khr: vk::SwapchainKHR,
    properties: SwapchainProperties,
    image_views: Vec<ImageView>,
    render_pass: vk::RenderPass,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set_layout: vk::DescriptorSetLayout,
    descriptor_sets: Vec<vk::DescriptorSet>,
    pipeline: vk::Pipeline,
    pipeline_layout: vk::PipelineLayout,
    framebuffers: Vec<vk::Framebuffer>,
    command_buffers: Vec<vk::CommandBuffer>,
}

impl VulkanApp {
    pub fn new(window: &Window, dimensions: [u32; 2]) -> Self {
        log::debug!("Creating application.");

        let entry = unsafe { Entry::new().expect("Failed to create entry.") };
        let instance = Self::create_instance(&entry, window);

        let surface = Surface::new(&entry, &instance);
        let surface_khr =
            unsafe { ash_window::create_surface(&entry, &instance, window, None).unwrap() };

        let debug_report_callback = setup_debug_messenger(&entry, &instance);

        let (physical_device, queue_families_indices) = Self::pick_physical_device(&instance, &surface, surface_khr);

        let (device, graphics_queue, present_queue) =
        Self::create_logical_device_with_graphics_queue(
            &instance,
            physical_device,
            queue_families_indices,
        );

        let vk_context = VkContext::new(
            entry,
            instance,
            debug_report_callback,
            surface,
            surface_khr,
            physical_device,
            device,
        );

        info!("in_flight_frames");
        let in_flight_frames = Self::create_sync_objects(vk_context.device());

        info!("command_pool");
        let command_pool = Self::create_command_pool(
            vk_context.device(),
            queue_families_indices,
            vk::CommandPoolCreateFlags::TRANSIENT, //| vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
        );

        info!("Context done");


        let setup = Vulkan_Setup{
            queue_families_indices,
            graphics_queue,
            present_queue,
            command_pool,
            in_flight_frames
        };

        let size_dependent = Self::create_size_dependent(&vk_context, &setup, dimensions);
        Self {
            vk_context,
            setup,
            size_dependent,
        }
    }

    fn create_size_dependent(
        vk_context: &VkContext, 
        setup: &Vulkan_Setup,
        dimensions: [u32; 2]
    ) -> Size_Dependent {

        info!("Creating size dependent");

        info!("swapchain");
        let (swapchain, swapchain_khr, properties, images) =
            Self::create_swapchain_and_images(&vk_context, setup.queue_families_indices, dimensions);

        info!("Creating swapchain_image_views");
        let image_views =
            Self::create_swapchain_image_views(vk_context.device(), &images, properties);

        
        info!("Creating render_pass");
        let render_pass = Self::create_render_pass(vk_context.device(), properties);


        info!("Creating framebuffers");

        let framebuffers = Self::create_framebuffers(
            vk_context.device(),
            &image_views,
            render_pass,
            properties,
        );


        info!("descriptor_pool");
        let descriptor_pool = Self::create_descriptor_pool(vk_context.device());

        info!("descriptor_set_layout");
        let descriptor_set_layout = Self::create_descriptor_set_layout(vk_context.device());

        info!("Creating descriptor_sets");
        let descriptor_sets = Self::create_descriptor_sets(
            vk_context.device(),
            descriptor_pool,
            &descriptor_set_layout,
            &image_views,
        );


        info!("pipeline");
        let (pipeline, pipeline_layout) = Self::create_compute_pipeline(
            vk_context.device(),
            &descriptor_set_layout,
        );

        info!("Creating command_buffers");
        let command_buffers = Self::create_and_register_command_buffers(
            vk_context.device(),
            &setup.command_pool,
            pipeline_layout,
            &descriptor_sets,
            pipeline,
            &images,
            render_pass,
            &framebuffers,
            properties
        );


        info!("images");
        for image in images {
            Self::transition_image_layout_one_time(
                vk_context.device(),
                &setup.command_pool,
                &setup.graphics_queue,
                image,
                properties.format.format,
                vk::ImageLayout::UNDEFINED,
                vk::ImageLayout::PRESENT_SRC_KHR,
            );
        }


        info!("Creating size dependent done");

        Size_Dependent{
            dimensions,
            swapchain,
            swapchain_khr,
            properties,
            image_views,
            render_pass,
            descriptor_pool,
            descriptor_set_layout,
            descriptor_sets,
            pipeline,
            pipeline_layout,
            framebuffers,
            command_buffers,
        }
    }

    pub fn recreate_size_dependent(&mut self, size: [u32; 2]){
        self.wait_gpu_idle();
        self.cleanup_size_dependent();
        self.size_dependent = Self::create_size_dependent(&self.vk_context, &self.setup, size);
    }


    pub fn draw_frame(&mut self) -> bool {
        
        let sync_objects = self.setup.in_flight_frames.next().unwrap();
        let image_available_semaphore = sync_objects.image_available_semaphore;
        let render_finished_semaphore = sync_objects.render_finished_semaphore;
        let in_flight_fence = sync_objects.fence;
        let wait_fences = [in_flight_fence];

        unsafe {
            self.vk_context
                .device()
                .wait_for_fences(&wait_fences, true, std::u64::MAX)
                .unwrap()
        };

        let result = unsafe {
            self.size_dependent.swapchain.acquire_next_image(
                self.size_dependent.swapchain_khr,
                std::u64::MAX,
                image_available_semaphore,
                vk::Fence::null(),
            )
        };
        let image_index = match result {
            Ok((image_index, _)) => image_index,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                return true;
            }
            Err(error) => panic!("Error while acquiring next image. Cause: {}", error),
        };

        unsafe { self.vk_context.device().reset_fences(&wait_fences).unwrap() };

        //self.update_uniform_buffers(image_index);

        let device = self.vk_context.device();
        let wait_semaphores = [image_available_semaphore];
        let signal_semaphores = [render_finished_semaphore];

        // Submit command buffer
        {
            let wait_stages = [vk::PipelineStageFlags::COMPUTE_SHADER];
            let command_buffers = [self.size_dependent.command_buffers[image_index as usize]];
            let submit_info = vk::SubmitInfo::builder()
                .wait_semaphores(&wait_semaphores)
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(&command_buffers)
                .signal_semaphores(&signal_semaphores)
                .build();
            let submit_infos = [submit_info];
            unsafe {
                device
                    .queue_submit(self.setup.graphics_queue, &submit_infos, in_flight_fence)
                    .unwrap()
            };
        }

        let swapchains = [self.size_dependent.swapchain_khr];
        let images_indices = [image_index];

        {
            let present_info = vk::PresentInfoKHR::builder()
                .wait_semaphores(&signal_semaphores)
                .swapchains(&swapchains)
                .image_indices(&images_indices)
                // .results() null since we only have one swapchain
                .build();
            let result = unsafe {
                self.size_dependent.swapchain
                    .queue_present(self.setup.present_queue, &present_info)
            };
            match result {
                Ok(true) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    return true;
                }
                Err(error) => panic!("Failed to present queue. Cause: {}", error),
                _ => {}
            }
        }
        false
    }

    pub fn cleanup_size_dependent(&mut self) {
        let size_dependent = &self.size_dependent;
        let device = self.vk_context.device();
        unsafe {
            size_dependent.framebuffers.iter().for_each(|f| device.destroy_framebuffer(*f, None));
            device.free_command_buffers(self.setup.command_pool, &size_dependent.command_buffers);
            device.destroy_pipeline(size_dependent.pipeline, None);
            device.destroy_pipeline_layout(size_dependent.pipeline_layout, None);
            device.destroy_render_pass(size_dependent.render_pass, None);

            device.destroy_descriptor_pool(size_dependent.descriptor_pool, None);
            device.destroy_descriptor_set_layout(size_dependent.descriptor_set_layout, None);

            size_dependent.image_views.iter().for_each(|v| device.destroy_image_view(*v, None));
            size_dependent.swapchain.destroy_swapchain(size_dependent.swapchain_khr, None);
        }
    }
}

impl Drop for VulkanApp {
    fn drop(&mut self) {
        debug!("Dropping application.");
        self.cleanup_size_dependent();

        let device = self.vk_context.device();
        self.setup.in_flight_frames.destroy(device);
        unsafe {
            device.destroy_command_pool(self.setup.command_pool, None);
        }
    }
}