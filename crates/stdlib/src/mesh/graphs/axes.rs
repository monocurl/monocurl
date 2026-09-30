use executor::{error::ExecutorError, executor::Executor, heap::with_heap, value::Value};
use geo::simd::{Float3, Float4};
use stdlib_macros::stdlib_func;

use crate::mesh::{constructors::vector_like_mesh_with_style, helpers::*};

use super::*;

fn tick_count(min: f32, max: f32, step: f32) -> usize {
    if max < min {
        0
    } else {
        (((max - min) / step).ceil() as usize).saturating_add(1)
    }
}

fn axis_tick_values(min: f32, max: f32, step: f32) -> Vec<(i64, f32)> {
    if max < min {
        return Vec::new();
    }

    let first = (min / step).ceil() as i64;
    let last = (max / step).floor() as i64;
    (first..=last)
        .map(|tick| (tick, tick as f32 * step))
        .collect()
}

#[derive(Clone, Copy)]
struct AxisRange {
    min: f32,
    max: f32,
    tick_step: f32,
}

#[derive(Clone)]
enum AxisLabelMap {
    DefaultFormat,
    None,
    Callable(Value),
}

/// Where a tick mark sits relative to the axis line.
#[derive(Clone, Copy, PartialEq)]
enum TickPlacement {
    /// `p ± extend` — straddles the axis (default, unchanged).
    Both,
    /// `p .. p + extend` — only on the `+side` of the axis.
    Positive,
    /// `p - extend .. p` — only on the `-side` of the axis.
    Negative,
    /// `p ± extend/2` — straddles, half length.
    Centered,
}

fn tick_placement_from_value(
    value: Value,
    name: &'static str,
) -> Result<TickPlacement, ExecutorError> {
    match value.elide_cached_wrappers_rec() {
        Value::Nil => Ok(TickPlacement::Both),
        Value::String(s) => match s.to_string().as_str() {
            "both" => Ok(TickPlacement::Both),
            "positive" => Ok(TickPlacement::Positive),
            "negative" => Ok(TickPlacement::Negative),
            "centered" => Ok(TickPlacement::Centered),
            _ => Err(ExecutorError::InvalidArgument {
                arg: name,
                message: "must be \"both\", \"positive\", \"negative\", or \"centered\"",
            }),
        },
        other => Err(ExecutorError::type_error_for(
            "string or nil",
            other.type_name(),
            name,
        )),
    }
}

#[derive(Clone)]
struct AxisStyle {
    range: AxisRange,
    title: Option<String>,
    major_tick_rate: usize,
    label_map: AxisLabelMap,
    arrow_extrusion: f32,
    /// draw the arrowheads at both ends of the axis. `false` when the style
    /// list's `arrow_extrusion` slot is `nil`.
    draw_arrows: bool,
    /// explicit tick positions in axis coordinates. Empty = uniform ticks from
    /// `range.tick_step`. Set when the `tick_spacing` slot is a list. Every
    /// listed position is drawn as a major tick.
    explicit_ticks: Vec<f32>,
    /// where tick marks sit relative to the axis line.
    tick_placement: TickPlacement,
}

impl AxisStyle {
    fn from_range(range: AxisRange) -> Self {
        Self {
            range,
            title: None,
            major_tick_rate: 4,
            label_map: AxisLabelMap::DefaultFormat,
            arrow_extrusion: AXIS_BUFFER,
            draw_arrows: true,
            explicit_ticks: Vec::new(),
            tick_placement: TickPlacement::Both,
        }
    }

    /// Tick `(index, value)` pairs for this style — either the explicit list
    /// (all major) or uniform ticks from `range`.
    fn ticks(&self) -> Vec<(i64, f32)> {
        if self.explicit_ticks.is_empty() {
            axis_tick_values(self.range.min, self.range.max, self.range.tick_step)
        } else {
            self.explicit_ticks.iter().map(|&v| (0_i64, v)).collect()
        }
    }

    /// Upper bound on how many ticks this style produces, for limit checks.
    fn tick_budget(&self) -> usize {
        if self.explicit_ticks.is_empty() {
            tick_count(self.range.min, self.range.max, self.range.tick_step)
        } else {
            self.explicit_ticks.len()
        }
    }
}

fn axis_number_from_value(value: Value, name: &'static str) -> Result<f32, ExecutorError> {
    match value.elide_lvalue_leader_rec() {
        Value::Integer(value) => Ok(value as f32),
        Value::Float(value) => Ok(value as f32),
        other => Err(ExecutorError::type_error_for(
            "number",
            other.type_name(),
            name,
        )),
    }
}

fn checked_axis_tick_step(value: f32, name: &'static str) -> Result<f32, ExecutorError> {
    if !value.is_finite() || value == 0.0 {
        return Err(ExecutorError::InvalidArgument {
            arg: name,
            message: "tick step must be a non-zero finite number",
        });
    }
    Ok(value.abs().max(1e-3))
}

fn checked_axis_arrow_extrusion(value: f32) -> Result<f32, ExecutorError> {
    if !value.is_finite() || value < 0.0 {
        return Err(ExecutorError::InvalidArgument {
            arg: "arrow_extrusion",
            message: "must be a non-negative finite number",
        });
    }
    Ok(value)
}

fn checked_axis_range(
    min: f32,
    max: f32,
    tick_step: f32,
    name: &'static str,
) -> Result<AxisRange, ExecutorError> {
    if !min.is_finite() || !max.is_finite() {
        return Err(ExecutorError::InvalidArgument {
            arg: name,
            message: "range bounds must be finite numbers",
        });
    }
    Ok(AxisRange {
        min,
        max,
        tick_step: checked_axis_tick_step(tick_step, name)?,
    })
}

fn axis_radius_range(radius: f32, name: &'static str) -> Result<AxisRange, ExecutorError> {
    if !radius.is_finite() || radius < 0.0 {
        return Err(ExecutorError::InvalidArgument {
            arg: name,
            message: "radius must be a non-negative finite number",
        });
    }
    checked_axis_range(-radius, radius, DEFAULT_AXIS_TICK_STEP, name)
}

fn label_map_from_value(value: Value) -> AxisLabelMap {
    match value.elide_cached_wrappers_rec() {
        Value::Nil => AxisLabelMap::None,
        other => AxisLabelMap::Callable(other),
    }
}

fn axis_title_from_value(
    value: Value,
    name: &'static str,
) -> Result<Option<String>, ExecutorError> {
    match value.elide_cached_wrappers_rec() {
        Value::Nil => Ok(None),
        Value::String(value) => Ok(Some(value.to_string())),
        Value::Integer(value) => Ok(Some(value.to_string())),
        Value::Float(value) => Ok(Some(value.to_string())),
        other => Err(ExecutorError::type_error_for(
            crate::STRING_COMPATIBLE_DESC,
            other.type_name(),
            name,
        )),
    }
}

fn axis_style_third_arg_is_title(value: Value) -> bool {
    // a number (tick_spacing) or a list (explicit tick positions) in slot 3 means
    // there is no axis title
    !matches!(
        value.elide_cached_wrappers_rec(),
        Value::Integer(_) | Value::Float(_) | Value::List(_)
    )
}

fn read_axis_style(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<AxisStyle, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue_leader_rec()
    {
        Value::Integer(radius) => Ok(AxisStyle::from_range(axis_radius_range(
            radius as f32,
            name,
        )?)),
        Value::Float(radius) => Ok(AxisStyle::from_range(axis_radius_range(
            radius as f32,
            name,
        )?)),
        Value::List(list) => {
            let elements = list.elements();
            let mut title = None;
            let mut major_tick_rate = 4;
            let mut label_map = AxisLabelMap::DefaultFormat;
            let mut arrow_extrusion = AXIS_BUFFER;
            let mut draw_arrows = true;
            let mut explicit_ticks: Vec<f32> = Vec::new();
            let mut tick_placement = TickPlacement::Both;
            let range = match elements.len() {
                1 => {
                    let radius = axis_number_from_value(
                        with_heap(|h| h.get(elements[0].key()).clone()),
                        name,
                    )?;
                    axis_radius_range(radius, name)?
                }
                2..=8 => {
                    let min = axis_number_from_value(
                        with_heap(|h| h.get(elements[0].key()).clone()),
                        name,
                    )?;
                    let max = axis_number_from_value(
                        with_heap(|h| h.get(elements[1].key()).clone()),
                        name,
                    )?;
                    let has_title = elements.len() >= 3
                        && axis_style_third_arg_is_title(with_heap(|h| {
                            h.get(elements[2].key()).clone()
                        }));
                    if !has_title && elements.len() > 7 {
                        return Err(ExecutorError::invalid_operation(format!(
                            "{name}: expected [min, max, (axis_title,) tick_spacing, major_tick_rate, label_map, arrow_extrusion, tick_placement]"
                        )));
                    }
                    let tick_step_index = if has_title {
                        title = axis_title_from_value(
                            with_heap(|h| h.get(elements[2].key()).clone()),
                            "axis_title",
                        )?;
                        3
                    } else {
                        2
                    };
                    let tick_step = if elements.len() > tick_step_index {
                        let raw = with_heap(|h| h.get(elements[tick_step_index].key()).clone());
                        match raw.clone().elide_cached_wrappers_rec() {
                            Value::List(list) => {
                                for key in list.elements() {
                                    let v = axis_number_from_value(
                                        with_heap(|h| h.get(key.key()).clone()),
                                        "tick_spacing",
                                    )?;
                                    if v.is_finite() {
                                        explicit_ticks.push(v);
                                    }
                                }
                                DEFAULT_AXIS_TICK_STEP
                            }
                            _ => axis_number_from_value(raw, "tick_spacing")?,
                        }
                    } else {
                        DEFAULT_AXIS_TICK_STEP
                    };
                    if elements.len() > tick_step_index + 1 {
                        major_tick_rate = label_rate_from_value(
                            with_heap(|h| h.get(elements[tick_step_index + 1].key()).clone()),
                            "major_tick_rate",
                        )?;
                    }
                    if elements.len() > tick_step_index + 2 {
                        label_map = label_map_from_value(with_heap(|h| {
                            h.get(elements[tick_step_index + 2].key()).clone()
                        }));
                    }
                    if elements.len() > tick_step_index + 3 {
                        let raw = with_heap(|h| h.get(elements[tick_step_index + 3].key()).clone());
                        if matches!(raw.clone().elide_cached_wrappers_rec(), Value::Nil) {
                            draw_arrows = false;
                            arrow_extrusion = 0.0;
                        } else {
                            arrow_extrusion = checked_axis_arrow_extrusion(
                                axis_number_from_value(raw, "arrow_extrusion")?,
                            )?;
                        }
                    }
                    if elements.len() > tick_step_index + 4 {
                        tick_placement = tick_placement_from_value(
                            with_heap(|h| h.get(elements[tick_step_index + 4].key()).clone()),
                            "tick_placement",
                        )?;
                    }
                    checked_axis_range(min, max, tick_step, name)?
                }
                len => {
                    return Err(ExecutorError::invalid_operation(format!(
                        "{name}: expected a number or [min, max, (axis_title,) tick_spacing, major_tick_rate, label_map, arrow_extrusion, tick_placement], got list of length {len}"
                    )));
                }
            };

            // keep only explicit ticks that fall inside the axis range
            let eps = (range.max - range.min).abs() * 1e-4 + 1e-6;
            explicit_ticks.retain(|&v| v >= range.min - eps && v <= range.max + eps);

            Ok(AxisStyle {
                range,
                title,
                major_tick_rate,
                label_map,
                arrow_extrusion,
                draw_arrows,
                explicit_ticks,
                tick_placement,
            })
        }
        other => Err(ExecutorError::type_error_for(
            "number or axis style list",
            other.type_name(),
            name,
        )),
    }
}

fn read_axis_basis(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
) -> Result<Float3, ExecutorError> {
    let basis = read_float3(executor, stack_idx, index, name)?;
    checked_axis_basis(basis, name)
}

fn checked_axis_basis(basis: Float3, name: &'static str) -> Result<Float3, ExecutorError> {
    if !basis.x.is_finite()
        || !basis.y.is_finite()
        || !basis.z.is_finite()
        || basis.len_sq() <= 1e-8
    {
        return Err(ExecutorError::InvalidArgument {
            arg: name,
            message: "must be a non-zero finite vector",
        });
    }
    Ok(basis)
}

fn read_axis_basis_list(
    executor: &Executor,
    stack_idx: usize,
    index: i32,
    name: &'static str,
    axis_names: &[&'static str],
) -> Result<Vec<Float3>, ExecutorError> {
    match executor
        .state
        .stack(stack_idx)
        .read_at(index)
        .clone()
        .elide_lvalue_leader_rec()
    {
        Value::List(list) if list.elements().len() == axis_names.len() => list
            .elements()
            .iter()
            .zip(axis_names.iter().copied())
            .map(|(key, axis_name)| {
                checked_axis_basis(
                    float3_from_value(with_heap(|h| h.get(key.key()).clone()), axis_name)?,
                    axis_name,
                )
            })
            .collect(),
        Value::List(list) => Err(ExecutorError::invalid_operation(format!(
            "{name}: expected list of length {}, got list of length {}",
            axis_names.len(),
            list.elements().len()
        ))),
        other => Err(ExecutorError::type_error_for(
            "axis basis list",
            other.type_name(),
            name,
        )),
    }
}

fn label_rate_from_value(value: Value, name: &'static str) -> Result<usize, ExecutorError> {
    let rate = match value.elide_lvalue_leader_rec() {
        Value::Integer(value) => value,
        Value::Float(value) if value.fract() == 0.0 => value as i64,
        other => {
            return Err(ExecutorError::type_error_for(
                "int",
                other.type_name(),
                name,
            ));
        }
    };
    if rate < 0 {
        return Err(ExecutorError::InvalidArgument {
            arg: name,
            message: "must be non-negative",
        });
    }
    Ok(rate as usize)
}

fn format_axis_number(value: f32) -> String {
    let value = if value.abs() < 1e-6 { 0.0 } else { value };
    let abs = value.abs();
    if abs >= 10_000.0 || (abs > 0.0 && abs < 0.001) {
        return format!("{value:.4e}");
    }

    let mut out = format!("{value:.6}");
    while out.contains('.') && out.ends_with('0') {
        out.pop();
    }
    if out.ends_with('.') {
        out.pop();
    }
    if out == "-0" {
        out.clear();
        out.push('0');
    }
    out
}

fn is_major_tick(tick: i64, major_tick_rate: usize) -> bool {
    major_tick_rate != 0 && tick % major_tick_rate as i64 == 0
}

fn axis_text_basis(right: Float3, normal: Float3) -> (Float3, Float3, Float3) {
    let normal = normalize_or(normal, Float3::Z);
    let projected_right = right - normal * right.dot(normal);
    let right = if projected_right.len_sq() > 1e-8 {
        projected_right.normalize()
    } else {
        polygon_basis(normal).0
    };
    let up = normalize_or(normal.cross(right), polygon_basis(normal).1);
    (right, up, normal)
}

#[derive(Clone, Copy)]
struct AxisTextFrame {
    right: Float3,
    up: Float3,
    normal: Float3,
}

impl AxisTextFrame {
    fn from_right_normal(right: Float3, normal: Float3) -> Self {
        let (right, up, normal) = axis_text_basis(right, normal);
        Self { right, up, normal }
    }

    fn from_right_up(right: Float3, up: Float3, normal_hint: Float3) -> Self {
        let up = up.normalize();
        let projected_right = right - up * right.dot(up);
        let right = if projected_right.len_sq() > 1e-8 {
            projected_right.normalize()
        } else {
            let projected_normal_right = up.cross(normal_hint);
            if projected_normal_right.len_sq() > 1e-8 {
                projected_normal_right.normalize()
            } else {
                polygon_basis(up).0
            }
        };
        let normal = right.cross(up).normalize();
        Self { right, up, normal }
    }
}

fn orient_text_tree(tree: &mut MeshTree, frame: AxisTextFrame) {
    let AxisTextFrame { right, up, normal } = frame;
    tree.for_each_mut(&mut |mesh| {
        transform_mesh_positions(mesh, |p| right * p.x + up * p.y + normal * p.z)
    });
}

fn place_tree_next_to_point(tree: &mut MeshTree, anchor: Float3, dir: Float3, buffer: f32) {
    let dir = normalize_or(dir, Float3::X);
    let center = tree_center(tree).unwrap_or(Float3::ZERO);
    let label_face = extremal_point(tree, -dir).unwrap_or(center).dot(dir);
    let orth = (anchor - center) - dir * (anchor - center).dot(dir);
    let delta = dir * (anchor.dot(dir) + buffer - label_face) + orth;
    tree.for_each_mut(&mut |mesh| transform_mesh_positions(mesh, |p| p + delta));
}

fn render_axis_tex_tree(
    executor: &Executor,
    tex: &str,
    scale: f32,
    name: &'static str,
) -> Result<Option<MeshTree>, ExecutorError> {
    let meshes = text::render_tex_with_quality(tex, scale, text_render_quality(executor)).map_err(
        |error| ExecutorError::invalid_invocation(format!("{name} render failed: {error:#}")),
    )?;
    if meshes.is_empty() {
        Ok(None)
    } else {
        Ok(Some(MeshTree::List(
            meshes.into_iter().map(MeshTree::Mesh).collect(),
        )))
    }
}

fn styled_line_mesh(
    mut lins: Vec<geo::mesh::Lin>,
    stroke_radius: f32,
    alpha: f32,
    color: Option<Float4>,
) -> Option<Value> {
    if lins.is_empty() {
        return None;
    }

    if let Some(color) = color {
        for lin in &mut lins {
            lin.a.col = color;
            lin.b.col = color;
        }
    }

    let Value::Mesh(mesh) = mesh_from_parts(vec![], lins, vec![]) else {
        unreachable!("mesh_from_parts always returns a mesh")
    };
    let mut mesh = (*mesh).clone();
    mesh.uniform.stroke_radius = stroke_radius;
    for lin in &mut mesh.lins {
        lin.a.col.w *= alpha;
        lin.b.col.w *= alpha;
    }
    Some(Value::Mesh(std::sync::Arc::new(mesh)))
}

fn push_styled_line_meshes(
    out: &mut Vec<Value>,
    small_lins: Vec<geo::mesh::Lin>,
    large_lins: Vec<geo::mesh::Lin>,
    color: Float4,
    grid: bool,
) {
    let (small_width, small_alpha, large_width, large_alpha) = if grid {
        (
            SMALL_TICK_WIDTH,
            SMALL_TICK_GRID_OPACITY,
            LARGE_TICK_GRID_WIDTH,
            LARGE_TICK_GRID_OPACITY,
        )
    } else {
        (
            SMALL_TICK_WIDTH,
            SMALL_TICK_OPACITY,
            LARGE_TICK_WIDTH,
            LARGE_TICK_OPACITY,
        )
    };

    if let Some(mesh) = styled_line_mesh(small_lins, small_width, small_alpha, Some(color)) {
        out.push(mesh);
    }
    if let Some(mesh) = styled_line_mesh(large_lins, large_width, large_alpha, Some(color)) {
        out.push(mesh);
    }
}

fn axis_result(axis_meshes: Vec<Value>, labels: Vec<Value>) -> Value {
    let mut values = axis_meshes;
    values.extend(labels);
    if values.len() == 1 {
        values.pop().expect("length checked")
    } else {
        list_value(values)
    }
}

fn axis_arrow_mesh(
    tail: Float3,
    delta: Float3,
    normal: Float3,
    color: Float4,
) -> Result<Value, ExecutorError> {
    let Value::Mesh(mesh) =
        vector_like_mesh_with_style(tail, delta, normal, 0.0, AXIS_ARROW_STYLE)?
    else {
        unreachable!("vector_like_mesh_with_style always returns a mesh")
    };
    let mut mesh = (*mesh).clone();
    recolor_mesh(&mut mesh, color);
    mesh.uniform.stroke_radius = AXIS_ARROW_STROKE_WIDTH;
    Ok(Value::Mesh(std::sync::Arc::new(mesh)))
}

fn push_axis_arrows(
    out: &mut Vec<Value>,
    center: Float3,
    basis: Float3,
    range: AxisRange,
    normal: Float3,
    color: Float4,
    arrow_extrusion: f32,
    draw_arrows: bool,
) -> Result<(), ExecutorError> {
    if !draw_arrows {
        return Ok(());
    }
    let dir = basis.normalize();
    out.push(axis_arrow_mesh(
        center,
        basis * range.max + dir * arrow_extrusion,
        normal,
        color,
    )?);
    out.push(axis_arrow_mesh(
        center,
        basis * range.min - dir * arrow_extrusion,
        normal,
        color,
    )?);
    Ok(())
}

fn axis_title_anchor(center: Float3, basis: Float3, max: f32, arrow_extrusion: f32) -> Float3 {
    center + basis * max + basis.normalize() * arrow_extrusion
}

fn axis_tick_lins(
    ticks: &[(i64, f32)],
    center: Float3,
    basis: Float3,
    side: Float3,
    normal: Float3,
    major_tick_rate: usize,
    placement: TickPlacement,
) -> (Vec<geo::mesh::Lin>, Vec<geo::mesh::Lin>) {
    let side = normalize_or(side, polygon_basis(basis.normalize()).1);
    let mut small = Vec::new();
    let mut large = Vec::new();
    for &(tick, value) in ticks {
        let p = center + basis * value;
        let major = is_major_tick(tick, major_tick_rate);
        let target = if major { &mut large } else { &mut small };
        let extend = if major {
            LARGE_TICK_EXTEND
        } else {
            SMALL_TICK_EXTEND
        };
        let (a, b) = match placement {
            TickPlacement::Both => (p - side * extend, p + side * extend),
            TickPlacement::Positive => (p, p + side * extend),
            TickPlacement::Negative => (p - side * extend, p),
            TickPlacement::Centered => (p - side * (extend * 0.5), p + side * (extend * 0.5)),
        };
        target.push(default_lin(a, b, normal));
    }
    (small, large)
}

fn axis_grid_lins(
    ticks: &[(i64, f32)],
    center: Float3,
    basis: Float3,
    cross_basis: Float3,
    cross_range: AxisRange,
    normal: Float3,
    major_tick_rate: usize,
) -> (Vec<geo::mesh::Lin>, Vec<geo::mesh::Lin>) {
    let mut small = Vec::new();
    let mut large = Vec::new();
    for &(tick, value) in ticks {
        let p = center + basis * value;
        let target = if is_major_tick(tick, major_tick_rate) {
            &mut large
        } else {
            &mut small
        };
        target.push(default_lin(
            p + cross_basis * cross_range.min,
            p + cross_basis * cross_range.max,
            normal,
        ));
    }
    (small, large)
}

fn push_axis_title(
    executor: &Executor,
    out: &mut Vec<Value>,
    label: Option<String>,
    anchor: Float3,
    dir: Float3,
    text_frame: AxisTextFrame,
    color: Float4,
) -> Result<(), ExecutorError> {
    let Some(label) = label else {
        return Ok(());
    };
    let Some(mut tree) = render_axis_tex_tree(executor, &label, AXIS_TITLE_SCALE, "axis label")?
    else {
        return Ok(());
    };
    recolor_tree(&mut tree, color);
    orient_text_tree(&mut tree, text_frame);
    place_tree_next_to_point(&mut tree, anchor, dir, AXIS_TITLE_BUFFER);
    out.push(tree.into_value());
    Ok(())
}

async fn axis_tick_label_text(
    executor: &mut Executor,
    label_map: &AxisLabelMap,
    value: f32,
) -> Result<Option<String>, ExecutorError> {
    match label_map {
        AxisLabelMap::DefaultFormat => Ok(Some(format_axis_number(value))),
        AxisLabelMap::None => Ok(None),
        AxisLabelMap::Callable(callable) => {
            let label = invoke_callable(
                executor,
                callable,
                vec![Value::Float(value as f64)],
                "label_map",
            )
            .await?;
            if matches!(label, Value::Nil) {
                Ok(None)
            } else {
                crate::stringify_value(executor, label).await.map(Some)
            }
        }
    }
}

async fn push_axis_tick_labels(
    executor: &mut Executor,
    out: &mut Vec<Value>,
    ticks: &[(i64, f32)],
    center: Float3,
    basis: Float3,
    side: Float3,
    text_frame: AxisTextFrame,
    style: &AxisStyle,
    label_zero: bool,
    zero_offset: f32,
    color: Float4,
) -> Result<(), ExecutorError> {
    if matches!(style.label_map, AxisLabelMap::None) || style.major_tick_rate == 0 {
        return Ok(());
    }

    let dir = basis.normalize();
    for &(tick, value) in ticks {
        if !is_major_tick(tick, style.major_tick_rate) || (!label_zero && value.abs() <= 1e-4) {
            continue;
        }
        let mut anchor = center + basis * value;
        if value.abs() <= 1e-4 {
            anchor -= dir * zero_offset;
        }
        let Some(text) = axis_tick_label_text(executor, &style.label_map, value).await? else {
            continue;
        };
        let Some(mut tree) =
            render_axis_tex_tree(executor, &text, AXIS_TICK_LABEL_SCALE, "axis tick label")?
        else {
            continue;
        };
        recolor_tree(&mut tree, color);
        orient_text_tree(&mut tree, text_frame);
        place_tree_next_to_point(&mut tree, anchor, side, AXIS_TICK_LABEL_BUFFER);
        out.push(tree.into_value());
    }
    Ok(())
}

#[stdlib_func]
pub async fn mk_axis1d(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let center = Float3::ZERO;
    let axis = read_axis_basis(executor, stack_idx, -4, "basis")?;
    let axis_dir = axis.normalize();
    let normal = read_float3(executor, stack_idx, -3, "normal")?;
    let color = read_float4(executor, stack_idx, -2, "color").await?;
    let style = read_axis_style(executor, stack_idx, -1, "x_axis")?;
    ensure_limit("axis ticks", style.tick_budget(), MAX_AXIS_TICKS)?;
    let tick_dir = {
        let dir = normal.cross(axis_dir);
        if dir.len_sq() > 1e-6 {
            dir.normalize()
        } else {
            polygon_basis(axis_dir).1
        }
    };
    let ticks = style.ticks();
    let mut axis_meshes = Vec::new();
    let (small_ticks, large_ticks) = axis_tick_lins(
        &ticks,
        center,
        axis,
        tick_dir,
        normal,
        style.major_tick_rate,
        style.tick_placement,
    );
    push_styled_line_meshes(&mut axis_meshes, small_ticks, large_ticks, color, false);
    push_axis_arrows(
        &mut axis_meshes,
        center,
        axis,
        style.range,
        normal,
        color,
        style.arrow_extrusion,
        style.draw_arrows,
    )?;

    let mut labels = Vec::new();
    let text_frame = AxisTextFrame::from_right_normal(axis_dir, normal);
    push_axis_tick_labels(
        executor,
        &mut labels,
        &ticks,
        center,
        axis,
        -tick_dir,
        text_frame,
        &style,
        true,
        AXIS_ZERO_TICK_LABEL_OFFSET,
        color,
    )
    .await?;
    push_axis_title(
        executor,
        &mut labels,
        style.title,
        axis_title_anchor(center, axis, style.range.max, style.arrow_extrusion),
        axis_dir,
        text_frame,
        color,
    )?;
    Ok(axis_result(axis_meshes, labels))
}

#[stdlib_func]
pub async fn mk_axis2d(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let center = Float3::ZERO;
    let axes = read_axis_basis_list(executor, stack_idx, -5, "basis", &["x_basis", "y_basis"])?;
    let [x_axis, y_axis]: [Float3; 2] = axes.try_into().expect("length checked");
    let color = read_float4(executor, stack_idx, -4, "color").await?;
    let x_style = read_axis_style(executor, stack_idx, -3, "x_axis")?;
    let y_style = read_axis_style(executor, stack_idx, -2, "y_axis")?;
    let grid_color = read_optional_color(executor, stack_idx, -1, "grid_color").await?;
    ensure_limit("axis x ticks", x_style.tick_budget(), MAX_AXIS_TICKS)?;
    ensure_limit("axis y ticks", y_style.tick_budget(), MAX_AXIS_TICKS)?;
    let x_dir = x_axis.normalize();
    let y_dir = y_axis.normalize();
    let normal = x_dir.cross(y_dir);
    if normal.len_sq() <= 1e-8 {
        return Err(ExecutorError::InvalidArgument {
            arg: "axes",
            message: "x_axis and y_axis must not be parallel",
        });
    }
    let normal = normal.normalize();
    let x_ticks = x_style.ticks();
    let y_ticks = y_style.ticks();
    let mut axis_meshes = Vec::new();
    if let Some(grid_color) = grid_color {
        let (small_x, large_x) = axis_grid_lins(
            &x_ticks,
            center,
            x_axis,
            y_axis,
            y_style.range,
            normal,
            x_style.major_tick_rate,
        );
        let (small_y, large_y) = axis_grid_lins(
            &y_ticks,
            center,
            y_axis,
            x_axis,
            x_style.range,
            normal,
            y_style.major_tick_rate,
        );
        push_styled_line_meshes(&mut axis_meshes, small_x, large_x, grid_color, true);
        push_styled_line_meshes(&mut axis_meshes, small_y, large_y, grid_color, true);
    } else {
        let (small_x, large_x) = axis_tick_lins(
            &x_ticks,
            center,
            x_axis,
            y_dir,
            normal,
            x_style.major_tick_rate,
            x_style.tick_placement,
        );
        let (small_y, large_y) = axis_tick_lins(
            &y_ticks,
            center,
            y_axis,
            x_dir,
            normal,
            y_style.major_tick_rate,
            y_style.tick_placement,
        );
        push_styled_line_meshes(&mut axis_meshes, small_x, large_x, color, false);
        push_styled_line_meshes(&mut axis_meshes, small_y, large_y, color, false);
    }
    push_axis_arrows(
        &mut axis_meshes,
        center,
        x_axis,
        x_style.range,
        normal,
        color,
        x_style.arrow_extrusion,
        x_style.draw_arrows,
    )?;
    push_axis_arrows(
        &mut axis_meshes,
        center,
        y_axis,
        y_style.range,
        normal,
        color,
        y_style.arrow_extrusion,
        y_style.draw_arrows,
    )?;

    let mut labels = Vec::new();
    let text_frame = AxisTextFrame::from_right_normal(x_dir, normal);
    push_axis_tick_labels(
        executor,
        &mut labels,
        &x_ticks,
        center,
        x_axis,
        -y_dir,
        text_frame,
        &x_style,
        true,
        AXIS_ZERO_TICK_LABEL_OFFSET,
        color,
    )
    .await?;
    push_axis_tick_labels(
        executor,
        &mut labels,
        &y_ticks,
        center,
        y_axis,
        -x_dir,
        text_frame,
        &y_style,
        false,
        0.0,
        color,
    )
    .await?;
    push_axis_title(
        executor,
        &mut labels,
        x_style.title,
        axis_title_anchor(center, x_axis, x_style.range.max, x_style.arrow_extrusion),
        x_dir,
        text_frame,
        color,
    )?;
    push_axis_title(
        executor,
        &mut labels,
        y_style.title,
        axis_title_anchor(center, y_axis, y_style.range.max, y_style.arrow_extrusion),
        y_dir,
        text_frame,
        color,
    )?;
    Ok(axis_result(axis_meshes, labels))
}

#[stdlib_func]
pub async fn mk_axis3d(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let center = Float3::ZERO;
    let axes = read_axis_basis_list(
        executor,
        stack_idx,
        -7,
        "basis",
        &["x_basis", "y_basis", "z_basis"],
    )?;
    let [x_axis, y_axis, z_axis]: [Float3; 3] = axes.try_into().expect("length checked");
    let color = read_float4(executor, stack_idx, -6, "color").await?;
    let label_ups = read_axis_basis_list(
        executor,
        stack_idx,
        -5,
        "label_up",
        &["x_label_up", "y_label_up", "z_label_up"],
    )?;
    let [x_label_up, y_label_up, z_label_up]: [Float3; 3] =
        label_ups.try_into().expect("length checked");
    let x_style = read_axis_style(executor, stack_idx, -4, "x_axis")?;
    let y_style = read_axis_style(executor, stack_idx, -3, "y_axis")?;
    let z_style = read_axis_style(executor, stack_idx, -2, "z_axis")?;
    let grid_color = read_optional_color(executor, stack_idx, -1, "grid_color").await?;
    ensure_limit("axis x ticks", x_style.tick_budget(), MAX_AXIS_TICKS)?;
    ensure_limit("axis y ticks", y_style.tick_budget(), MAX_AXIS_TICKS)?;
    ensure_limit("axis z ticks", z_style.tick_budget(), MAX_AXIS_TICKS)?;
    let x_dir = x_axis.normalize();
    let y_dir = y_axis.normalize();
    let z_dir = z_axis.normalize();
    if x_dir.cross(y_dir).len_sq() <= 1e-8
        || x_dir.cross(z_dir).len_sq() <= 1e-8
        || y_dir.cross(z_dir).len_sq() <= 1e-8
    {
        return Err(ExecutorError::InvalidArgument {
            arg: "axes",
            message: "axis basis vectors must not be parallel",
        });
    }
    let xy_normal = normalize_or(x_dir.cross(y_dir), z_dir);
    let xz_normal = y_dir;
    let yz_normal = normalize_or(y_dir.cross(z_dir), x_dir);
    let z_normal = normalize_or(x_dir.cross(z_dir), xy_normal);
    let x_ticks = x_style.ticks();
    let y_ticks = y_style.ticks();
    let z_ticks = z_style.ticks();
    let mut axis_meshes = Vec::new();
    if let Some(grid_color) = grid_color {
        let (small_x_xy, large_x_xy) = axis_grid_lins(
            &x_ticks,
            center,
            x_axis,
            y_axis,
            y_style.range,
            xy_normal,
            x_style.major_tick_rate,
        );
        let (small_y_xy, large_y_xy) = axis_grid_lins(
            &y_ticks,
            center,
            y_axis,
            x_axis,
            x_style.range,
            xy_normal,
            y_style.major_tick_rate,
        );
        let (small_x_xz, large_x_xz) = axis_grid_lins(
            &x_ticks,
            center,
            x_axis,
            z_axis,
            z_style.range,
            xz_normal,
            x_style.major_tick_rate,
        );
        let (small_z_xz, large_z_xz) = axis_grid_lins(
            &z_ticks,
            center,
            z_axis,
            x_axis,
            x_style.range,
            xz_normal,
            z_style.major_tick_rate,
        );
        let (small_y_yz, large_y_yz) = axis_grid_lins(
            &y_ticks,
            center,
            y_axis,
            z_axis,
            z_style.range,
            yz_normal,
            y_style.major_tick_rate,
        );
        let (small_z_yz, large_z_yz) = axis_grid_lins(
            &z_ticks,
            center,
            z_axis,
            y_axis,
            y_style.range,
            yz_normal,
            z_style.major_tick_rate,
        );
        push_styled_line_meshes(&mut axis_meshes, small_x_xy, large_x_xy, grid_color, true);
        push_styled_line_meshes(&mut axis_meshes, small_y_xy, large_y_xy, grid_color, true);
        push_styled_line_meshes(&mut axis_meshes, small_x_xz, large_x_xz, grid_color, true);
        push_styled_line_meshes(&mut axis_meshes, small_z_xz, large_z_xz, grid_color, true);
        push_styled_line_meshes(&mut axis_meshes, small_y_yz, large_y_yz, grid_color, true);
        push_styled_line_meshes(&mut axis_meshes, small_z_yz, large_z_yz, grid_color, true);
    } else {
        let (small_x, large_x) = axis_tick_lins(
            &x_ticks,
            center,
            x_axis,
            y_dir,
            xy_normal,
            x_style.major_tick_rate,
            x_style.tick_placement,
        );
        let (small_y, large_y) = axis_tick_lins(
            &y_ticks,
            center,
            y_axis,
            x_dir,
            xy_normal,
            y_style.major_tick_rate,
            y_style.tick_placement,
        );
        let (small_z, large_z) = axis_tick_lins(
            &z_ticks,
            center,
            z_axis,
            x_dir,
            z_normal,
            z_style.major_tick_rate,
            z_style.tick_placement,
        );
        push_styled_line_meshes(&mut axis_meshes, small_x, large_x, color, false);
        push_styled_line_meshes(&mut axis_meshes, small_y, large_y, color, false);
        push_styled_line_meshes(&mut axis_meshes, small_z, large_z, color, false);
    }
    push_axis_arrows(
        &mut axis_meshes,
        center,
        x_axis,
        x_style.range,
        xy_normal,
        color,
        x_style.arrow_extrusion,
        x_style.draw_arrows,
    )?;
    push_axis_arrows(
        &mut axis_meshes,
        center,
        y_axis,
        y_style.range,
        xy_normal,
        color,
        y_style.arrow_extrusion,
        y_style.draw_arrows,
    )?;
    push_axis_arrows(
        &mut axis_meshes,
        center,
        z_axis,
        z_style.range,
        z_normal,
        color,
        z_style.arrow_extrusion,
        z_style.draw_arrows,
    )?;

    let mut labels = Vec::new();
    let x_text_frame = AxisTextFrame::from_right_up(x_dir, x_label_up, xy_normal);
    let y_text_frame = AxisTextFrame::from_right_up(x_dir, y_label_up, xy_normal);
    let z_text_frame = AxisTextFrame::from_right_up(x_dir, z_label_up, z_normal);
    push_axis_tick_labels(
        executor,
        &mut labels,
        &x_ticks,
        center,
        x_axis,
        -y_dir,
        x_text_frame,
        &x_style,
        true,
        AXIS_ZERO_TICK_LABEL_OFFSET,
        color,
    )
    .await?;
    push_axis_tick_labels(
        executor,
        &mut labels,
        &y_ticks,
        center,
        y_axis,
        -x_dir,
        y_text_frame,
        &y_style,
        false,
        0.0,
        color,
    )
    .await?;
    push_axis_tick_labels(
        executor,
        &mut labels,
        &z_ticks,
        center,
        z_axis,
        -x_dir,
        z_text_frame,
        &z_style,
        false,
        0.0,
        color,
    )
    .await?;
    push_axis_title(
        executor,
        &mut labels,
        x_style.title,
        axis_title_anchor(center, x_axis, x_style.range.max, x_style.arrow_extrusion),
        x_dir,
        x_text_frame,
        color,
    )?;
    push_axis_title(
        executor,
        &mut labels,
        y_style.title,
        axis_title_anchor(center, y_axis, y_style.range.max, y_style.arrow_extrusion),
        y_dir,
        y_text_frame,
        color,
    )?;
    push_axis_title(
        executor,
        &mut labels,
        z_style.title,
        axis_title_anchor(center, z_axis, z_style.range.max, z_style.arrow_extrusion),
        z_dir,
        z_text_frame,
        color,
    )?;
    Ok(axis_result(axis_meshes, labels))
}

#[stdlib_func]
pub fn mk_polar_axis(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let center = read_float3(executor, stack_idx, -9, "center")?;
    let theta_min = crate::read_float(executor, stack_idx, -8, "theta_min")? as f32;
    let theta_max = crate::read_float(executor, stack_idx, -7, "theta_max")? as f32;
    let theta_step = crate::read_float(executor, stack_idx, -6, "theta_step")?
        .abs()
        .max(1e-3) as f32;
    let radius_min = crate::read_float(executor, stack_idx, -4, "radius_min")?.max(0.0) as f32;
    let radius_max =
        crate::read_float(executor, stack_idx, -3, "radius_max")?.max(radius_min as f64) as f32;
    let radius_step = crate::read_float(executor, stack_idx, -2, "radius_step")?
        .abs()
        .max(1e-3) as f32;
    ensure_limit(
        "polar axis rings",
        tick_count(radius_min.max(radius_step), radius_max, radius_step),
        MAX_AXIS_TICKS,
    )?;
    ensure_limit(
        "polar axis rays",
        tick_count(theta_min, theta_max, theta_step),
        MAX_AXIS_TICKS,
    )?;
    let (x, y, normal) = polygon_basis(Float3::Z);
    let mut lins = Vec::new();

    let mut r = radius_min.max(radius_step);
    while r <= radius_max + 1e-4 {
        let samples = 64usize;
        let points: Vec<_> = (0..samples)
            .map(|i| {
                let theta = std::f32::consts::TAU * i as f32 / samples as f32;
                center + x * (r * theta.cos()) + y * (r * theta.sin())
            })
            .collect();
        push_closed_polyline(&mut lins, &points, normal);
        r += radius_step;
    }

    let mut theta = theta_min;
    while theta <= theta_max + 1e-4 {
        let end = center + x * (radius_max * theta.cos()) + y * (radius_max * theta.sin());
        lins.push(default_lin(
            center + x * (radius_min * theta.cos()) + y * (radius_min * theta.sin()),
            end,
            normal,
        ));
        theta += theta_step;
    }
    Ok(mesh_from_parts(vec![], lins, vec![]))
}
