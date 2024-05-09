use crate::node::{BlockIndex, NodeID, NodeIndex, NODE_INDEX_NONE};
use crate::rotation::Rot;
use crate::{
    math::to_1d,
    node::{Material, Node, NODE_SIZE, NODE_VOXEL_LENGTH},
};
use dot_vox::{DotVoxData, Position, SceneNode};
use octa_force::egui::ahash::HashMap;
use octa_force::glam::{ivec3, IVec3, UVec3};
use octa_force::log::warn;
use octa_force::{
    anyhow::{bail, Result},
    glam::uvec3,
    log,
};

pub struct VoxelLoader {
    pub path: String,
    pub mats: [Material; 256],
    pub nodes: Vec<Node>,
    pub block_names: Vec<String>,
    pub node_positions: HashMap<UVec3, NodeID>,
    pub block_positions: HashMap<UVec3, BlockIndex>,
}

struct ModelInfo {
    pub nodes: Vec<(UVec3, NodeID)>,
}

impl VoxelLoader {
    pub fn new(path: &str) -> Result<VoxelLoader> {
        let r = dot_vox::load(path);
        let data = if r.is_err() {
            bail!("Could not load .vox file");
        } else {
            r.unwrap()
        };

        let mats = Self::load_materials(&data)?;
        let (nodes, model_infos) = Self::load_models(&data)?;
        let (block_names, block_positions) = Self::load_blocks(&data);
        let node_positions = Self::load_node_positions(&data);

        let voxel_loader = Self {
            path: path.to_owned(),
            mats,
            nodes,
            block_names,
            node_positions: Default::default(),
            block_positions: Default::default(),
        };

        Ok(voxel_loader)
    }

    fn load_materials(data: &DotVoxData) -> Result<[Material; 256]> {
        let mut mats = [Material::default(); 256];
        for (i, color) in data.palette.iter().enumerate() {
            mats[i] = color.into();
        }

        Ok(mats)
    }

    fn load_models(data: &DotVoxData) -> Result<(Vec<Node>, Vec<Vec<(UVec3, NodeID)>>)> {
        let mut nodes = Vec::new();
        let mut model_infos = Vec::new();

        for (model_id, model) in data.models.iter().enumerate() {
            let size = uvec3(model.size.x, model.size.y, model.size.z);
            if (size % 4) != UVec3::ZERO {
                warn!("Model size not multiple of 4");
                continue;
            }

            let mut model_nodes = Vec::new();
            for cx in 0..(size.x / 4) {
                for cy in 0..(size.y / 4) {
                    for cz in 0..(size.z / 4) {
                        let mut empty = true;
                        let mut voxels = [0; NODE_VOXEL_LENGTH];
                        for v in model.voxels.iter() {
                            let pos = uvec3(v.x as u32, v.y as u32, v.z as u32);
                            let c_pos = uvec3(cx, cy, cz);
                            if pos.cmple(c_pos * 4).any()
                                || pos.cmpge((c_pos + UVec3::ONE) * 4).any()
                            {
                                continue;
                            }

                            let n_pos = pos - c_pos * 4;

                            //let x = (NODE_SIZE.x - 1) - v.x as u32; // Flip x to match game axis system.
                            //let y = (NODE_SIZE.y - 1) - v.y as u32; // Flip x to match game axis system.

                            voxels[to_1d(n_pos, NODE_SIZE)] = v.i;

                            if v.i != 0 {
                                empty = false;
                            }
                        }

                        if !empty {
                            let node = Node::new(voxels);

                            let duplicate = node.search_duplicate_node(&nodes);

                            if duplicate.is_some() {
                                model_nodes.push((uvec3(cx, cy, cz), duplicate.unwrap()));
                            } else {
                                model_nodes.push((
                                    uvec3(cx, cy, cz),
                                    NodeID::new(nodes.len(), Rot::default()),
                                ));
                                nodes.push(node);
                            }
                        }
                    }
                }
            }

            model_infos.push(model_nodes);
        }

        Ok((nodes, model_infos))
    }

    fn get_group_ids(data: &DotVoxData, name: &str) -> (Vec<u32>, IVec3) {
        data.scenes
            .iter()
            .find_map(|n| match n {
                SceneNode::Transform {
                    attributes,
                    child,
                    frames,
                    ..
                } => {
                    if attributes.get("_name").is_some_and(|s| s == name) {
                        let children = match &data.scenes[*child as usize] {
                            SceneNode::Group { children, .. } => Some(children.to_owned()),
                            _ => None,
                        };

                        if children.is_none() {
                            return None;
                        }

                        let p = frames[0]
                            .position()
                            .unwrap_or(Position { x: 0, y: 0, z: 0 });

                        Some((children.unwrap(), ivec3(p.x, p.y, p.z)))
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .unwrap()
    }

    fn load_blocks(data: &DotVoxData) -> (Vec<String>, HashMap<UVec3, BlockIndex>) {
        let (block_ids, block_group_pos) = Self::get_group_ids(data, "blocks");

        let mut block_names = Vec::new();
        let mut block_positions = HashMap::default();
        for block_id in block_ids {
            match &data.scenes[block_id as usize] {
                SceneNode::Transform {
                    frames, attributes, ..
                } => {
                    let name = attributes.get("_name").unwrap();

                    let duplicate = block_names.iter().position(|n: &String| **n == *name);

                    let block_index = if duplicate.is_some() {
                        duplicate.unwrap()
                    } else {
                        let i = block_names.len();
                        block_names.push(name.to_owned());
                        i
                    };

                    let p = frames[0]
                        .position()
                        .unwrap_or(Position { x: 0, y: 0, z: 0 });
                    let pos = ivec3(p.x, p.y, p.z) + block_group_pos - IVec3::ONE * 4;

                    block_positions.insert(pos.as_uvec3(), block_index);
                }
                _ => {}
            }
        }

        (block_names, block_positions)
    }

    fn load_node_positions(data: &DotVoxData) -> HashMap<UVec3, NodeID> {
        let (node_ids, block_group_pos) = Self::get_group_ids(data, "nodes");

        let mut node_positions = HashMap::default();
        for node_id in node_ids {
            match &data.scenes[node_id as usize] {
                SceneNode::Transform {
                    frames, attributes, ..
                } => {
                    let p = frames[0]
                        .position()
                        .unwrap_or(Position { x: 0, y: 0, z: 0 });
                    let pos = ivec3(p.x, p.y, p.z) + block_group_pos - IVec3::ONE * 4;
                    
                    
                }
                _ => {}
            }
        }

        node_positions
    }
}
