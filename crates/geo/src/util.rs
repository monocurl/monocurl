use crate::mesh::Mesh;
// MARK: query
impl Mesh {
    pub fn rank(&self) -> usize {
        if !self.tris.is_empty() {
            3
        }
        else if !self.lins.is_empty() {
            2
        }
        else if !self.dots.is_empty() {
            1
        }
        else {
            0
        }
    }

    pub fn bounding_box(&self) {}
}

// MARK: point / geometry
impl Mesh {
    pub fn point_transformation() {}

    pub fn linear_transformation() {}

    pub fn translate() {}

    pub fn shift() {}

    pub fn scale() {}

    pub fn rotate() {}

    pub fn extrude() {

    }

    pub fn revolve() {

    }

    pub fn contour_separate() {}

    // pain
    pub fn tesselate() {}

    // pain
    pub fn uprank() {}

    pub fn downrank() {}
}

// MARK: color / visuals
impl Mesh {
}
