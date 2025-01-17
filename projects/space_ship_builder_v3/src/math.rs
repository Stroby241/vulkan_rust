use octa_force::glam::{ivec3, uvec3, IVec3, UVec3};

pub fn to_1d(pos: UVec3, max: UVec3) -> usize {
    ((pos.z * max.x * max.y) + (pos.y * max.x) + pos.x) as usize
}

pub fn to_1d_i(pos: IVec3, max: IVec3) -> isize {
    ((pos.z * max.x * max.y) + (pos.y * max.x) + pos.x) as isize
}

pub fn to_3d(mut i: u32, max: UVec3) -> UVec3 {
    let z = i / (max.x * max.y);
    i -= z * max.x * max.y;
    let y = i / max.x;
    let x = i % max.x;
    uvec3(x, y, z)
}

pub fn to_3d_i(mut i: i32, max: IVec3) -> IVec3 {
    let z = i / (max.x * max.y);
    i -= z * max.x * max.y;
    let y = i / max.x;
    let x = i % max.x;
    ivec3(x, y, z)
}

pub fn get_neigbor_offsets() -> [IVec3; 6] {
    [
        ivec3(1, 0, 0),
        ivec3(0, 1, 0),
        ivec3(0, 0, 1),
        ivec3(-1, 0, 0),
        ivec3(0, -1, 0),
        ivec3(0, 0, -1),
    ]
}

pub const PACKED_WORD_SIZE: usize = 8;
pub fn get_packed_index(index: usize) -> (usize, u8) {
    (index / PACKED_WORD_SIZE, 1 << (index % PACKED_WORD_SIZE))
}
