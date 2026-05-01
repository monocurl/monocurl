# Params, Camera, And Background

## Params

`param` works like `mesh`: it has a leader value, which code edits, and a
follower value, which is the live value synchronized by `play`. They are advanced features and not needed in the majority of cases. Params are also
exposed as interactive controls in presentation mode, so the viewer can adjust
them while the scene is paused or presenting. `param` declarations must be
top-level.

A plain read such as `radius` reads the current concrete leader value. The `$`
sigil creates a stateful reference to the live follower value, as in
`$radius`. Params are the only leader kind that can be referenced with `$`.

A stateful value continuously re-evaluates using the current follower values of
all params it depends on. This is what lets a visible mesh react while a
presentation slider changes. Stateful values may only be stored in `mesh`
leaders. Assigning one to `let`, `var`, or another `param` is a runtime error.

`$param` must appear inside a labeled function or operator call assigned to a
mesh leader, so the mesh can recompute from the live value. Arithmetic on
`$param` is valid as a sub-expression inside that labeled call, but the
outermost reactive expression must still be a labeled call.

```monocurl
param radius = 0.36
param delta = [-1, 0, 0]
# invalid patterns:
# let illegal = $radius (assignment to let)
# var also_illegal = Circle(radius: $radius) (binary op on stateful)
# param still_illegal = $radius

let Ball = |pos, radius, col|
    center{pos}
    fill{alpha{0.24} col}
    stroke{col, 2}
    Circle(radius)

let double = |x| 2 * x
slide "Parameters"
    # the ball leader depends on radius's live leader value
    mesh ball = Ball(pos: [0, 0, 0], radius: $radius, col: GREEN)
    # this materializes ball into its current evaluation based on the current evaluation of radius
    let ball_eval = ball
    mesh doubled = shift{$delta} Circle(radius: double($radius))
    play Fade(0.7)
    # now, the ball follower depends on the radius follower

    # this edits the parameter leader; Lerp moves the parameter follower
    # and ball recomputes from the live follower during the animation
    radius = 0.8
    play Lerp(1.0)

    # a plain read evaluates the leader concretely
    let current = ball
```

Stateful operators still expose labeled fields. Mutating a label edits the
non-stateful part of the leader, while the `$param` part continues to react:

```monocurl
param delta = 0l
mesh shifted = shift{delta: $delta} Circle(center: 0l)

shifted.center = 1l
delta = 3l
play Lerp(1)

let snap = shifted # concrete value, not a stateful reference
```

When followers are synced to leaders with `Set` or at the end of `Lerp`,
stateful leader expressions propagate to the follower, keeping the follower
reactive. Use params when a value should be exposed to presentation mode or
shared between scripted animation and live user controls.

## Camera And Background

`camera` and `background` are effectively scene-level params with special
renderer meaning. They can be assigned and synchronized like other leaders, but
`camera` controls the view and `background` controls scene color. Use
`CameraLerp` for camera movement. Use `camera_transfer{original_camera,
live_camera}` for meshes that should stay fixed relative to the frame while the
camera moves.

With the default camera at `4b` and `16 / 9` frame, the `z = 0` authoring plane
shows roughly `x = -4..4` and `y = -2.25..2.25`. Place most ordinary 2D content
inside that box unless the scene intentionally animates the camera.
`background` can be assigned and synchronized like any other scene leader, but
camera motion should usually use `CameraLerp` rather than plain `Lerp`.

```monocurl
mesh object = Circle(1)

# by reading the original and current values of camera and cleverly using
# camera_transfer, the mesh appears fixed in frame
mesh fixed_in_frame =
    camera_transfer{camera, $camera}
    shift{2r + 1u}
    Square(1)

camera = Camera([1.5, 0.8, 4])

# camera lerp is a specialized lerp for cameras that tries to make the overall
# motion more smooth
# as it moves, fixed_in_frame appears to stay in place because camera_transfer
# positions it perfectly relative to the new camera
play CameraLerp(&camera, 1)
```
