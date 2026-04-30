# Cheat Sheet

This is not the full API. For the full list, read the MCP stdlib resources:
`monocurl://stdlib/mesh`, `monocurl://stdlib/anim`,
`monocurl://stdlib/util`, `monocurl://stdlib/math`,
`monocurl://stdlib/color`, and `monocurl://stdlib/scene`.

## Common Imports

```monocurl
import std.util
import std.math
import std.color
import std.mesh
import std.anim
import std.scene
```

## Common Mesh Constructors

- Basic 2D: `Dot`, `Circle`, `Annulus`, `Square`, `Rect`, `RegularPolygon`,
  `Polygon`, `Polyline`, `Line`, `Arrow`, `Arc`, `Capsule`, `Triangle`,
  `Bezier`.
- Basic 3D: `Sphere`, `RectangularPrism`, `Cylinder`, `Cone`, `Torus`, `Plane`,
  `Vector`, `HalfVector`.
- Text and labels: `Text`, `Tex`, `Latex`, `Number`, `Label`, `Brace`,
  `Measure`.
- Layout: `Stack`, `XStack`, `YStack`, `ZStack`, `Grid`, `Table`,
  `BoundingBox`.
- Graphs and fields: `Axis1d`, `Axis2d`, `Axis3d`, `PolarAxis`, `ColorGrid`,
  `LineGrid`, `Field`, `ParametricFunc`, `ExplicitFunc`, `ExplicitFunc2d`,
  `ImplicitFunc2d`, `ExplicitFuncDiff`.
- Media: `Image`.

## Common Mesh Operators

- Placement: `shift`, `center`, `scale`, `rotate`, `in_space`, `next_to`,
  `matched_edge`, `to_side`, `to_corner`, `camera_transfer`, `projected`.
- Styling: `fill`, `stroke`, `color`, `fade`, `dotted`, `dashed`, `gloss`,
  `textured`, `z_index`.
- Identity and subsets: `tag`, `text_tag`, `tag_filter`, `tag_split`,
  `tag_map`, `subset_map`, `contour_separate`.
- Geometry transforms: `point_map`, `color_map`, `uv_map`, `uprank`,
  `downrank`, `wireframe`, `subdivide`, `tesselated`, `extrude`, `revolve`.

## Common Animations

- Timeline control: `Wait(time)`, `Set([&mesh])`.
- Interpolation: `Lerp(time, [&mesh], rate)`, `PrimitiveAnim(...)`.
- Entrances/exits: `Grow`, `Fade`, `Write`.
- Shape morphing: `Trans`, `TagTrans`, `Bend`, `TagBend`.
- Camera: `CameraLerp(&camera, time)`.
- Indication and movement helpers: `Highlight`, `Flash`, `Transfer`, `Copy`,
  `TransferSubset`, `CopySubset`, `TransSubsetTo`, `TransSubsetCopy`.
- Composition and timing: `LaggedMap`, `delay{...}`, `rate{...}`, `slow{...}`,
  `fast{...}`.
- Common rates: `linear`, `smooth`, `identity`, `bounce`, `elastic`,
  `ease_in`, `ease_out`, `ease_in_out`.

## Common Utilities

- Lists and loops: `range`, `sample`, `sample_clopen`, `len`, `map`, `filter`,
  `reduce`, `zip`, `enumerate`, `reverse`, `sort`, `take`, `drop`,
  `list_subset`, `flatten`.
- Aggregates: `all`, `any`, `count`, `sum`, `product`, `max_of`, `min_of`.
- Math: `PI`, `TAU`, `ORIGIN`, `LEFT`, `RIGHT`, `UP`, `DOWN`, `FORWARD`,
  `BACKWARD`, `sqrt`, `sin`, `cos`, `min`, `max`, `clamp`, `lerp`,
  `keyframe_lerp`, `map_range`, `deg_to_rad`, `rad_to_deg`.
- Colors: `RED`, `ORANGE`, `YELLOW`, `GREEN`, `TEAL`, `CYAN`, `BLUE`,
  `PURPLE`, `MAGENTA`, `WHITE`, `LIGHT_GRAY`, `GRAY`, `DARK_GRAY`, `BLACK`,
  `CLEAR`, `rgb`, `hsv`, and `alpha`.
- Type/introspection: `type_of`, `is_list`, `is_map`, `is_mesh`,
  `is_callable`, `has_attr`, `get_attr`, `set_attr`, `get_defaults`,
  `set_default`, `set_defaults`, `runtime_error`.
- Scene: `Camera`, `DEFAULT_CAMERA`, `DEFAULT_BACKGROUND`.
