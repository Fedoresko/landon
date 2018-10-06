//! Blender files can have meshes such as circles, cubes, cylinders, a dragon or any other
//! 3D shape.
//!
//! A mesh can be represented as a group of vertices and data about those vertices, such as their
//! normals or UV coordinates.
//!
//! Meshes can also have metadata, such as the name of it's parent armature (useful for vertex
//! skinning).
//!
//! blender-mesh-to-json seeks to be a well tested, well documented exporter for blender mesh
//! metadata.
//!
//! You can write data to stdout or to a file. At the onset it will be geared towards @chinedufn's
//! needs - but if you have needs that aren't met feel very free to open an issue.
//!
//! @see https://docs.blender.org/manual/en/dev/modeling/meshes/introduction.html - Mesh Introduction
//! @see https://github.com/chinedufn/blender-actions-to-json - Exporting blender armatures / actions

#[macro_use]
extern crate failure;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;

use serde_json::Error;
use std::cmp::max;
use std::collections::HashMap;
use std::collections::HashSet;

/// Something went wrong in the Blender child process that was trying to parse your mesh data.
#[derive(Debug, Fail)]
pub enum BlenderError {
    /// Errors in Blender are written to stderr. We capture the stderr from the `blender` child
    /// process that we spawned when attempting to export meshes from a `.blend` file.
    #[fail(
        display = "There was an issue while exporting meshes: Blender stderr output: {}",
        _0
    )]
    Stderr(String),
}

/// All of the data about a Blender mesh
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(Default))]
pub struct BlenderMesh {
    /// All of the mesh's vertices. Three items in the vector make one vertex.
    /// So indices 0, 1 and 2 are a vertex, 3, 4 and 5 are a vertex.. etc.
    /// [v1x, v1y, v1z, v2x, v2y, v2z, ...]
    pub vertex_positions: Vec<f32>,
    /// The indices within vertex positions that make up each triangle in our mesh.
    /// Three vertex position indices correspond to one triangle
    /// [0, 1, 2, 0, 2, 3, ...]
    pub vertex_position_indices: Vec<u16>,
    /// TODO: enum..? if they're all equal we replace the MyEnum::PerVertex(Vec<u8>) with MyEnum::Equal(4)
    pub num_vertices_in_each_face: Vec<u8>,
    pub vertex_normals: Vec<f32>,
    pub vertex_normal_indices: Option<Vec<u16>>,
    /// If your mesh is textured these will be all of the mesh's vertices' uv coordinates.
    /// Every vertex has two UV coordinates.
    /// [v1s, v1t, v2s, v2t, v3s, v3t]
    /// TODO: Combine vertex_uvs, vertex_uv_indices, texture_name into texture_info
    pub vertex_uvs: Option<Vec<f32>>,
    pub vertex_uv_indices: Option<Vec<u16>>,
    pub texture_name: Option<String>,
    pub armature_name: Option<String>,
    /// TODO: When we move to single index triangulate and add new vertices give those vertices the same group indices / weights
    /// TODO: A function that trims this down to `n` weights and indices per vertex. Similar to our
    /// triangulate function
    /// TODO: Make sure that when we combine vertex indices we expand our group weights
    pub vertex_group_indices: Option<Vec<u8>>,
    pub vertex_group_weights: Option<Vec<f32>>,
    /// TODO: enum..? if they're all equal we replace the MyEnum::PerVertex(Vec<u8>) with MyEnum::Equal(4)
    pub num_groups_for_each_vertex: Option<Vec<u8>>, // TODO: textures: HashMap<TextureNameString, {uvs, uv_indices}>
}

impl BlenderMesh {
    pub fn from_json(json_str: &str) -> Result<BlenderMesh, Error> {
        serde_json::from_str(json_str)
    }
}

mod combine_indices;

impl BlenderMesh {
    /// When exporting a mesh from Blender, faces will usually have 4 vertices (quad) but some
    /// faces might have 3 (triangle).
    ///
    /// We read `self.num_vertices_in_each_face` to check how
    /// many vertices each face has.
    ///
    /// If a face has 4 vertices we convert it into two triangles, each with 3 vertices.
    ///
    /// # Panics
    ///
    /// Panics if a face has more than 4 vertices. In the future we might support 5+ vertices,
    /// but I haven't run into that yet. Not even sure if Blender can have faces with 5 vertices..
    pub fn triangulate(&mut self) {
        let mut triangulated_position_indices = vec![];
        let mut triangulated_face_vertex_counts = vec![];

        let mut face_pointer = 0;

        for num_verts_in_face in self.num_vertices_in_each_face.iter() {
            match num_verts_in_face {
                &3 => {
                    triangulated_face_vertex_counts.push(3);

                    triangulated_position_indices.push(self.vertex_position_indices[face_pointer]);
                    triangulated_position_indices
                        .push(self.vertex_position_indices[face_pointer + 1]);
                    triangulated_position_indices
                        .push(self.vertex_position_indices[face_pointer + 2]);

                    face_pointer += 3;
                }
                &4 => {
                    triangulated_face_vertex_counts.push(3);
                    triangulated_face_vertex_counts.push(3);

                    triangulated_position_indices.push(self.vertex_position_indices[face_pointer]);
                    triangulated_position_indices
                        .push(self.vertex_position_indices[face_pointer + 1]);
                    triangulated_position_indices
                        .push(self.vertex_position_indices[face_pointer + 2]);
                    triangulated_position_indices.push(self.vertex_position_indices[face_pointer]);
                    triangulated_position_indices
                        .push(self.vertex_position_indices[face_pointer + 2]);
                    triangulated_position_indices
                        .push(self.vertex_position_indices[face_pointer + 3]);

                    face_pointer += 4;
                }
                _ => {
                    panic!("blender-mesh currently only supports triangulating faces with 3 or 4 vertices");
                }
            }
        }

        self.vertex_position_indices = triangulated_position_indices;
        self.num_vertices_in_each_face = triangulated_face_vertex_counts;
    }
}

impl BlenderMesh {
    /// Blender meshes get exported with a Z up coordinate system.
    /// Here we flip our coordinate system to be y up
    ///
    /// @see https://gamedev.stackexchange.com/a/7932
    ///
    /// TODO: When we have bone data we'll need to change them to port change-mat4-coordinate-system
    /// into here.
    /// https://github.com/chinedufn/change-mat4-coordinate-system/blob/master/change-mat4-coordinate-system.js
    pub fn y_up(&mut self) {
        for vert_num in 0..(self.vertex_positions.len() / 3) {
            let y_index = vert_num * 3 + 1;
            let z_index = y_index + 1;

            let new_z = -self.vertex_positions[y_index];
            self.vertex_positions[y_index] = self.vertex_positions[z_index];
            self.vertex_positions[z_index] = new_z;

            let new_z = -self.vertex_normals[y_index];
            self.vertex_normals[y_index] = self.vertex_normals[z_index];
            self.vertex_normals[z_index] = new_z;
        }
    }
}

impl BlenderMesh {
    /// Different vertices might have different numbers of bones that influence them.
    /// A vertex near the shoulder might be influenced by the neck and upper arm and sternum,
    /// while a vertex in a toe might only be influenced by a toe bone.
    ///
    /// When passing data to the GPU, each vertex needs the same number of bone attributes, so
    /// we must add/remove bones from each vertex to get them equal.
    ///
    /// Say we're setting 3 groups per vertex:
    ///  - If a vertex has one vertex group (bone) we will create two fake bones with 0.0 weight.
    ///  - If a vertex has 5 bones we'll remove the one with the smallest weighting (influence).
    pub fn set_groups_per_vertex(&mut self, count: u8) {
        let mut normalized_group_indices = vec![];
        let mut normalized_group_weights = vec![];

        let mut current_index: u32 = 0;

        {
            let mut indices = self.vertex_group_indices.as_mut().unwrap();
            let weights = self.vertex_group_weights.as_mut().unwrap();

            self.num_groups_for_each_vertex = Some(
                self.num_groups_for_each_vertex
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|group_count| {
                        let mut vertex_indices = vec![];
                        let mut vertex_weights = vec![];

                        for index in current_index..(current_index + *group_count as u32) {
                            vertex_indices.push(index);
                            vertex_weights.push(weights[index as usize]);
                        }

                        vertex_weights.sort_by(|a, b| b.partial_cmp(a).unwrap());
                        vertex_indices.sort_by(|a, b| {
                            weights[*b as usize]
                                .partial_cmp(&weights[*a as usize])
                                .unwrap()
                        });

                        let mut vertex_indices: Vec<u8> = vertex_indices
                            .iter()
                            .map(|i| indices[*i as usize])
                            .collect();

                        vertex_indices.resize(count as usize, 0);
                        vertex_weights.resize(count as usize, 0.0);

                        normalized_group_indices.append(&mut vertex_indices);
                        normalized_group_weights.append(&mut vertex_weights);

                        current_index += *group_count as u32;
                        count
                    }).collect(),
            );
        }

        self.vertex_group_indices = Some(normalized_group_indices);
        self.vertex_group_weights = Some(normalized_group_weights);
    }
}

pub type MeshNamesToData = HashMap<String, BlenderMesh>;
pub type FilenamesToMeshes = HashMap<String, MeshNamesToData>;

/// Given a buffer of standard output from Blender we parse all of the mesh JSON that was
/// written to stdout by `blender-mesh-to-json.py`.
///
/// Meshes data in stdout will look like:
///
/// START_MESH_JSON /path/to/file.blend my_mesh_name
/// {...}
/// END_MESH_JSON /path/to/file.blend my_mesh_name
///
/// @see blender-mesh-to-json.py - This is where we write to stdout
pub fn parse_meshes_from_blender_stdout(
    blender_stdout: &str,
) -> Result<FilenamesToMeshes, failure::Error> {
    let mut filenames_to_meshes = HashMap::new();

    let mut index = 0;

    while let Some((filename_to_mesh, next_start_index)) =
        find_first_mesh_after_index(blender_stdout, index)
    {
        filenames_to_meshes.extend(filename_to_mesh);
        index = next_start_index;
    }

    Ok(filenames_to_meshes)
}

fn find_first_mesh_after_index(
    blender_stdout: &str,
    index: usize,
) -> Option<(FilenamesToMeshes, usize)> {
    let blender_stdout = &blender_stdout[index as usize..];

    if let Some(mesh_start_index) = blender_stdout.find("START_MESH_JSON") {
        let mut filenames_to_meshes = HashMap::new();
        let mut mesh_name_to_data = HashMap::new();

        let mesh_end_index = blender_stdout.find("END_MESH_JSON").unwrap();

        let mesh_data = &blender_stdout[mesh_start_index..mesh_end_index];

        let mut lines = mesh_data.lines();

        let first_line = lines.next().unwrap();

        let mesh_filename: Vec<&str> = first_line.split(" ").collect();
        let mesh_filename = mesh_filename[1].to_string();

        let mesh_name = first_line.split(" ").last().unwrap().to_string();

        let mesh_data: String = lines.collect();
        let mesh_data: BlenderMesh = serde_json::from_str(&mesh_data).unwrap();

        mesh_name_to_data.insert(mesh_name, mesh_data);
        filenames_to_meshes.insert(mesh_filename, mesh_name_to_data);

        return Some((filenames_to_meshes, index + mesh_end_index + 1));
    }

    return None;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Concatenate a series of vectors into one vector
    macro_rules! concat_vecs {
        ( $( $vec:expr),* ) => {
            {
                let mut concatenated_vec = Vec::new();
                $(
                    concatenated_vec.append(&mut $vec.clone());
                )*
                concatenated_vec
            }
        }
    }

    struct CombineIndicesTest {
        mesh_to_combine: BlenderMesh,
        expected_combined_mesh: BlenderMesh,
    }

    fn test_combine_indices(mut combine_indices_test: CombineIndicesTest) {
        combine_indices_test
            .mesh_to_combine
            .combine_vertex_indices();
        let combined_mesh = combine_indices_test.mesh_to_combine;
        assert_eq!(combined_mesh, combine_indices_test.expected_combined_mesh);
    }

    fn make_mesh_to_combine_without_uvs() -> BlenderMesh {
        let start_positions = concat_vecs!(v(0), v(1), v(2), v(3));
        let start_normals = concat_vecs!(v(4), v(5), v(6));

        BlenderMesh {
            vertex_positions: start_positions,
            vertex_position_indices: vec![0, 1, 2, 3, 0, 1, 2, 3, 0, 1, 2, 3],
            num_vertices_in_each_face: vec![4, 4, 4],
            vertex_normals: start_normals,
            // Our last 4 vertices already exist so our expected mesh will generate
            // position indices 4, 5, 6 and 7 and use those for the second to last 4 and
            // then last 4 indices
            vertex_normal_indices: Some(vec![0, 1, 0, 1, 2, 2, 2, 2, 2, 2, 2, 2]),
            num_groups_for_each_vertex: Some(vec![3, 2, 5, 1]),
            vertex_group_indices: Some(vec![0, 1, 2, 0, 3, 4, 5, 6, 7, 8, 11]),
            vertex_group_weights: Some(vec![
                0.05, 0.8, 0.15, 0.5, 0.5, 0.1, 0.2, 0.2, 0.2, 0.3, 0.999,
            ]),
            ..BlenderMesh::default()
        }
    }

    fn make_expected_combined_mesh() -> BlenderMesh {
        let end_positions = concat_vecs!(v(0), v(1), v(2), v(3), v(0), v(1), v(2), v(3));
        let end_normals = concat_vecs!(v(4), v(5), v(4), v(5), v(6), v(6), v(6), v(6));

        BlenderMesh {
            vertex_positions: end_positions,
            vertex_position_indices: vec![0, 1, 2, 3, 4, 5, 6, 7, 4, 5, 6, 7],
            num_vertices_in_each_face: vec![4, 4, 4],
            vertex_normals: end_normals,
            num_groups_for_each_vertex: Some(vec![3, 2, 5, 1, 3, 2, 5, 1]),
            vertex_group_indices: Some(vec![
                0, 1, 2, 0, 3, 4, 5, 6, 7, 8, 11, 0, 1, 2, 0, 3, 4, 5, 6, 7, 8, 11,
            ]),
            vertex_group_weights: Some(vec![
                0.05, 0.8, 0.15, 0.5, 0.5, 0.1, 0.2, 0.2, 0.2, 0.3, 0.999, 0.05, 0.8, 0.15, 0.5,
                0.5, 0.1, 0.2, 0.2, 0.2, 0.3, 0.999,
            ]),
            ..BlenderMesh::default()
        }
    }

    #[test]
    fn combine_pos_norm_indices() {
        let mesh_to_combine = make_mesh_to_combine_without_uvs();
        let expected_combined_mesh = make_expected_combined_mesh();

        test_combine_indices(CombineIndicesTest {
            mesh_to_combine,
            expected_combined_mesh,
        });
    }

    #[test]
    fn combine_pos_norm_uv_indices() {
        // We create a mesh where our first three triangles have no repeating vertices
        // (across norms, uvs and positions) then our fourth triangle has all repeating vertices
        let mesh_to_combine = BlenderMesh {
            vertex_positions: concat_vecs!(v(0), v(1), v(2), v(3)),
            vertex_normals: concat_vecs!(v(4), v(5), v(6)),
            num_vertices_in_each_face: vec![4, 4, 4, 4],
            vertex_position_indices: concat_vecs!(
                vec![0, 1, 2, 3],
                vec![0, 1, 2, 3],
                vec![0, 1, 2, 3],
                vec![0, 1, 2, 3]
            ),
            vertex_normal_indices: Some(concat_vecs!(
                vec![0, 1, 0, 1],
                vec![2, 2, 2, 2],
                vec![2, 2, 2, 2],
                vec![2, 2, 2, 2]
            )),
            vertex_uvs: Some(concat_vecs!(v2(7), v2(8), v2(9), v2(10))),
            vertex_uv_indices: Some(concat_vecs!(
                vec![0, 1, 0, 1],
                vec![2, 2, 2, 2],
                vec![3, 3, 3, 3],
                vec![3, 3, 3, 3]
            )),
            // We already tested vertex group indices / weights about so not bothering setting up
            // more test data
            num_groups_for_each_vertex: None,
            vertex_group_indices: None,
            vertex_group_weights: None,
            ..BlenderMesh::default()
        };

        let expected_combined_mesh = BlenderMesh {
            vertex_positions: concat_vecs!(v3_x4(0, 1, 2, 3), v3_x4(0, 1, 2, 3), v3_x4(0, 1, 2, 3)),
            vertex_position_indices: concat_vecs![
                // First Triangle
                vec![0, 1, 2, 3,],
                // Second Triangle
                vec![4, 5, 6, 7],
                // Third Triangle
                vec![8, 9, 10, 11],
                // Fourth Triangle
                vec![8, 9, 10, 11]
            ],
            num_vertices_in_each_face: vec![4, 4, 4, 4],
            vertex_normals: concat_vecs!(v3_x4(4, 5, 4, 5), v3_x4(6, 6, 6, 6), v3_x4(6, 6, 6, 6)),
            vertex_uvs: Some(concat_vecs!(
                v2_x4(7, 8, 7, 8),
                v2_x4(9, 9, 9, 9),
                v2_x4(10, 10, 10, 10)
            )),
            ..BlenderMesh::default()
        };

        test_combine_indices(CombineIndicesTest {
            mesh_to_combine,
            expected_combined_mesh,
        });
    }

    #[test]
    fn triangulate_faces() {
        let mut start_mesh = BlenderMesh {
            vertex_position_indices: vec![0, 1, 2, 3, 4, 5, 6, 7],
            num_vertices_in_each_face: vec![4, 4],
            ..BlenderMesh::default()
        };

        start_mesh.triangulate();
        let triangulated_mesh = start_mesh;

        let expected_mesh = BlenderMesh {
            vertex_position_indices: vec![0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7],
            num_vertices_in_each_face: vec![3, 3, 3, 3],
            ..BlenderMesh::default()
        };

        assert_eq!(triangulated_mesh, expected_mesh);
    }

    #[test]
    fn z_up_to_y_up() {
        let mut start_mesh = BlenderMesh {
            vertex_positions: vec![0.0, 1.0, 2.0, 0.0, 1.0, 2.0],
            vertex_normals: vec![0.0, 1.0, 2.0, 0.0, 1.0, 2.0],
            ..BlenderMesh::default()
        };

        start_mesh.y_up();
        let y_up_mesh = start_mesh;

        let expected_mesh = BlenderMesh {
            vertex_positions: vec![0.0, 2.0, -1.0, 0.0, 2.0, -1.0],
            vertex_normals: vec![0.0, 2.0, -1.0, 0.0, 2.0, -1.0],
            ..BlenderMesh::default()
        };

        assert_eq!(y_up_mesh, expected_mesh);
    }

    #[test]
    fn set_joints_per_vert() {
        let mut start_mesh = BlenderMesh {
            vertex_group_indices: Some(vec![0, 2, 3, 4, 0, 1, 3, 2]),
            num_groups_for_each_vertex: Some(vec![1, 3, 4]),
            vertex_group_weights: Some(vec![1.0, 0.5, 0.2, 0.3, 0.6, 0.15, 0.1, 0.15]),
            ..BlenderMesh::default()
        };

        start_mesh.set_groups_per_vertex(3);
        let three_joints_per_vert = start_mesh;

        let expected_mesh = BlenderMesh {
            vertex_group_indices: Some(vec![0, 0, 0, 2, 4, 3, 0, 1, 2]),
            num_groups_for_each_vertex: Some(vec![3, 3, 3]),
            vertex_group_weights: Some(vec![1.0, 0.0, 0.0, 0.5, 0.3, 0.2, 0.6, 0.15, 0.15]),
            ..BlenderMesh::default()
        };

        assert_eq!(three_joints_per_vert, expected_mesh);
    }

    // Create a 3 dimensional vector with all three values the same.
    // Useful for quickly generating some fake vertex data.
    // v(0.0) -> vec![0.0, 0.0, 0.0]
    fn v(val: u8) -> Vec<f32> {
        vec![val as f32, val as f32, val as f32]
    }

    fn v2_x4(vert1: u8, vert2: u8, vert3: u8, vert4: u8) -> Vec<f32> {
        concat_vecs!(v2(vert1), v2(vert2), v2(vert3), v2(vert4))
    }

    fn v3_x4(v1: u8, v2: u8, v3: u8, v4: u8) -> Vec<f32> {
        concat_vecs!(v(v1), v(v2), v(v3), v(v4))
    }

    fn v2(val: u8) -> Vec<f32> {
        vec![val as f32, val as f32]
    }
}
