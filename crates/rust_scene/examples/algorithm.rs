//! Rust translation of the key idea in `(Example) Algorithm.mcs`.
//!
//! The BFS and the visual recipe are ordinary concrete Rust. No macro is used:
//! the algorithm replaces one `Grid` keyframe with another `Grid` keyframe.

use rust_scene::{Color, MeshValue, Recipe, Result, Scene, Vec2, mesh};

const WIDTH: usize = 6;
const HEIGHT: usize = 4;
const START: usize = 0;
const GOAL: usize = WIDTH * HEIGHT - 1;

#[derive(Clone)]
struct Grid {
    distances: Vec<i32>,
    frontier: Vec<usize>,
}

impl Grid {
    fn new() -> Self {
        let mut distances = vec![-1; WIDTH * HEIGHT];
        distances[START] = 0;
        Self {
            distances,
            frontier: vec![START],
        }
    }

    fn expand(&mut self) {
        let mut next = Vec::new();
        for cell in self.frontier.drain(..) {
            let distance = self.distances[cell] + 1;
            for neighbor in neighbors(cell) {
                if self.distances[neighbor] == -1 {
                    self.distances[neighbor] = distance;
                    next.push(neighbor);
                }
            }
        }
        self.frontier = next;
    }
}

impl Recipe for Grid {
    fn evaluate(&self) -> Result<MeshValue> {
        Ok(MeshValue::group((0..WIDTH * HEIGHT).map(|cell| {
            let x = cell % WIDTH;
            let y = cell / WIDTH;
            let color = if cell == START {
                Color::BLUE
            } else if cell == GOAL {
                Color::ORANGE
            } else if self.frontier.contains(&cell) {
                Color::ORANGE
            } else if self.distances[cell] >= 0 {
                Color {
                    r: 130,
                    g: 180,
                    b: 255,
                }
            } else {
                Color {
                    r: 230,
                    g: 230,
                    b: 230,
                }
            };
            MeshValue::square(Vec2::new(x as f64, -(y as f64)), 0.82, color)
        })))
    }
}

fn neighbors(cell: usize) -> impl Iterator<Item = usize> {
    let x = cell % WIDTH;
    let y = cell / WIDTH;
    [
        x.checked_sub(1).map(|x| y * WIDTH + x),
        (x + 1 < WIDTH).then_some(y * WIDTH + x + 1),
        y.checked_sub(1).map(|y| y * WIDTH + x),
        (y + 1 < HEIGHT).then_some((y + 1) * WIDTH + x),
    ]
    .into_iter()
    .flatten()
}

fn main() -> Result<()> {
    let mut scene = Scene::new();
    let grid = mesh(Grid::new())?;

    scene.slide("Wavefront Search", |slide| {
        slide.set(&grid)?;
        Ok(())
    })?;

    // The loop is deliberately outside the closure so `Grid` remains a normal
    // typed value rather than an erased scene variable.
    let mut state = Grid::new();
    while !state.frontier.is_empty() && state.distances[GOAL] == -1 {
        state.expand();
        grid.set(state.clone());
        scene.slides[0].lerp(&grid, 0.2, 2)?;
    }

    println!("BFS recorded {} frames", scene.slides[0].frames.len());
    Ok(())
}
