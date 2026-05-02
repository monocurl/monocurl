# Animations

## Leader/Follower Model

A `mesh` declaration creates a leader/follower pair:

- The leader is the value the script edits. Assignments and attribute edits
  change it immediately.
- The follower is the value currently visible in the viewport. It stays at the
  last synced state until a `play` statement tells it how to catch up.

Think of the leader as the next destination keyframe and the follower as the
current rendered frame. Assigning a new leader does not draw by itself.

At the end of init, leaders are implicitly synced to followers so zero-slide
still images and initial frames have visible state, but this is not true for slides. In slides, you must `play`
explicitly whenever the visible frame should update.

This is the most important animation rule: code changes the leader first, then
`play` decides how the follower catches up. Right after an assignment, code
reads the destination value, but the viewport still shows the previous follower
until the next `play`.

## Wait And Set

`Set` snaps followers to leaders immediately. `Wait` reserves time without
changing leaders or followers.

```monocurl
slide "Wait And Set"
    # follower starts as []
    mesh ball = Ball(pos: 1l, radius: 0.32, col: BLUE)
    play Wait(0.35) # reserves time; follower is still []

    # follower snaps to the current leader
    # it's not visible on screen as the same content as the leader
    play Set()
```

## Lerp

`Lerp` interpolates the follower toward the leader. It works best when the
leader and follower have the same shape. lerp(a, b, t) evaluates recursively under the following rules:

- equal values stay fixed.
- numbers interpolate linearly.
- same-length lists (and maps with same keys) interpolate element by element, including coordinate lists/colors.
- live calls to the same labeled function interpolate labeled arguments and
  rerun the function.
- live operators interpolate their labeled operator arguments.

```monocurl
mesh ball = Ball(pos: 1l, radius: 0.32, col: BLUE)
play Set([&ball])

ball.pos = 1r
ball.radius = 0.55
ball.col = ORANGE
play Lerp(0.9, [&ball])
```

`Lerp` is strongest when the expression is a labeled live function/operator. The
runtime interpolates the labels and reruns the call at each frame. This is why
`Ball(pos: ..., radius: ..., col: ...)` is better than an unlabeled positional
call in an animated scene.

Operators interpolate through their identity state. For example,
`shift{delta: 2r} Circle(0.5)` can lerp from the unshifted circle `Circle(0.5)` because the
operator supplies an identity value (`delta = 0r`) and a modified value
(`delta = 2r`). You rarely need to author this low-level pair yourself; build
custom operators from stdlib operators when possible.

Internally, a primitive interpolatable operator is shaped like this:

```monocurl
let shift = operator |target, delta| {
    let go = |d| __monocurl__native__ op_shift(target, d)
    return [go(0r), go(delta)]
}
```

That returned pair means "identity appearance" and "acted appearance" in such a way that you can interpolate from the initial and acted appearence. Most
scene code should avoid this low-level pattern and compose existing stdlib
operators instead.

Anyways, all this means that the following is a clearly defined animation for rotation.

```
mesh init = Square(1)
play Set()
init = rotate{TAU} init
play Lerp()
```
## Shape Morphs

Use `Trans` when source and destination differ in a way that is not interpolatable. Use `TagTrans` when you want greater control on the matching algoithm.

`Trans` uses contour matching based on geometry heuristics
`TagTrans` only allows matches between sources and destination meshes with the same tags. If you tag properly, this lets you

```monocurl
mesh pair = [
    tag{1} center{1l} Circle(0.25),
    tag{2} center{1r} Circle(0.25)
]
play Set([&pair])

pair = [
    tag{2} center{1l} Square(0.45),
    tag{1} center{1r} Circle(0.35)
]
play TagTrans(1, [&pair])
```

Use `Write` for drawing strokes/text on, `Grow` for appearing from a center,
and `Fade` for opacity entrances/exits. To fade out, assign the leader to `[]`
and play `Fade`.

`Write`, `Grow`, and `Fade` compare the current follower and destination leader.
They keep shared contours still, show newly added contours, and hide deleted
contours. This is why `note = []` followed by `play Fade(...)` fades the old
visible note out instead of snapping it away.

```monocurl
mesh note = Text("hello", 0.6)
play Write(0.8, [&note])

note = []
play Fade(0.4, [&note])
```

## Animation Blocks And Parallelism

Animation blocks package sequences. They do not execute until `play` is called.
When given a list of animations, `play` runs them in parallel and waits for all
branches to finish.

Target explicit leaders with `&leader` in nontrivial scenes, especially inside
parallel animation branches. Two parallel branches must not animate the same
leader at the same time.

Think of `anim { ... }` as a coroutine. It can contain assignments, prints,
and nested `play` statements, but none of that runs when the block is created.
The work starts only when the block itself is played.

```monocurl
let move_pair = anim {
    pair = shift{delta: 0.6u} pair
    play Lerp(0.9, [&pair])
}

let pulse = anim {
    ring = Circle(0.4)
    play Grow(0.4, [&ring])
    ring = []
    play Fade(0.4, [&ring])
}

play [move_pair, pulse]
```

For reusable animations, a lambda can return an `anim` block. Notice that these types of animations can actually cause leaders to change, instead of animations like Write. Such animations that cause changes to leaders are called progressors.

```monocurl
let MoveAndGrow = |&m| {
    let amount = 2r
    return anim {
        m = shift{amount} m
        play Lerp(1)
    }
}

play MoveAndGrow(&token)
```

## Rates

Rates remap animation time. Most primitive animations default to `smooth`.
Use `rate{...}` to override timing:

```monocurl
play [
    rate{linear} Lerp(1.2, [&left]),
    rate{bounce} Lerp(1.2, [&right])
]
```
