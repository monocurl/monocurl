use geo::mesh::Mesh;

pub struct PrimitiveMesh {
    pub mesh: Mesh,
    hash_cache: Option<usize>,
}
