use octa_force::glam::UVec2;
use octa_force::vulkan::ash::vk;
use octa_force::vulkan::{CommandBuffer, Context};
use octa_force::anyhow::Result;
use crate::render::parallax::chunk::ParallaxData;
use crate::render::parallax::renderer::ParallaxRenderer;
use crate::rules::Rules;
use crate::world::block_object::{BlockChunk, BlockObject, ChunkIndex};

pub mod parallax;
// pub mod raytracer;

pub enum ActiveRenderer {
    None,
    Parallax,
    Compute, 
    Raytracing
}

pub struct Renderer {
    parallax_renderer: Option<ParallaxRenderer>,
    active_renderer: ActiveRenderer,
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer {
            parallax_renderer: None,
            active_renderer: ActiveRenderer::None,
        }
    }
    
    pub fn enable_parallax(
        &mut self, 
        context: &Context, 
        num_frames: usize, 
        color_attachment_format: vk::Format, 
        depth_attachment_format: vk::Format,
        rules: &Rules
    ) -> Result<()> {
        if self.parallax_renderer.is_none() {
            self.parallax_renderer = Some(ParallaxRenderer::new(context, num_frames, color_attachment_format, depth_attachment_format, rules)?);
        }
        self.active_renderer = ActiveRenderer::Parallax;
        
        Ok(())
    }
    
    pub fn update_object(
        &self,
        object: &mut BlockObject,
        changed_chunks: Vec<ChunkIndex>,
        context: &Context,
        frame_index: usize,
        num_frames: usize,
    ) -> Result<()>{
        match self.active_renderer {
            ActiveRenderer::None => {}
            ActiveRenderer::Parallax => {
                let renderer = self.parallax_renderer.as_ref().unwrap();
                
                for chunk_index in changed_chunks {
                    let chunk = &mut object.chunks[chunk_index];

                    if chunk.parallax_data.is_none() {
                        chunk.parallax_data = Some(ParallaxData::new(
                            chunk.pos,
                            object.nodes_per_chunk,
                            object.nodes_length, 
                            num_frames, 
                            context,
                            &renderer.chunk_descriptor_layout,
                            &renderer.descriptor_pool,
                        )?);
                    }
                    
                    chunk.parallax_data.as_mut().unwrap().update(
                        object.nodes_per_chunk, 
                        &chunk.node_id_bits,
                        &chunk.render_nodes,
                        context,
                        &mut renderer.to_drop_buffers[frame_index],
                    ).unwrap();
                }
            }
            ActiveRenderer::Compute => {}
            ActiveRenderer::Raytracing => {}
        }
        
        Ok(())
    }

    pub fn render<'a, I>(&self, buffer: &CommandBuffer, frame_index: usize, chunks_to_render: I)
        where I: Iterator<Item = &'a BlockChunk>, 
    {
        match self.active_renderer {
            ActiveRenderer::None => {}
            ActiveRenderer::Parallax => {
                let renderer = self.parallax_renderer.as_ref().unwrap();
                renderer.begin_render(buffer, frame_index);

                for chunk in chunks_to_render {
                    if chunk.parallax_data.is_none() {
                        continue
                    }
                    
                    renderer.render_data(
                        buffer, 
                        frame_index, 
                        chunk.parallax_data.as_ref().unwrap()
                    
                    );
                }
            }
            ActiveRenderer::Compute => {}
            ActiveRenderer::Raytracing => {}
        }
    }
}
