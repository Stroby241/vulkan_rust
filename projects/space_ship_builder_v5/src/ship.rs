use crate::math::{get_packed_index, to_3d_i};
use crate::node::{Node, NodeID, PatternIndex, EMPYT_PATTERN_INDEX};
use crate::rules::Rules;
use crate::ship_mesh::RenderNode;
use crate::{
    math::{to_1d, to_1d_i, to_3d},
    node::{BlockIndex, BLOCK_INDEX_EMPTY},
    ship_mesh::ShipMesh,
};
use index_queue::IndexQueue;
use log::{debug, info};
use octa_force::{anyhow::*, glam::*, log};
use std::cmp::max;

#[cfg(debug_assertions)]
use crate::debug::DebugController;

pub type ChunkIndex = usize;
pub type WaveIndex = usize;

pub const CHUNK_SIZE: i32 = 32;

pub struct Ship {
    pub chunks: Vec<ShipChunk>,

    pub blocks_per_chunk: IVec3,
    pub nodes_per_chunk: IVec3,
    pub chunk_pos_mask: IVec3,
    pub chunk_voxel_size: IVec3,
    pub in_chunk_pos_mask: IVec3,
    pub node_index_bits: usize,
    pub node_index_mask: usize,

    pub to_reset: IndexQueue,
    pub was_reset: IndexQueue,
    pub to_propergate: IndexQueue,
    pub to_collapse: IndexQueue,
}

pub struct ShipChunk {
    pub pos: IVec3,
    pub blocks: Vec<BlockIndex>,
    pub nodes: Vec<Option<Vec<(NodeID, usize)>>>,
    pub node_id_bits: Vec<u32>,
    pub render_nodes: Vec<RenderNode>,
}

impl Ship {
    pub fn new(node_size: i32, rules: &Rules) -> Result<Ship> {
        let blocks_per_chunk = IVec3::ONE * node_size / 2;
        let nodes_per_chunk = IVec3::ONE * node_size;
        let chunk_pos_mask = IVec3::ONE * !(node_size - 1);
        let chunk_voxel_size = IVec3::ONE * node_size;
        let in_chunk_pos_mask = IVec3::ONE * (node_size - 1);
        let node_index_bits = (nodes_per_chunk.element_product().trailing_zeros() + 1) as usize;
        let node_index_mask = (nodes_per_chunk.element_product() - 1) as usize;

        let mut ship = Ship {
            chunks: Vec::new(),

            blocks_per_chunk,
            nodes_per_chunk,
            chunk_pos_mask,
            chunk_voxel_size,
            in_chunk_pos_mask,
            node_index_bits,
            node_index_mask,

            to_reset: IndexQueue::default(),
            was_reset: IndexQueue::default(),
            to_propergate: IndexQueue::default(),
            to_collapse: IndexQueue::default(),
        };
        ship.add_chunk(IVec3::ZERO);

        //ship.place_block(ivec3(0, 0, 0), 1, rules)?;
        //ship.fill_all(0, node_controller)?;

        Ok(ship)
    }

    pub fn place_block(
        &mut self,
        block_pos: IVec3,
        block_index: BlockIndex,
        rules: &Rules,
    ) -> Result<()> {
        let pos = self.get_node_pos_from_block_pos(block_pos);

        let chunk_index = self.get_chunk_index(pos)?;
        let in_chunk_block_index = self.get_block_index(pos);

        let chunk = &mut self.chunks[chunk_index];

        let old_block_index = chunk.blocks[in_chunk_block_index];
        if old_block_index == block_index {
            return Ok(());
        }

        log::info!("Place: {block_pos:?}");
        chunk.blocks[in_chunk_block_index] = block_index;

        let mut push_reset = |block_index: BlockIndex, pos: IVec3| -> Result<()> {
            if block_index == BLOCK_INDEX_EMPTY {
                return Ok(());
            }

            for offset in rules.affected_by_block[block_index].iter() {
                let affected_pos = pos + *offset;

                let chunk_index = self.get_chunk_index(affected_pos);
                if chunk_index.is_err() {
                    continue;
                }

                let node_index = self.get_node_index(affected_pos);

                let node_world_index = self.to_world_node_index(chunk_index.unwrap(), node_index);
                self.to_reset.push_back(node_world_index);
            }

            Ok(())
        };

        push_reset(old_block_index, pos)?;
        push_reset(block_index, pos)?;

        self.was_reset = IndexQueue::default();

        Ok(())
    }

    pub fn tick(
        &mut self,
        actions_per_tick: usize,
        rules: &Rules,
        #[cfg(debug_assertions)] debug: bool,
    ) -> Result<(bool, Vec<ChunkIndex>)> {
        let mut changed_chunks = Vec::new();
        for _ in 0..actions_per_tick {
            if !self.to_reset.is_empty() {
                #[cfg(debug_assertions)]
                self.reset(rules, debug);

                #[cfg(not(debug_assertions))]
                self.reset(rules);
            } else if !self.to_propergate.is_empty() {
                self.propergate(rules);
            } else if !self.to_collapse.is_empty() {
                self.collapse()?;
                changed_chunks = vec![0];
            } else {
                return Ok((false, changed_chunks));
            }
        }

        info!("Tick: {actions_per_tick}");

        Ok((true, changed_chunks))
    }

    fn reset(&mut self, rules: &Rules, #[cfg(debug_assertions)] debug: bool) {
        let node_world_index = self.to_reset.pop_front().unwrap();
        let (chunk_index, node_index) = self.from_world_node_index(node_world_index);
        let pos = self.pos_from_world_node_index(chunk_index, node_index);

        let new_possible_node_ids = self.propergate_node_world_index(rules, node_world_index, true);

        let old_possible_node_ids = self.chunks[chunk_index].nodes[node_index]
            .take()
            .unwrap_or(Vec::new());

        if new_possible_node_ids != old_possible_node_ids {
            let mut push_neigbors = |node_id: NodeID| {
                for offset in rules.affected_by_node[&node_id].iter() {
                    let affected_pos = pos + *offset;

                    let chunk_index = self.get_chunk_index(affected_pos);
                    if chunk_index.is_err() {
                        continue;
                    }

                    let node_index = self.get_node_index(affected_pos);

                    let node_world_index =
                        self.to_world_node_index(chunk_index.unwrap(), node_index);

                    if !self.was_reset.contains(node_world_index) {
                        self.to_reset.push_back(node_world_index);
                    } else {
                        self.to_propergate.push_back(node_world_index);
                    }
                }
            };

            for (node_id, _) in old_possible_node_ids.iter() {
                push_neigbors(node_id.to_owned());
            }

            for (node_id, _) in new_possible_node_ids.iter() {
                push_neigbors(node_id.to_owned());
            }

            #[cfg(debug_assertions)]
            if debug {
                let node_index_plus_padding =
                    self.node_index_to_node_index_plus_padding(node_index);
                self.chunks[chunk_index].render_nodes[node_index_plus_padding] = RenderNode(true);
            }
            self.chunks[chunk_index].node_id_bits[node_index] = NodeID::none().into();

            self.was_reset.push_back(node_world_index);
            self.to_propergate.push_back(node_world_index);
            self.to_collapse.push_back(node_world_index);
        }

        self.chunks[chunk_index].nodes[node_index] = Some(new_possible_node_ids);
    }

    fn propergate_node_world_index(
        &mut self,
        rules: &Rules,
        node_world_index: usize,
        reset_nodes: bool,
    ) -> Vec<(NodeID, usize)> {
        let (chunk_index, node_index) = self.from_world_node_index(node_world_index);
        let pos = self.pos_from_world_node_index(chunk_index, node_index);

        let mut new_possible_node_ids = Vec::new();

        for ((node_id, block_reqs), node_req) in rules
            .map_rules_index_to_node_id
            .iter()
            .zip(rules.block_rules.iter())
            .zip(rules.node_rules.iter())
        {
            let mut block_accepted = false;
            let mut block_prio = 0;

            // Go over all possible nodes
            for (req, prio) in block_reqs {
                let mut check_performed = false;
                let mut accepted = true;
                // Go over all offsets of the requirement
                for (offset, id) in req.iter() {
                    let test_pos = pos + *offset;

                    // If the offset does not aling with the node just ignore it.
                    if (test_pos % 2) != IVec3::ZERO {
                        continue;
                    }

                    let test_chunk_index = self.get_chunk_index(test_pos);
                    let test_block_index = self.get_block_index(test_pos);

                    let mut found = false;
                    if test_chunk_index.is_err() {
                        // If the block is in chunk that does not exist it is always Air.

                        found = *id == BLOCK_INDEX_EMPTY
                    } else {
                        // If the chuck exists.

                        let index = self.chunks[test_chunk_index.unwrap()].blocks[test_block_index]
                            .to_owned();

                        // Check if the Block at the pos is in the allowed id.
                        found = *id == index;
                    };

                    accepted &= found;
                    check_performed = true;
                }

                if accepted && check_performed {
                    block_accepted = true;
                    block_prio = max(block_prio, *prio);
                    break;
                }
            }

            if !block_accepted {
                continue;
            }

            let mut node_accepted = true;
            for (offset, ids) in node_req {
                let test_pos = pos + *offset;

                let test_chunk_index = self.get_chunk_index(test_pos);
                let test_node_index = self.get_node_index(test_pos);

                let mut found = false;
                if test_chunk_index.is_err() {
                    found = ids.iter().any(|node| node.is_none());
                } else {
                    let test_node = &self.chunks[test_chunk_index.unwrap()].nodes[test_node_index];
                    if reset_nodes || test_node.is_none() {
                        found = true;
                    } else {
                        let test_ids = test_node.to_owned().unwrap();
                        for (test_id, _) in test_ids {
                            if ids.contains(&test_id) {
                                found = true;
                                break;
                            }
                        }
                    }
                }

                node_accepted &= found;
            }

            if !node_accepted {
                continue;
            }

            new_possible_node_ids.push((node_id.to_owned(), block_prio));
        }

        return new_possible_node_ids;
    }

    fn propergate(&mut self, rules: &Rules) {
        let node_world_index = self.to_propergate.pop_front().unwrap();
        let (chunk_index, node_index) = self.from_world_node_index(node_world_index);
        let pos = self.pos_from_world_node_index(chunk_index, node_index);

        //debug!("Propergate: {node_world_index}");

        let new_possible_node_ids =
            self.propergate_node_world_index(rules, node_world_index, false);

        let old_possible_node_ids = self.chunks[chunk_index].nodes[node_index]
            .take()
            .unwrap_or(Vec::new());

        if old_possible_node_ids != new_possible_node_ids {
            let mut push_neigbors = |node_id: NodeID| {
                for offset in rules.affected_by_node[&node_id].iter() {
                    let affected_pos = pos + *offset;

                    let chunk_index = self.get_chunk_index(affected_pos);
                    if chunk_index.is_err() {
                        continue;
                    }

                    let node_index = self.get_node_index(affected_pos);

                    let node_world_index =
                        self.to_world_node_index(chunk_index.unwrap(), node_index);

                    self.to_propergate.push_back(node_world_index);
                }
            };

            for (node_id, _) in old_possible_node_ids.iter() {
                push_neigbors(node_id.to_owned());
            }

            for (node_id, _) in new_possible_node_ids.iter() {
                push_neigbors(node_id.to_owned());
            }

            self.to_collapse.push_back(node_world_index);
        }

        self.chunks[chunk_index].nodes[node_index] = Some(new_possible_node_ids);
    }

    fn collapse(&mut self) -> Result<()> {
        let node_world_index = self.to_collapse.pop_front().unwrap();
        let (chunk_index, node_index) = self.from_world_node_index(node_world_index);
        let node_index_plus_padding = self.node_index_to_node_index_plus_padding(node_index);

        //debug!("Collapse: {node_world_index}");

        let possible_node_ids = self.chunks[chunk_index].nodes[node_index]
            .take()
            .unwrap_or(Vec::new());

        let (node_id, _) = possible_node_ids
            .iter()
            .max_by(|(_, prio1), (_, prio2)| prio1.cmp(prio2))
            .unwrap_or(&(NodeID::none(), 0))
            .to_owned();
        self.chunks[chunk_index].node_id_bits[node_index] = node_id.into();
        self.chunks[chunk_index].render_nodes[node_index_plus_padding] =
            RenderNode(!node_id.is_none());

        self.chunks[chunk_index].nodes[node_index] = Some(possible_node_ids);
        Ok(())
    }

    #[cfg(debug_assertions)]
    pub fn show_debug(&self, debug_controller: &mut DebugController) {
        for chunk in self.chunks.iter() {
            debug_controller.add_cube(
                (chunk.pos * self.nodes_per_chunk).as_vec3(),
                ((chunk.pos + IVec3::ONE) * self.nodes_per_chunk).as_vec3(),
                vec4(1.0, 0.0, 0.0, 1.0),
            );
        }

        let mut to_reset = self.to_reset.to_owned();

        while !to_reset.is_empty() {
            let node_world_index = to_reset.pop_front().unwrap();
            let (chunk_index, node_index) = self.from_world_node_index(node_world_index);
            let pos = self.pos_from_world_node_index(chunk_index, node_index);

            debug_controller.add_cube(
                pos.as_vec3(),
                pos.as_vec3() + Vec3::ONE,
                vec4(1.0, 0.0, 0.0, 1.0),
            );
        }

        let mut to_propergate = self.to_propergate.to_owned();

        while !to_propergate.is_empty() {
            let node_world_index = to_propergate.pop_front().unwrap();
            let (chunk_index, node_index) = self.from_world_node_index(node_world_index);
            let pos = self.pos_from_world_node_index(chunk_index, node_index);

            debug_controller.add_cube(
                pos.as_vec3() + Vec3::ONE * 0.01,
                pos.as_vec3() + Vec3::ONE * 0.99,
                vec4(0.0, 1.0, 0.0, 1.0),
            );
        }

        let mut to_collapse = self.to_collapse.to_owned();

        while !to_collapse.is_empty() {
            let node_world_index = to_collapse.pop_front().unwrap();
            let (chunk_index, node_index) = self.from_world_node_index(node_world_index);
            let pos = self.pos_from_world_node_index(chunk_index, node_index);

            debug_controller.add_cube(
                pos.as_vec3() + Vec3::ONE * 0.02,
                pos.as_vec3() + Vec3::ONE * 0.98,
                vec4(0.0, 0.0, 1.0, 1.0),
            );
        }
    }

    pub fn on_rules_changed(&mut self) -> Result<()> {
        for chunk_index in 0..self.chunks.len() {
            for node_index in 0..self.node_length() {
                let node_world_index = self.to_world_node_index(chunk_index, node_index);
                self.to_propergate.push_back(node_world_index);
            }
        }

        std::prelude::rust_2015::Ok(())
    }

    // Math
    pub fn block_length(&self) -> usize {
        self.blocks_per_chunk.element_product() as usize
    }
    pub fn node_length(&self) -> usize {
        self.nodes_per_chunk.element_product() as usize
    }
    pub fn node_size_plus_padding(&self) -> IVec3 {
        self.nodes_per_chunk + 2
    }
    pub fn node_length_plus_padding(&self) -> usize {
        Self::node_size_plus_padding(self).element_product() as usize
    }

    pub fn add_chunk(&mut self, chunk_pos: IVec3) {
        let chunk = ShipChunk {
            pos: chunk_pos,
            blocks: vec![BLOCK_INDEX_EMPTY; self.block_length()],
            nodes: vec![None; self.node_length()],
            node_id_bits: vec![0; self.node_length()],
            render_nodes: vec![RenderNode(false); self.node_length_plus_padding()],
        };

        self.chunks.push(chunk)
    }

    pub fn has_chunk(&self, chunk_pos: IVec3) -> bool {
        chunk_pos == IVec3::ZERO
    }

    pub fn get_chunk_index(&self, pos: IVec3) -> Result<usize> {
        let chunk_pos = self.get_chunk_pos(pos);

        if !self.has_chunk(chunk_pos) {
            bail!("Chunk not found!");
        }

        Ok(0)
    }

    pub fn get_node_pos_from_block_pos(&self, pos: IVec3) -> IVec3 {
        pos * 2
    }

    pub fn get_chunk_pos(&self, pos: IVec3) -> IVec3 {
        (pos & self.chunk_pos_mask)
            - self.chunk_voxel_size
                * ivec3((pos.x < 0) as i32, (pos.y < 0) as i32, (pos.z < 0) as i32)
    }

    pub fn get_in_chunk_pos(&self, pos: IVec3) -> IVec3 {
        pos & self.in_chunk_pos_mask
    }

    pub fn get_block_index(&self, pos: IVec3) -> usize {
        let in_chunk_index = self.get_in_chunk_pos(pos);
        to_1d_i(in_chunk_index / 2, self.blocks_per_chunk) as usize
    }

    pub fn get_node_index(&self, pos: IVec3) -> usize {
        let in_chunk_index = self.get_in_chunk_pos(pos);
        to_1d_i(in_chunk_index, self.nodes_per_chunk) as usize
    }

    pub fn to_world_node_index(&self, chunk_index: usize, node_index: usize) -> usize {
        node_index + (chunk_index << self.node_index_bits)
    }

    pub fn from_world_node_index(&self, node_world_index: usize) -> (usize, usize) {
        (
            node_world_index >> self.node_index_bits,
            node_world_index & self.node_index_mask,
        )
    }

    pub fn pos_from_world_node_index(&self, chunk_index: usize, node_index: usize) -> IVec3 {
        let chunk_pos = self.chunks[chunk_index].pos;
        let node_pos = to_3d_i(node_index as i32, self.nodes_per_chunk);

        chunk_pos + node_pos
    }

    pub fn node_index_to_node_index_plus_padding(&self, node_index: usize) -> usize {
        let node_pos = to_3d_i(node_index as i32, self.nodes_per_chunk);
        to_1d_i(node_pos + IVec3::ONE, self.node_size_plus_padding()) as usize
    }
}
