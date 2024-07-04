use crate::math::get_neighbors_without_zero;
use crate::node::{NodeID, NODE_INDEX_EMPTY};
use crate::rotation::Rot;
use crate::rules::block::{Block, BlockIndex, BlockNameIndex};
use crate::rules::req_tree::BroadReqTree;
use crate::rules::solver::SolverCacheIndex;
use crate::rules::{
    Prio, Rules, BLOCK_MODEL_IDENTIFIER, BLOCK_TYPE_IDENTIFIER, FOLDER_MODEL_IDENTIFIER,
    REQ_TYPE_IDENTIFIER,
};
use crate::ship::data::ShipData;
use crate::voxel_loader::VoxelLoader;
use log::{debug, info};
use octa_force::anyhow::{bail, Result};
use octa_force::glam::{IVec3, Mat4};
use octa_force::puffin_egui::puffin;

#[derive(Clone, Debug)]
pub struct BasicBlocks {
    blocks: Vec<(Vec<(IVec3, BlockNameIndex)>, Block)>,

    #[cfg(debug_assertions)]
    pub debug_basic_blocks: Vec<(Vec<(IVec3, BlockNameIndex)>, Block)>,
}

impl BasicBlocks {
    pub fn new(
        rules: &mut Rules,
        voxel_loader: &VoxelLoader,
        folder_name_part: &str,
        folder_amount: usize,
    ) -> Result<Self> {
        let mut basic_blocks: Vec<(Vec<(IVec3, BlockNameIndex)>, Block)> = vec![];

        for i in 0..folder_amount {
            let (blocks, req_blocks) = load_basic_block_req_folder(
                &format!("{folder_name_part}-{i}"),
                voxel_loader,
                rules,
            )?;

            for (block, pos) in blocks.to_owned().into_iter() {
                let mut reqs = vec![];

                for offset in get_neighbors_without_zero() {
                    let neighbor_pos = pos + offset * 8;

                    for (block_name_index, test_pos) in req_blocks.to_owned() {
                        if neighbor_pos == test_pos {
                            reqs.push((offset, block_name_index))
                        }
                    }
                }

                basic_blocks.push((reqs, block))
            }
        }

        let mut rotated_basic_blocks = permutate_basic_blocks(&basic_blocks, rules);

        Ok(BasicBlocks {
            blocks: rotated_basic_blocks,
            #[cfg(debug_assertions)]
            debug_basic_blocks: basic_blocks,
        })
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn has_index(&self, index: usize) -> bool {
        index < self.blocks.len()
    }

    pub fn get_block(&self, index: usize) -> &(Vec<(IVec3, BlockNameIndex)>, Block) {
        &self.blocks[index]
    }

    pub fn get_possible_blocks(
        &self,
        ship: &mut ShipData,
        world_block_pos: IVec3,
        block_name_index: BlockIndex,
    ) -> Vec<SolverCacheIndex> {
        #[cfg(debug_assertions)]
        puffin::profile_function!();

        let test_block_name_index = ship.get_block_name_from_world_block_pos(world_block_pos);
        if test_block_name_index != block_name_index {
            return vec![];
        }

        for (i, (reqs, _)) in self.blocks.iter().enumerate() {
            let mut pass = true;
            for (offset, block_name_index) in reqs {
                let req_world_block_pos = world_block_pos + *offset;
                let test_block_name_index =
                    ship.get_block_name_from_world_block_pos(req_world_block_pos);

                if test_block_name_index != *block_name_index {
                    pass = false;
                    break;
                }
            }

            if pass {
                return vec![i];
            }
        }

        return vec![];
    }
}

fn load_basic_block_req_folder(
    folder_name: &str,
    voxel_loader: &VoxelLoader,
    rules: &mut Rules,
) -> Result<(Vec<(Block, IVec3)>, Vec<(usize, IVec3)>)> {
    let mut blocks = vec![];
    let mut req_blocks = vec![];

    let (models, rot) = voxel_loader.get_name_folder(folder_name)?;

    if rot != Rot::IDENTITY {
        bail!("Block Req Folder {} Rot should be IDENTITY", folder_name);
    }

    for (name, index, rot, pos) in models {
        let name_parts: Vec<_> = name.split('-').collect();

        if name_parts[0] == BLOCK_TYPE_IDENTIFIER {
            let block = if name_parts[1] == BLOCK_MODEL_IDENTIFIER {
                rules.load_block_from_block_model_by_index(index, voxel_loader)?
            } else if name_parts[1] == FOLDER_MODEL_IDENTIFIER {
                rules.load_block_from_node_folder(&name, voxel_loader)?
            } else {
                bail!("Part 1 of {name} is not identified.");
            };
            let block = block.rotate(rot, rules);

            blocks.push((block, pos))
        } else {
            let req_block_name = name_parts[0];
            let index = rules
                .block_names
                .iter()
                .position(|block_name| block_name == req_block_name);
            if index.is_none() {
                bail!("{req_block_name} is not a valid Block name!");
            }

            req_blocks.push((index.unwrap(), pos))
        }
    }

    Ok((blocks, req_blocks))
}

fn permutate_basic_blocks(
    blocks: &[(Vec<(IVec3, BlockNameIndex)>, Block)],
    rules: &mut Rules,
) -> Vec<(Vec<(IVec3, BlockNameIndex)>, Block)> {
    let mut rotated_blocks = vec![];
    for (reqs, block) in blocks.iter() {
        for rot in Rot::IDENTITY.get_all_permutations() {
            let mat: Mat4 = rot.into();
            let rotated_reqs: Vec<_> = reqs
                .iter()
                .map(|(offset, block_name_index)| {
                    (
                        mat.transform_vector3((*offset).as_vec3())
                            .round()
                            .as_ivec3(),
                        *block_name_index,
                    )
                })
                .collect();

            let rotated_block = block.rotate(rot, rules);

            let mut found = false;
            for (_, test_block) in rotated_blocks.iter() {
                if *test_block == rotated_block {
                    found = true;
                    break;
                }
            }

            if !found {
                rotated_blocks.push((rotated_reqs, rotated_block))
            }
        }
    }

    rotated_blocks
}
