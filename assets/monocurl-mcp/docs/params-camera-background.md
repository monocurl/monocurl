# Camera And Background

`camera` and `background` are built-in scene leaders. They have leader values
edited by code and follower values synchronized by `play`, like mesh leaders,
but they have special renderer meaning. `camera` controls the view and
`background` controls the clear color.

There are currently no user-declared scene leaders. Declare animated visible
content with `mesh`, and use references such as `&title`, `&camera`, and
`&background` when an animation or helper needs to sync or mutate a leader.

Use `CameraLerp` for camera movement. It interpolates camera orientation more
smoothly than a structural `Lerp`. Use `Set` when a scene leader should snap to
its current target value.

With the default camera at `4b` and `16 / 9` frame, the `z = 0` authoring plane
shows roughly `x = -4..4` and `y = -2.25..2.25`. Place most ordinary 2D content
inside that box unless the scene intentionally animates the camera.

```monocurl
import std.anim
import std.color
import std.mesh
import std.scene

background = WHITE

mesh object = Circle(1)

slide "Camera"
    play Set([&object, &camera, &background])

    camera = Camera([1.5, 0.8, 4])
    background = BLACK

    play CameraLerp(&camera, 1)
    play Set([&background])
```
