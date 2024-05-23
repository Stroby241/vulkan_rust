use crate::debug::DebugController;
use crate::math::to_1d_i;
use crate::ship::{Ship, ShipChunk};
use crate::ship_mesh::{MeshChunk, RenderNode, ShipMesh};
use crate::ship_renderer::{ShipRenderer, RENDER_MODE_BUILD};
use octa_force::anyhow::Result;
use octa_force::glam::{ivec3, vec3, vec4, IVec3, Vec3};
use octa_force::vulkan::ash::vk;
use octa_force::vulkan::{Buffer, CommandBuffer, Context, DescriptorPool, DescriptorSetLayout};

pub struct DebugPossibleNodeRenderer {
    mesh: ShipMesh,
}

impl DebugPossibleNodeRenderer {
    pub fn new(image_len: usize, ship: &Ship) -> Result<Self> {
        Ok(DebugPossibleNodeRenderer {
            mesh: ShipMesh::new(image_len, IVec3::ONE * 128, ship.nodes_per_chunk)?,
        })
    }

    pub fn render(&mut self, buffer: &CommandBuffer, renderer: &ShipRenderer, image_index: usize) {
        buffer.bind_graphics_pipeline(&renderer.pipeline);
        buffer.bind_descriptor_sets(
            vk::PipelineBindPoint::GRAPHICS,
            &renderer.pipeline_layout,
            0,
            &[&renderer.static_descriptor_sets[image_index]],
        );

        renderer.render_ship_mesh(buffer, image_index, &self.mesh, RENDER_MODE_BUILD)
    }
}

impl DebugController {
    pub fn update_possible_nodes(
        &mut self,
        ship: &Ship,
        image_index: usize,
        context: &Context,
        descriptor_layout: &DescriptorSetLayout,
        descriptor_pool: &DescriptorPool,
    ) -> Result<()> {
        self.add_text(vec!["WFC".to_owned()], vec3(-1.0, 0.0, 0.0));

        ship.show_debug(self);

        self.possible_node_renderer.mesh.to_drop_buffers[image_index].clear();

        for chunk in ship.chunks.iter() {
            let mesh_chunk_index = self
                .possible_node_renderer
                .mesh
                .chunks
                .iter()
                .position(|c| c.pos == chunk.pos);

            let node_id_bits = self.get_chunk_node_id_bits_debug(
                chunk,
                self.possible_node_renderer.mesh.size,
                ship,
            );

            if mesh_chunk_index.is_some() {
                self.possible_node_renderer.mesh.chunks[mesh_chunk_index.unwrap()]
                    .update_from_data(
                        &node_id_bits,
                        &chunk.render_nodes,
                        context,
                        &mut self.possible_node_renderer.mesh.to_drop_buffers[image_index],
                    )?;
            } else {
                let new_chunk = MeshChunk::new_from_data(
                    chunk.pos,
                    self.possible_node_renderer.mesh.size,
                    self.possible_node_renderer.mesh.render_size,
                    &node_id_bits,
                    &chunk.render_nodes,
                    self.possible_node_renderer.mesh.to_drop_buffers.len(),
                    context,
                    descriptor_layout,
                    descriptor_pool,
                )?;

                if new_chunk.is_some() {
                    self.possible_node_renderer
                        .mesh
                        .chunks
                        .push(new_chunk.unwrap())
                }
            }
        }

        self.text_renderer.push_texts()?;
        self.line_renderer.push_lines()?;

        Ok(())
    }

    fn get_chunk_node_id_bits_debug(
        &mut self,
        ship_chunk: &ShipChunk,
        size: IVec3,
        ship: &Ship,
    ) -> Vec<u32> {
        let mut node_debug_node_id_bits = vec![0; size.element_product() as usize];
        let pattern_block_size = size / ship.nodes_per_chunk;

        for x in 0..ship.nodes_per_chunk.x {
            for y in 0..ship.nodes_per_chunk.y {
                for z in 0..ship.nodes_per_chunk.z {
                    let node_pos = ivec3(x, y, z);
                    let node_index = ship.get_node_index(node_pos);
                    let r = ship_chunk.nodes[node_index].to_owned();
                    if r.is_none() {
                        continue;
                    }

                    let mut pattern_counter = 0;
                    let possible_pattern = r.unwrap();
                    let node_pos = node_pos * pattern_block_size;

                    'iter: for iz in 0..pattern_block_size.x {
                        for iy in 0..pattern_block_size.y {
                            for ix in 0..pattern_block_size.z {
                                if possible_pattern.len() <= pattern_counter {
                                    break 'iter;
                                }

                                let pattern_pos = ivec3(ix, iy, iz) + node_pos;
                                let index = to_1d_i(pattern_pos, size) as usize;

                                let (node_id, _) = possible_pattern[pattern_counter];
                                node_debug_node_id_bits[index] = node_id.into();

                                if node_id.is_empty() {
                                    let one_cell_size = Vec3::ONE / pattern_block_size.as_vec3();
                                    let p = pattern_pos.as_vec3() * one_cell_size;
                                    self.add_cube(p, p + one_cell_size, vec4(0.0, 1.0, 0.0, 1.0));
                                }

                                pattern_counter += 1;
                            }
                        }
                    }
                }
            }
        }

        node_debug_node_id_bits
    }
}