use std::{collections::HashMap, path::PathBuf, sync::Arc};

use geo::mesh::Mesh;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum LatexBackendConfig {
    Bundled,
    System(SystemBackendConfig),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SystemBackendConfig {
    pub latex: PathBuf,
    pub dvisvgm: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemToolPaths {
    pub latex: Option<PathBuf>,
    pub dvisvgm: Option<PathBuf>,
}

impl SystemToolPaths {
    pub fn into_config(self) -> Option<SystemBackendConfig> {
        Some(SystemBackendConfig {
            latex: self.latex?,
            dvisvgm: self.dvisvgm?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SystemBackendStatus {
    pub latex: bool,
    pub dvisvgm: bool,
}

impl SystemBackendStatus {
    pub fn is_available(self) -> bool {
        self.latex && self.dvisvgm
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RenderQuality {
    Normal,
    High,
}

#[derive(Clone, Debug)]
pub struct RenderedOutput {
    pub meshes: Vec<Arc<Mesh>>,
    pub span_mesh_indices: HashMap<String, Vec<usize>>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum BackendKind {
    Text,
    Tex,
    Latex,
}
