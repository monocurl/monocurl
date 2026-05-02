# Language Basics

## Values And Assignment

Monocurl is dynamically typed. Assignments are deep copies, so ordinary value
assignment cannot create reference cycles. Values include numbers, strings,
lists, maps, lambdas, operators, meshes, animation blocks, and `nil`.

Use `let` for immutable names and `var` for mutable construction state:

```monocurl
let scale = 0.6
var points = []
points .= [-1, 0, 0]
points .= [1, 0, 0]
points = [points] # deep copy: now [[-1, 0, 0], [1, 0, 0]]

let colors = ["left" -> BLUE, "right" -> ORANGE]
let f = |x, y = 1| x + y
```

`x .. y` appends `y` to `x`. `x .= y` is shorthand for `x = x .. y`.
Deep-copy assignment is intentional: it keeps ordinary values simple and avoids
reference cycles. If you need mutation to affect a visible value over time, use
`mesh` / `param` leaders and `play`.

Common vector literals are `1l`, `1r`, `1u`, `1d`, `1f`, and `1b` for left,
right, up, down, forward, and backward. These are 3D vectors. In the default
camera, `1f` is negative z and `1b` is positive z.

## Control Flow

Use ordinary `if`, `for`, and `while` statements:

```monocurl
var total = 0
for (x in [1, 2, 3]) {
    if (x > 1) {
        total = total + x
        break
    }
}
```

Prefer building data first, then turning it into mesh values. This keeps
algorithm scenes readable.

## Lambdas

Use multiline lambdas when a helper needs multiple statements:

```monocurl
let capture = 6
let Path = |count| {
    var points = []
    for (i in range(0, count)) {
        let x = -2.6 + i * capture / (count - 1)
        var y = -0.7
        if (i // 2 * 2 == i) { y = -1.2 }
        points .= [x, y, 0]
    }
    return stroke{TEAL, 3} Polyline(points)
}
```

Lambdas may capture immutable `let` values. Mutable construction state and scene
leaders (`var`, `mesh`, `param`, `camera`, etc.) should not be captured. Pass
them explicitly, or use `&` reference parameters when the helper lambda needs to mutate
a mesh/param leader.

Recursive lambdas take themselves explicitly:

```monocurl
let fact = |self, n| {
    if (n <= 1) { return 1 }
    return n * self(self, n - 1)
}
let value = fact(fact, 5)
```

## Block Accumulation

`block { ... }` initializes the implicit accumulator `_` to `[]`; lines
beginning with `.` append to it, and `_` is returned at the end. Use this for
helpers that assemble many pieces:

```monocurl
let DotRow = |count, y = 0, col = BLUE| block {
    for (i in range(0, count)) {
        let x = i - (count - 1) / 2
        . tag{i}
            center{[x * 0.55, y, 0]}
            fill{alpha{0.22} col}
            stroke{col, 2}
            Circle(0.16)
    }
}
```

## Calls And Operators

Function calls use parentheses:

```monocurl
let f = |x| x * 2
let res = f(24 + 4)
```

Operator calls use braces before the target:

```monocurl
# shift is an operator acting on Circle
shift{2r} Circle(0.5)
```

Function and operator labels become meaningful fields on live calls. That means
later code can address `ball.pos`, `ball.radius`, `token.delta`, or `eq.tex`
instead of rebuilding an expression by positional argument order. Label
arguments that an animation may later mutate or interpolate.

```monocurl
let Ball = |pos, radius, col|
    center{pos}
    fill{alpha{0.24} col}
    stroke{col, 2}
    Circle(radius)

slide "Labels"
    # labels create readable attributes on the live function result
    mesh ball = Ball(pos: 1l, radius: 0.32, col: BLUE)
    play Set([&ball])

    # these update the labeled arguments, then Lerp interpolates them
    ball.pos = 1r
    ball.radius = 0.55
    ball.col = ORANGE
    play Lerp(0.9, [&ball])

    # operator labels work the same way
    mesh token = shift{delta: 1.2l} Circle(radius: 0.28)
    play Set([&token])

    token.delta = 1.2r
    token.radius = 0.44
    play Lerp(0.9, [&token])
```

In the example above, `ball.pos`, `ball.radius`, and `ball.col` are labels on
the `Ball(...)` call. `token.delta` is the label on the `shift{...}` operator,
while `token.radius` is the label on the nested `Circle(...)` call. Attribute
assignment edits those stored arguments; it does not mean meshes have built-in
fields named `pos` or `radius`.

Operators compose around a target. Custom operators are good for repeated style
chains:

```monocurl
let badge_style = operator |target, col|
    fill{alpha{0.18} col}
    stroke{col, 2.2}
    target
```

## set_default (Advanced)

`set_default{name, value}` is an operator that pre-fills a named argument on a
callable target without invoking it. It returns an identity/acted pair, so it is
lerp-compatible like any other operator. `set_defaults{map}` sets multiple
defaults at once.

This is how `axis_style` is implemented — it wraps an axis constructor and
pre-fills its `x_axis` or `y_axis` labeled argument:

```monocurl
# axis_style{"x", 0, 4, "x"} is roughly:
#   set_default{"x_axis", [0, 4, "x", ...]} Axis2d(...)
axis_style{"x", 0, 4, "x", 1, 4}
axis_style{"y", 0, 5, "f(x)", 1, 4}
Axis2d([1r * 0.5, 1u * 0.5])
```

Use `set_default` to build configuration operators that compose cleanly onto
constructors without calling them:

```monocurl
let time_axis = operator |target|
    set_default{"x_axis", [0, 10, "t", 1, 4, |x| x]}
    set_default{"y_axis", [0, 5, "v", 0.5, 4, |y| y]}
    target

time_axis{} Axis2d([1r * 0.45, 1u * 0.45])
```

Prefer `axis_style` over raw `set_default` for axis configuration. Use raw
`set_default` only when building a reusable operator that needs to pre-configure
a constructor's named arguments in a way that no existing stdlib operator covers.

## References

Reference parameters are prefixed with `&`. Use them when a helper needs to
mutate a mesh/param leader or return an animation that will animate it. Only
mesh and parameter leaders can be passed by reference.

```monocurl
# highlight is a stdlib helper that mutates its input leader, so the mesh must
# be passed by reference.
mesh x = Circle(0.5)
play Highlight(&x, YELLOW)

# transfer is also defined in stdlib; this is the core idea
let Transfer = |&from, &into| anim {
    into = [into, from]
    from = []
    play Set([&from, &into])
}

mesh y = []
play Transfer(&x, &y)

let MoveAndGrow = |&m| {
    # you can do setup immediately before returning the actual animation
    # `anim` is just a special expression; it does not run until played
    let amount = 2r
    return anim {
        m = shift{amount} m
        play Lerp(1)
    }
}

play MoveAndGrow(&y)
```

This pattern is powerful because the lambda can parameterize an infinite number of animations and let the caller decide when to `play` it.
Use this for reusable animation helpers that need to mutate one or more leaders. Animations will be discussed more shortly.
