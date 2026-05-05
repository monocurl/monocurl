# Debugging, Patterns, And Examples

Use `print <expr>` for debugging and quick inspection. It evaluates an
expression and appends the value's string form to the execution transcript.

```monocurl
let points = [1, 2, 4]
print points
print ["count" -> len(points)]
```

The editor shows transcript lines inline under the statement and in the bottom
console panel. Use `print` intentionally; repeated prints inside frequent
updates or loops can flood the transcript.

## Patterns And Anti-Patterns

### Prefer

- Start from the standard imports unless the scene deliberately uses only a
  subset.
- Keep init for imports, helpers, custom operators, params, background, camera,
  and initial leaders.
- Indent all slide bodies, including the first slide.
- Build scenes from small helper lambdas that return mesh values.
- Construct simple shapes at the origin, then place/style them with operators.
- Label semantically important arguments such as `pos:`, `radius:`, `delta:`,
  `tex:`, `col:`, `count:`, `phase:`, `src:`, and `dst:`.
- Label operator arguments when later code may mutate them, such as
  `shift{delta: 2r}` or `rotate{radians: PI / 4}`.
- Use `block` / dot accumulation for repeated mesh assembly.
- Use numeric tags or numeric-list tags on pieces that must persist across
  transforms, especially repeated dots, formula terms, graph regions, and
  objects animated with `TagTrans` or `TagBend`.
- Assign destination states first, then call `play`.
- Target specific leaders with `&mesh` in nontrivial scenes.
- Use `print` for sanity checks while authoring algorithmic visuals.
- Use `Write` for strokes/text drawing on, `Grow` for appearing from center,
  `Fade` for opacity entrance/exit, `Lerp` for labeled argument interpolation,
  `Trans` for general mesh morphs, and `TagTrans` for identity-preserving
  morphs.
- Pass mutable scene leaders such as `camera` into helpers instead of capturing
  them in lambdas.

### Avoid

- Do not explicitly specify the variables for animations when they can be inferred (i.e. only one animation running on all of the changed variables). Try to structure and intersperse changes to leaders and play calls so that you can avoid specifying the variables as much as possible
- Do not call `__monocurl__native__` directly from scenes; use stdlib wrappers.
- Do not put `play` statements before the first `slide`.
- Do not leave first-slide statements unindented.
- Do not capture mutable values or scene leaders in lambdas; pass them
  explicitly.
- Do not pass centers/normals to constructors that do not expose them; use
  `shift`, `center`, `in_space`, or other operators.
- Do not mutate the same mesh leader from two parallel animation branches.
- Do not rely on unlabeled positional arguments for complex calls when labels
  make later animation clearer.
- Do not use string tags. Use numbers or lists of numbers.
- Do not use deterministic `random` / `randint` inside helper lambdas; they are
  only available in the root frame.
- Do not hand-author low-level mesh topology for ordinary scenes; use stdlib
  constructors/operators and mesh queries.

## Complete Example

Read `monocurl://examples/riemann-rectangles` for a complete scene. It is a
good reference for standard imports, helpers in init, slide indentation, graph
coordinate transforms, tagged rectangles, `text_tag` formula pieces,
transcript prints, and multi-slide animation flow.

## Formatting Conventions

### Indentation

- Use four spaces for slide bodies, blocks, loops, and conditionals.
- Put one statement per line unless a very short `if` branch is clearer on one
  line.
- When a mesh expression spans multiple lines, put each operator on its own
  indented line and put the constructor or target last.

```monocurl
mesh disk =
    center{[0, 0.3, 0]}
    fill{alpha{0.25} BLUE}
    stroke{BLUE, 2}
    Circle(0.6)
```

### Naming

- Use `lower_snake_case` for local values, mesh leaders, params, numeric/data
  helpers, and most custom operators: `graph_origin`, `live_radius`,
  `embed_graph`, `tag_palette`.
- Use `PascalCase` for mesh factory helpers that behave like constructors:
  `Label`, `Panel`, `Ball`, `FlowScene`, `RecursiveTree`.
- Use `ALL_CAPS` for top-level constants that are configuration values:
  `BASE`, `HEIGHT`, `START`, `GOAL`, `ORIGINAL_CAMERA`.
- Use short lowercase names only for tiny math helpers or loop-local values:
  `f`, `pt`, `col`, `idx`.
- Prefer semantic mesh names over visual-only names: `title`, `axes`, `graph`,
  `rects`, `measure`, `field`, `trail`.

### Labels

Label arguments that later code may mutate or interpolate. This is one of the
most important Monocurl idioms.

```monocurl
mesh ball = Ball(pos: 1l, radius: 0.32, col: BLUE)
play Set([&ball])

ball.radius = 0.55
ball.col = ORANGE
play Lerp(0.9)
```

Use labels for operator arguments too:

```monocurl
mesh token = shift{delta: 1.2l} Circle(radius: 0.32)
play Set()

token.delta = 1.2r
token.radius = 0.48
play Lerp(0.9)
```

### Comments

Use comments to explain intent or non-obvious semantics, not every line of
mechanics. Tutorial scenes use comments before important transitions, animation
model details, and debugging prints.
