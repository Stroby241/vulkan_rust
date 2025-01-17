use crate::math::{all_sides_dirs, get_all_poses, get_neighbors, oct_positions, to_1d};
use crate::node::{NodeID, NODE_VOXEL_LENGTH, VOXEL_EMPTY};
use crate::rotation::Rot;
use crate::rules::block::Block;
use crate::rules::empty::EMPTY_BLOCK_NAME_INDEX;
use crate::rules::solver::{Solver, SolverCacheIndex};
use crate::rules::Prio::HULL_BASE;
use crate::rules::{Prio, Rules};
use crate::ship::data::{CacheIndex, ShipData};
use crate::ship::possible_blocks::PossibleBlocks;
use crate::voxel_loader::VoxelLoader;
use log::{debug, error, set_boxed_logger, warn};
use octa_force::anyhow::bail;
use octa_force::glam::{uvec3, UVec3};
use octa_force::{
    anyhow::Result,
    glam::{ivec3, BVec3, IVec3, Mat3, Mat4},
};
use std::ops::Deref;

const HULL_CACHE_NONE: CacheIndex = CacheIndex::MAX;
const HULL_BLOCK_NAME: &str = "Hull";
const HULL_BASE_NAME_PART: &str = "Hull-Base";
const HULL_MULTI_NAME_PART: &str = "Hull-Multi";
const HULL_MULTI_BLOCK: &str = "Block";
const HULL_MULTI_REQ: &str = "Req";
const BLOCK_MODEL_IDENTIFIER: &str = "B";
const FOLDER_MODEL_IDENTIFIER: &str = "F";

pub struct HullSolver {
    pub block_name_index: usize,
    pub basic_blocks: Vec<(Vec<IVec3>, Block, Prio)>,
    pub multi_blocks: Vec<(Vec<(IVec3, Vec<Block>)>, Block, Prio)>,

    #[cfg(debug_assertions)]
    pub debug_basic_blocks: Vec<(Vec<IVec3>, Block, Prio)>,

    #[cfg(debug_assertions)]
    pub debug_multi_blocks: Vec<(Vec<(IVec3, Vec<Block>)>, Block, Prio)>,
}

impl Rules {
    pub fn make_hull(&mut self, voxel_loader: &VoxelLoader) -> Result<()> {
        debug!("Making Hull");

        let hull_block_name_index = self.block_names.len();
        self.block_names.push(HULL_BLOCK_NAME.to_owned());

        let mut hull_solver = HullSolver {
            block_name_index: hull_block_name_index,
            basic_blocks: vec![],
            multi_blocks: vec![],

            #[cfg(debug_assertions)]
            debug_basic_blocks: vec![],

            #[cfg(debug_assertions)]
            debug_multi_blocks: vec![],
        };

        hull_solver.add_base_blocks(self, voxel_loader)?;
        hull_solver.add_multi_blocks(self, voxel_loader)?;

        self.solvers.push(Box::new(hull_solver));

        debug!("Making Hull Done");
        Ok(())
    }
}

impl Solver for HullSolver {
    fn block_check_reset(
        &self,
        ship: &mut ShipData,
        block_index: usize,
        chunk_index: usize,
        world_block_pos: IVec3,
    ) -> Vec<SolverCacheIndex> {
        let mut cache = vec![];
        cache.append(&mut self.get_basic_blocks(ship, world_block_pos));
        cache.append(&mut self.get_multi_blocks_reset(ship, world_block_pos));
        cache
    }

    fn debug_block_check_reset(
        &self,
        ship: &mut ShipData,
        block_index: usize,
        chunk_index: usize,
        world_block_pos: IVec3,
    ) -> Vec<(SolverCacheIndex, Vec<(IVec3, bool)>)> {
        self.get_multi_blocks_reset_debug(ship, world_block_pos)
    }

    fn block_check(
        &self,
        ship: &mut ShipData,
        chunk_index: usize,
        node_index: usize,
        world_block_pos: IVec3,
        cache: Vec<SolverCacheIndex>,
    ) -> Vec<SolverCacheIndex> {
        let mut new_cache = vec![];
        for index in cache {
            if index < self.basic_blocks.len() {
                new_cache.push(index);
            } else {
                if self.keep_multi_block(ship, world_block_pos, index) {
                    new_cache.push(index);
                }
            }
        }

        new_cache
    }

    fn debug_block_check(
        &self,
        ship: &mut ShipData,
        block_index: usize,
        chunk_index: usize,
        world_block_pos: IVec3,
        blocks: &[PossibleBlocks],
    ) -> Vec<(SolverCacheIndex, Vec<(IVec3, bool)>)> {
        let mut new_cache = vec![];
        let cache = blocks[block_index]
            .to_owned()
            .get_cache(self.block_name_index)
            .to_owned();
        for index in cache {
            if index < self.basic_blocks.len() {
                // new_cache.push((index, vec![]));
            } else {
                let req_result = self.keep_multi_block_debug(ship, world_block_pos, index, blocks);
                new_cache.push((index, req_result));
            }
        }

        new_cache
    }

    fn get_block(
        &self,
        ship: &mut ShipData,
        block_index: usize,
        chunk_index: usize,
        world_block_pos: IVec3,
        cache: Vec<SolverCacheIndex>,
    ) -> (Block, Prio, usize) {
        let mut best_block = Block::from_single_node_id(NodeID::empty());
        let mut best_prio = Prio::EMPTY;
        let mut best_index = 0;

        for index in cache {
            if index < self.basic_blocks.len() {
                let (_, block, prio) = &self.basic_blocks[index];
                if best_prio < *prio {
                    best_block = *block;
                    best_prio = *prio;
                    best_index = index;
                }
            } else {
                let (_, block, prio) = &self.multi_blocks[index - self.basic_blocks.len()];
                if best_prio < *prio {
                    best_block = *block;
                    best_prio = *prio;
                    best_index = index;
                }
            }
        }

        (best_block, best_prio, best_index)
    }

    fn get_block_from_cache_index(&self, index: usize) -> Block {
        return if index < self.basic_blocks.len() {
            self.basic_blocks[index].1
        } else {
            self.multi_blocks[index - self.basic_blocks.len()].1
        };
    }
}

impl HullSolver {
    fn add_base_blocks(&mut self, rules: &mut Rules, voxel_loader: &VoxelLoader) -> Result<()> {
        let hull_reqs = vec![(vec![], HULL_BASE)];

        let mut base_blocks = vec![];
        for (i, (req, prio)) in hull_reqs.into_iter().enumerate() {
            let block = rules
                .load_block_from_node_folder(&format!("{HULL_BASE_NAME_PART}-{i}"), voxel_loader)?;

            base_blocks.push((req, block, prio));
        }

        let mut rotated_base_blocks = permutate_base_blocks(&base_blocks, rules);
        self.basic_blocks.append(&mut rotated_base_blocks);

        #[cfg(debug_assertions)]
        self.debug_basic_blocks.append(&mut base_blocks);

        Ok(())
    }

    fn add_multi_blocks(&mut self, rules: &mut Rules, voxel_loader: &VoxelLoader) -> Result<()> {
        let mut multi_blocks: Vec<(Vec<(IVec3, Vec<Block>)>, Block, Prio)> = vec![];

        let num = 2;
        for i in 0..num {
            let mut blocks = vec![];
            let mut req_blocks = vec![];

            let (models, rot) =
                voxel_loader.get_name_folder(&format!("{HULL_MULTI_NAME_PART}-{i}"))?;

            if rot != Rot::IDENTITY {
                bail!("Multi Block Rot should be IDENTITY");
            }

            for (name, index, rot, pos) in models {
                let name_parts: Vec<_> = name.split('-').collect();

                let block = if name_parts[1] == BLOCK_MODEL_IDENTIFIER {
                    rules.load_block_from_block_model_by_index(index, voxel_loader)?
                } else if name_parts[1] == FOLDER_MODEL_IDENTIFIER {
                    rules.load_block_from_node_folder(&name, voxel_loader)?
                } else {
                    bail!("Part 1 of {name} is not identified.");
                };
                let block = block.rotate(rot, rules);

                if name_parts[0] == HULL_MULTI_BLOCK {
                    let prio = name_parts[2].parse::<usize>()?;
                    blocks.push((block, pos, Prio::HULL_MULTI(prio)))
                } else if name_parts[0] == HULL_MULTI_REQ {
                    req_blocks.push((block, pos))
                } else {
                    bail!("Part 0 of {name} is not identified.");
                }
            }

            for (block, pos, prio) in blocks.to_owned().into_iter() {
                let mut empty_reqs = vec![];
                let mut add = false;
                let reqs = multi_blocks
                    .iter_mut()
                    .find_map(|(reqs, test_block, _)| {
                        if *test_block == block {
                            Some(reqs)
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| {
                        add = true;
                        &mut empty_reqs
                    });

                for offset in get_neighbors() {
                    let neighbor_pos = pos + offset * 8;

                    for (block, test_pos) in req_blocks.to_owned().into_iter().chain(
                        blocks
                            .to_owned()
                            .into_iter()
                            .map(|(block, pos, _)| (block.to_owned(), pos.to_owned())),
                    ) {
                        if neighbor_pos == test_pos {
                            let blocks = reqs.iter_mut().find_map(|(test_offset, blocks)| {
                                if *test_offset == offset {
                                    Some(blocks)
                                } else {
                                    None
                                }
                            });

                            if blocks.is_some() {
                                blocks.unwrap().push(block);
                            } else {
                                reqs.push((offset, vec![block]));
                            }
                        }
                    }
                }

                if add {
                    multi_blocks.push((empty_reqs, block, prio))
                }
            }
        }

        let mut rotated_multi_blocks = permutate_multi_blocks(&multi_blocks, rules);
        self.multi_blocks.append(&mut rotated_multi_blocks);

        #[cfg(debug_assertions)]
        self.debug_multi_blocks.append(&mut multi_blocks);

        Ok(())
    }

    fn get_basic_blocks(
        &self,
        ship: &mut ShipData,
        world_block_pos: IVec3,
    ) -> Vec<SolverCacheIndex> {
        let block_name_index = ship.get_block_name_from_world_block_pos(world_block_pos);
        if block_name_index != self.block_name_index {
            return vec![];
        }

        let mut best_block_index = None;
        let mut best_prio = Prio::ZERO;

        for (i, (reqs, _, prio)) in self.basic_blocks.iter().enumerate() {
            let mut pass = true;
            for offset in reqs {
                let req_world_block_pos = world_block_pos + *offset;
                let block_name_index =
                    ship.get_block_name_from_world_block_pos(req_world_block_pos);

                if block_name_index != self.block_name_index {
                    pass = false;
                    break;
                }
            }

            if pass && best_prio < *prio {
                best_block_index = Some(i);
                best_prio = *prio;
            }
        }

        return if best_block_index.is_some() {
            vec![best_block_index.unwrap()]
        } else {
            vec![]
        };
    }

    fn get_multi_blocks_reset(
        &self,
        ship: &mut ShipData,
        world_block_pos: IVec3,
    ) -> Vec<SolverCacheIndex> {
        let mut cache = vec![];
        for (i, (reqs, _, _)) in self.multi_blocks.iter().enumerate() {
            let block_name_index = ship.get_block_name_from_world_block_pos(world_block_pos);
            let mut pass = block_name_index == self.block_name_index;

            if pass {
                for (req_pos, req_blocks) in reqs {
                    let req_world_block_pos = world_block_pos + *req_pos;
                    let block_name_index =
                        ship.get_block_name_from_world_block_pos(req_world_block_pos);

                    let mut ok = false;
                    for req_block in req_blocks {
                        let req_empty = *req_block == Block::from_single_node_id(NodeID::empty());
                        //let req_base = *req_block == self.basic_blocks[0].1;

                        if !req_empty && block_name_index == self.block_name_index {
                            ok = true;
                            break;
                        }
                        if req_empty && block_name_index == EMPTY_BLOCK_NAME_INDEX {
                            ok = true;
                            break;
                        }
                    }

                    if !ok {
                        pass = false;
                        break;
                    }
                }
            }

            if pass {
                cache.push(i + self.basic_blocks.len())
            }
        }

        cache
    }

    fn get_multi_blocks_reset_debug(
        &self,
        ship: &mut ShipData,
        world_block_pos: IVec3,
    ) -> Vec<(SolverCacheIndex, Vec<(IVec3, bool)>)> {
        let mut cache = vec![];
        for (i, (reqs, _, _)) in self.multi_blocks.iter().enumerate() {
            let mut req_results = vec![];
            for (req_pos, req_blocks) in reqs {
                let req_world_block_pos = world_block_pos + *req_pos;
                let block_name_index =
                    ship.get_block_name_from_world_block_pos(req_world_block_pos);

                let mut ok = false;
                for req_block in req_blocks {
                    let req_empty = *req_block == Block::from_single_node_id(NodeID::empty());
                    //let req_base = *req_block == self.basic_blocks[0].1;

                    if !req_empty && block_name_index == self.block_name_index {
                        ok = true;
                        break;
                    }
                    if req_empty && block_name_index == EMPTY_BLOCK_NAME_INDEX {
                        ok = true;
                        break;
                    }
                }

                req_results.push((req_world_block_pos, ok))
            }

            cache.push((i + self.basic_blocks.len(), req_results))
        }

        cache
    }

    fn keep_multi_block(
        &self,
        ship: &mut ShipData,
        world_block_pos: IVec3,
        cache_index: CacheIndex,
    ) -> bool {
        let (reqs, _, _) = &self.multi_blocks[cache_index - self.basic_blocks.len()];

        let mut pass = true;
        for (req_pos, req_blocks) in reqs {
            let req_world_block_pos = world_block_pos + *req_pos;
            let cache =
                ship.get_cache_from_world_block_pos(req_world_block_pos, self.block_name_index);

            let mut ok = false;
            'iter: for req_block in req_blocks {
                if *req_block == Block::from_single_node_id(NodeID::empty()) {
                    ok = true;
                    break 'iter;
                }

                for index in cache.iter() {
                    let test_block = self.get_block_from_cache_index(*index);

                    if *req_block == test_block {
                        ok = true;
                        break 'iter;
                    }
                }
            }

            if !ok {
                pass = false;
                break;
            }
        }

        pass
    }

    fn keep_multi_block_debug(
        &self,
        ship: &mut ShipData,
        world_block_pos: IVec3,
        cache_index: CacheIndex,
        blocks: &[PossibleBlocks],
    ) -> Vec<(IVec3, bool)> {
        let (reqs, _, _) = &self.multi_blocks[cache_index - self.basic_blocks.len()];

        let mut reqs_result = vec![];
        for (req_pos, req_blocks) in reqs {
            let req_world_block_pos = world_block_pos + *req_pos;
            let in_chunk_block_index =
                ship.get_block_index_from_world_block_pos(req_world_block_pos);
            let cache = blocks[in_chunk_block_index]
                .to_owned()
                .get_cache(self.block_name_index)
                .to_owned();

            let mut ok = false;
            'iter: for req_block in req_blocks {
                if *req_block == Block::from_single_node_id(NodeID::empty()) {
                    ok = true;
                    break 'iter;
                }

                for index in cache.iter() {
                    let test_block = self.get_block_from_cache_index(*index);

                    if *req_block == test_block {
                        ok = true;
                        break 'iter;
                    }
                }
            }

            reqs_result.push((req_world_block_pos, ok))
        }

        reqs_result
    }
}

fn permutate_base_blocks(
    blocks: &[(Vec<IVec3>, Block, Prio)],
    rules: &mut Rules,
) -> Vec<(Vec<IVec3>, Block, Prio)> {
    let mut rotated_blocks = vec![];
    for (reqs, block, prio) in blocks.iter() {
        for rot in Rot::IDENTITY.get_all_permutations() {
            let mat: Mat4 = rot.into();
            let rotated_reqs: Vec<_> = reqs
                .iter()
                .map(|req| mat.transform_vector3((*req).as_vec3()).round().as_ivec3())
                .collect();

            let rotated_block = block.rotate(rot, rules);

            let mut found = false;
            for (_, test_block, _) in rotated_blocks.iter() {
                if *test_block == rotated_block {
                    found = true;
                    break;
                }
            }

            if !found {
                rotated_blocks.push((rotated_reqs, rotated_block, *prio))
            }
        }
    }

    rotated_blocks
}

fn permutate_multi_blocks(
    blocks: &[(Vec<(IVec3, Vec<Block>)>, Block, Prio)],
    rules: &mut Rules,
) -> Vec<(Vec<(IVec3, Vec<Block>)>, Block, Prio)> {
    let mut rotated_blocks = vec![];
    for (reqs, block, prio) in blocks.iter() {
        for rot in Rot::IDENTITY.get_all_permutations() {
            let mat: Mat4 = rot.into();
            let rotated_reqs: Vec<_> = reqs
                .iter()
                .map(|(req_pos, req_blocks)| {
                    let rotated_pos = mat
                        .transform_vector3((*req_pos).as_vec3())
                        .round()
                        .as_ivec3();
                    let rotated_blocks = req_blocks.iter().map(|b| b.rotate(rot, rules)).collect();
                    (rotated_pos, rotated_blocks)
                })
                .collect();

            let rotated_block = block.rotate(rot, rules);

            let mut found = false;
            for (_, test_block, _) in rotated_blocks.iter() {
                if *test_block == rotated_block {
                    found = true;
                    break;
                }
            }

            if !found {
                rotated_blocks.push((rotated_reqs, rotated_block, *prio))
            }
        }
    }

    rotated_blocks
}
