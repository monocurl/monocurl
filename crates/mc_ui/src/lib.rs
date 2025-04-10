use quarve::core::Environment;
use quarve::prelude::{IntoViewProvider, StandardConstEnv, StandardVarEnv};

mod editor;
mod home;
mod timeline;
mod viewport;
mod project_window;
pub mod home_window;
mod menu;
mod theme;

pub struct Env(StandardConstEnv, StandardVarEnv);

impl Environment for Env {
    type Const = StandardConstEnv;
    type Variable = StandardVarEnv;

    fn root_environment() -> Self {
        Env(StandardConstEnv::new(), StandardVarEnv::new())
    }

    fn const_env(&self) -> &Self::Const {
        &self.0
    }

    fn variable_env(&self) -> &Self::Variable { &self.1 }

    fn variable_env_mut(&mut self) -> &mut Self::Variable { &mut self.1 }
}

pub(crate) trait IVP: IntoViewProvider<Env, UpContext=(), DownContext=()> { }

impl<I> IVP for I where I: IntoViewProvider<Env, UpContext=(), DownContext=()> { }

