#version 450


// General
#define DEBUG_STEPS false
#define NODE_SIZE 4
#define MAX_STEPS 25
#define RAY_POS_OFFSET 0.0001
#define BORDER_SIZE 0.01


// Out
layout(location = 0) out vec4 finalColor;


// UV input
layout(location = 0) in vec3 oUV;
#define POSITION oUV * NODE_SIZE


// ID input
layout(location = 1) flat in uint oNodeId;
#define NODE_INDEX oNodeId >> 7


// Render buffer
layout(binding = 0) uniform RenderBuffer {
  mat4 proj_mat;
  mat4 view_mat;
  vec3 dir;
  vec2 size;
} renderbuffer;
#define DIRECTION renderbuffer.dir

// Voxels
struct Node {
    uint voxels[(NODE_SIZE * NODE_SIZE * NODE_SIZE) / 4];
};

layout(binding = 1) buffer Nodes {
    Node nodes[];
} nodes;
#define TO_INDEX(pos) ((pos.z * NODE_SIZE * NODE_SIZE) + (pos.y * NODE_SIZE) + pos.x)
#define GET_VOXEL(node, index) (node.voxels[index / 4] >> ((index % 4) * 8)) & 255


// Materials 
layout(binding = 2) buffer Mats {
    uint mats[];
} mats;
#define GET_MAT(mat) (vec4(float(mat & 255) / 255.0, float((mat >> 8) & 255) / 255.0, float((mat >> 16) & 255) / 255.0, float((mat >> 24) & 255) / 255.0))


// Ship type (Push constant)
layout(push_constant, std430) uniform PushConstant {
    uint ship_type;
} push_constant;
#define SHIP_TYPE push_constant.ship_type

// Debugging
vec3 getColorGradient(float x){
    if (x == 0){
        return vec3(0);
    }

    vec3 firstColor = vec3(0, 1, 0); // green
    vec3 middleColor = vec3(0, 0, 1); // blue
    vec3 endColor = vec3(1, 0, 0); // red

    float h = 0.5; // adjust position of middleColor
    vec3 col = mix(mix(firstColor, middleColor, x/h), mix(middleColor, endColor, (x - h)/(1.0 - h)), step(h, x));
    return col;
}


// Render
struct Ray{
    vec3 pos;
    vec3 dir;
    vec3 odir; 
};

struct Rot {
    mat4 mat;
    ivec3 offset;
};

Rot getRot() {
    uint index_nz1 = oNodeId & 3;
    uint index_nz2 = (oNodeId >> 2) & 3;
    uint index_nz3 = 3 - index_nz1 - index_nz2;

    int row_1_sign = (oNodeId & (1 << 4)) == 0 ? 1 : -1;
    int row_2_sign = (oNodeId & (1 << 5)) == 0 ? 1 : -1;
    int row_3_sign = (oNodeId & (1 << 6)) == 0 ? 1 : -1;

    mat4 mat = mat4(0);
    mat[index_nz1][0] = row_1_sign;
    mat[index_nz2][1] = row_2_sign;
    mat[index_nz3][2] = row_3_sign;

    Rot rot = Rot(mat, ivec3(row_1_sign == -1, row_2_sign == -1, row_3_sign == -1));
    return rot;
}

ivec3 applyRot(Rot rot, ivec3 v) {
    return ivec3(rot.mat * vec4(v, 1.0));    
}

bool checkHit(in Ray ray, in vec3 nodePos, in uint size, out float tMin, out float tMax) {
    vec3 minSize = nodePos;
    vec3 maxSize = nodePos + vec3(size);

    vec3 isPositive = vec3(ray.odir.x >= 0, ray.odir.y >= 0, ray.odir.z >= 0); 
    vec3 isNegative = 1.0f - isPositive;

    vec3 leftSide  = isPositive * minSize + isNegative * maxSize;
    vec3 rightSide = isPositive * maxSize + isNegative * minSize;

    vec3 leftSideTimesOneOverDir  = (leftSide  - ray.pos) * ray.odir;
    vec3 rightSideTimesOneOverDir = (rightSide - ray.pos) * ray.odir;

    tMin = max(leftSideTimesOneOverDir.x, max(leftSideTimesOneOverDir.y, leftSideTimesOneOverDir.z));
    tMax = min(rightSideTimesOneOverDir.x, min(rightSideTimesOneOverDir.y, rightSideTimesOneOverDir.z));

    // vec3 directionSign = sign(odir);
    // sideMin = vec3(leftSideTimesOneOverDir.x == tMin, leftSideTimesOneOverDir.y == tMin, leftSideTimesOneOverDir.z == tMin) * directionSign;
    // sideMax = vec3(rightSideTimesOneOverDir.x == tMax, rightSideTimesOneOverDir.y == tMax, rightSideTimesOneOverDir.z == tMax) * directionSign;

    return tMax > tMin;
}

bool hitBorder(in Ray ray){
    ivec3 pos = ivec3(ray.pos);
    vec3 lower = ray.pos - pos;
    vec3 upper = vec3(1.0) - lower;
    return (lower.x < BORDER_SIZE && lower.y < BORDER_SIZE) 
        || (lower.y < BORDER_SIZE && lower.z < BORDER_SIZE)
        || (lower.z < BORDER_SIZE && lower.x < BORDER_SIZE)
        || (upper.x < BORDER_SIZE && upper.y < BORDER_SIZE) 
        || (upper.y < BORDER_SIZE && upper.z < BORDER_SIZE)
        || (upper.z < BORDER_SIZE && upper.x < BORDER_SIZE)
        || (lower.x < BORDER_SIZE && upper.y < BORDER_SIZE)
        || (lower.y < BORDER_SIZE && upper.z < BORDER_SIZE)
        || (lower.z < BORDER_SIZE && upper.x < BORDER_SIZE)
        || (upper.x < BORDER_SIZE && lower.y < BORDER_SIZE)
        || (upper.y < BORDER_SIZE && lower.z < BORDER_SIZE)
        || (upper.z < BORDER_SIZE && lower.x < BORDER_SIZE);
}

vec4 voxelColor(in Ray ray, in uint voxel) {
    //return vec4(ray.pos, 1.0);

    if (hitBorder(ray)) {
            return vec4(0.0, 0.0, 0.0, 1.0);
        }

    uint mat = mats.mats[voxel];
    return GET_MAT(mat);
}

vec4 raycaster(in Ray ray, in Node node, in Rot rot){
    float tMin, tMax = 0;
    float rayLen = 0;
    ivec3 voxelPos = ivec3(0);
    ivec3 cellPos = ivec3(0);
    uint voxelIndex = 0;
    uint voxel = 0;
    ivec3 node_size_half = ivec3(NODE_SIZE / 2);

    if (!checkHit(ray, vec3(0), NODE_SIZE, tMin, tMax)) {
        return vec4(0);
    }
    ray.pos = ray.pos + ray.dir * (tMin + RAY_POS_OFFSET);  
    rayLen += tMin;   
    
    int counter = 1;
    while (counter < MAX_STEPS) {
        voxelPos = ivec3(ray.pos) - ivec3(ray.pos.x < 0, ray.pos.y < 0, ray.pos.z < 0);

        cellPos = voxelPos - node_size_half;
        cellPos = applyRot(rot, cellPos);
        cellPos += node_size_half - rot.offset;

        voxelIndex = TO_INDEX(cellPos);
        voxel = GET_VOXEL(node, voxelIndex);

        if (voxel != 0) {
            if (DEBUG_STEPS) {
                return vec4(getColorGradient(float(counter) / MAX_STEPS), 1.0);
            }

            return voxelColor(ray, voxel);
        }

        checkHit(ray, vec3(voxelPos), 1, tMin, tMax);                     
        ray.pos = ray.pos + ray.dir * (tMax + RAY_POS_OFFSET);              
        rayLen += tMax; 

        if (ray.pos.x < 0 || ray.pos.x >= NODE_SIZE || ray.pos.y < 0 || ray.pos.y >= NODE_SIZE || ray.pos.z < 0 || ray.pos.z >= NODE_SIZE){
            return vec4(0);
        }

        counter++;
    }

    return vec4(getColorGradient(1.0), 1.0);
}

void main() {
    vec2 uv = -((2 * gl_FragCoord.xy - renderbuffer.size.xy) / renderbuffer.size.x);

    vec3 ro = POSITION;
    vec3 fwd = DIRECTION;
    vec3 up = vec3(0.,0.,1.);
    vec3 right = normalize(cross(up, fwd));
    up = cross(fwd, right);
    vec3 rd = right * uv.x + up * uv.y + fwd;
    rd = normalize(rd);

    Ray ray = Ray(ro, rd, vec3(1) / rd);
    Node node = nodes.nodes[NODE_INDEX];
    Rot rot = getRot();

    float tMin, tMax;
    vec4 color = raycaster(ray, node, rot);

    if (SHIP_TYPE == 1) {
        color.w *= 0.5;
    }

    finalColor = color;

    if (color.w == 0) {
        gl_FragDepth = 1.0;
    } else {
        gl_FragDepth = gl_FragCoord.z;
    }
}
