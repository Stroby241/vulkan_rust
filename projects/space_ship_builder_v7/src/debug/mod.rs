pub mod hull_base;
pub mod line_renderer;
pub mod rotation_debug;
pub mod text_renderer;

use crate::debug::hull_base::DebugHullBaseRenderer;
use crate::debug::line_renderer::DebugLineRenderer;
use crate::debug::rotation_debug::RotationDebugRenderer;
use crate::debug::text_renderer::DebugTextRenderer;
use crate::node::{NodeID, Voxel};
use crate::rules::Rules;
use crate::ship::data::ShipData;
use crate::ship::renderer::ShipRenderer;
use crate::voxel_loader::VoxelLoader;
use octa_force::anyhow::Result;
use octa_force::camera::Camera;
use octa_force::controls::Controls;
use octa_force::egui_winit::winit::window::Window;
use octa_force::vulkan::ash::vk::{Extent2D, Format};
use octa_force::vulkan::{CommandBuffer, Context};
use std::time::Duration;

#[derive(PartialEq)]
pub enum DebugMode {
    OFF,
    ROTATION_DEBUG,
    HULL_BASE,
}

const DEBUG_MODE_CHANGE_SPEED: Duration = Duration::from_millis(500);

pub struct DebugController {
    pub mode: DebugMode,
    pub line_renderer: DebugLineRenderer,
    pub text_renderer: DebugTextRenderer,

    pub rotation_debug: RotationDebugRenderer,
    pub renderer_hull_base: DebugHullBaseRenderer,

    last_mode_change: Duration,
}

impl DebugController {
    pub fn new(
        context: &Context,
        images_len: usize,
        format: Format,
        window: &Window,
        renderer: &ShipRenderer,
        test_node_id: NodeID,
    ) -> Result<Self> {
        let line_renderer = DebugLineRenderer::new(
            1000000,
            context,
            images_len as u32,
            format,
            Format::D32_SFLOAT,
            &renderer,
        )?;

        let text_renderer = DebugTextRenderer::new(context, format, window, images_len)?;
        let rotation_debug_renderer = RotationDebugRenderer::new(images_len, test_node_id);
        let hull_block_req_renderer = DebugHullBaseRenderer::new(images_len);

        Ok(DebugController {
            mode: DebugMode::HULL_BASE,
            line_renderer,
            text_renderer,
            rotation_debug: rotation_debug_renderer,
            renderer_hull_base: hull_block_req_renderer,
            last_mode_change: Duration::ZERO,
        })
    }

    pub fn update(
        &mut self,
        context: &Context,
        controls: &Controls,
        renderer: &ShipRenderer,
        voxel_loader: &mut VoxelLoader,
        total_time: Duration,
        ship: &ShipData,
        image_index: usize,
        rules: &Rules,
    ) -> Result<()> {
        if controls.f2 && (self.last_mode_change + DEBUG_MODE_CHANGE_SPEED) < total_time {
            self.last_mode_change = total_time;

            self.mode = if self.mode != DebugMode::ROTATION_DEBUG {
                DebugMode::ROTATION_DEBUG
            } else {
                DebugMode::OFF
            }
        }
        if controls.f3 && (self.last_mode_change + DEBUG_MODE_CHANGE_SPEED) < total_time {
            self.last_mode_change = total_time;

            self.mode = if self.mode != DebugMode::HULL_BASE {
                DebugMode::HULL_BASE
            } else {
                DebugMode::OFF
            }
        }

        match self.mode {
            DebugMode::OFF => {
                self.line_renderer.vertecies_count = 0;
            }
            DebugMode::HULL_BASE => {
                self.update_hull_base(
                    rules.solvers[1].to_hull(),
                    controls,
                    image_index,
                    &context,
                    &renderer.chunk_descriptor_layout,
                    &renderer.descriptor_pool,
                )?;
            }
            DebugMode::ROTATION_DEBUG => {
                self.update_rotation_debug(
                    controls,
                    image_index,
                    &context,
                    &renderer.chunk_descriptor_layout,
                    &renderer.descriptor_pool,
                )?;
            }
        }

        Ok(())
    }

    pub fn render(
        &mut self,
        buffer: &CommandBuffer,
        image_index: usize,
        camera: &Camera,
        extent: Extent2D,
        renderer: &ShipRenderer,
    ) -> Result<()> {
        if self.mode == DebugMode::OFF {
            return Ok(());
        }

        self.text_renderer.render(buffer, camera, extent)?;
        self.line_renderer.render(buffer, image_index);

        match self.mode {
            DebugMode::OFF => {}
            DebugMode::HULL_BASE => {
                self.renderer_hull_base
                    .render(buffer, renderer, image_index);
            }
            DebugMode::ROTATION_DEBUG => self.rotation_debug.render(buffer, renderer, image_index),
        }

        Ok(())
    }
}
