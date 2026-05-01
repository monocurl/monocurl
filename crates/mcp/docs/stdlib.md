# Monocurl Stdlib Documentation

The Monocurl standard library resources exposed by this MCP server are public
language-level wrappers over native runtime functions. Prefer these wrappers
when authoring scenes; calls named `__monocurl__native__ ...` are
implementation details.

For exact signatures, examples, and per-function notes, read the raw wrapper
resources under `monocurl://stdlib/*`. Those files contain structured `##`
documentation blocks next to the public wrapper definitions, and are usually
the best reference when authoring or debugging a scene.

Most scenes import the full public surface:

```monocurl
import std.util
import std.math
import std.color
import std.mesh
import std.anim
import std.scene
```

## Modules

- `std.util`: collection helpers, range and sampling helpers, string and
  conversion utilities, type predicates, runtime errors, and live/default
  argument introspection helpers.
- `std.math`: scalar math, vector math, interpolation helpers, statistics,
  and combinatorics.
- `std.color`: named colors and color manipulation helpers.
- `std.mesh`: primitive constructors, text/Tex/LaTeX constructors, graphing
  helpers, mesh styling/layout operators, tags, filters, topology queries, and
  mesh transforms.
- `std.anim`: rate functions, primitive animations, follower animations,
  indication animations, transfer animations, and animation composition/time
  operators.
- `std.scene`: scene-level camera and background helpers.

## Authoring Notes

- Construct geometry first, then place/style/tag it with operators such as
  `center`, `shift`, `fill`, `stroke`, `tag`, `scale`, `rotate`, `to_side`,
  and `to_corner`.
- Most primitive meshes are canonical and origin-based. Use `shift`/`center`
  for placement, and use `in_space{origin, x_unit, y_unit, z_unit}` when a mesh
  is authored in a local coordinate system such as graph axes and must be
  embedded into the global scene.
- `mesh` and `param` declarations create leaders whose follower values can be
  animated. Pass references like `&title` to animations.
- Animation helpers eventually lower to primitive animations such as `Wait`,
  `Set`, and `Lerp`. Higher-level helpers like `Grow`, `Fade`, `Write`,
  `Trans`, `TagTrans`, `Bend`, and `CameraLerp` are the preferred public API.
- Use `Lerp` when the leader and follower are the same live expression shape.
  Use `Trans`/`TagTrans` for mesh topology changes, and use `CameraLerp` for
  camera movement because it interpolates camera orientation more smoothly than
  a structural value `Lerp`.
- `uprank` and `downrank` change mesh topology rank: dots to lines to filled
  surfaces, and back down to boundaries/dots. They are geometry tools, not
  z-order controls.
- Text-like constructors are `Text`, `Tex`, and `Latex`. Use `text_tag{...}`
  when fragments need stable tags for later transforms.
- Scene-level `background` and `camera` are ordinary top-level names with
  special meaning to the renderer.

## MCP Resources

This MCP server also exposes the raw wrapper source for each module:

- `monocurl://stdlib/util`
- `monocurl://stdlib/math`
- `monocurl://stdlib/color`
- `monocurl://stdlib/mesh`
- `monocurl://stdlib/anim`
- `monocurl://stdlib/scene`
