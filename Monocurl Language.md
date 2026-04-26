High level idea: You can use the Monocurl programming language to specify commands that ultimately control a viewport for a video or slideshow. This is done by running a "scene" file, which is the top level module that imports "library" files, that themselves may import other library files. Library files cannot be run/rendered directly; only scene files can. When editing a scene file, it is continually renderered up to the current timestamp of the editor so that a live preview of the scene contents can be shown.

Monocurl is line major, like python, and brace major, like C. You can infer the exact grammar via the lexer (crates/lexer/src/{lexer,token}.rs) and parser (crates/parser/src/{ast,parser}.rs) or (/Syntax.md); what follows is meant to highlight the semantics. It is an untyped lambda calculus.

You can declare constants via let, and mutable variables via var. `let x = 2` `var y = 4\n y = 6`.
There's many binary and unary operators as with other languages.

Functions are done via lambdas, with syntax similar to rust. 
```
let f = |x, y| x * y
let g = |x, y, z = 5| {
  return x * y + z;
}
let res = g(2, 4)
let res2 = g(2, 4, 7)
```
Control flow: there are if, for (over a container), and while loops
```
if (x < 4) {
  y = 4
}
else if (y < 5) {
  y = 6
}
else {
  for (label in vector) {
    y = y + 5
  }
  while (y < 10) {
    y = y + 1
  }
}
```
You can do a "block" to make inline multiline expressions
```
let y = block {
  let tmp = 5;
  return tmp + 6;
}
```

Dot syntax is a shorthand for appending to a vector, and is useful for building up pipelines of values implicitly via the `_` variable. Each block implicitly initializes `_` to `[]` and returns it at the end, so dot-appending to `_` accumulates a list of values without needing an explicit variable.
```
var x = []
x = x . 4
x .= 4
x .= 5
var f = || {
    . lerp 
    . normalize
    # equivalent to 
    _ .= lerp
    # intermediate statement
    let g = 4
    _ .= translate(1, 2, 3)
    # implicit return at end of every function
    # return _
}
```

The primitives are ints, doubles, vectors, maps
```
let y = []
let g = [->]
let z = [4->5] # map
```

There are specialized literals for directions, degrees, etc (see parser for more details).
```
1l == [-1, 0, 0]
1r == [1, 0, 0]
1u == [0, 1, 0]
1d == [0, -1, 0]
1f == [0, 0, -1]
1b == [0, 0, 1]
```

Function Invocation. You can invoke functions as normal, but you can also do a labeled invocation. In a labeled invocation, you label some of the arguments. This allows you to refer to the arguments later as attributes. This allows you to change the argument to the function, and the entire result is recomputed.
```
let f =  |x, y| x * (y + 1)
var inv = f(a: 2, 1) # equal to  4
inv.a = 3 # equal to 6 now
```

Operators are a syntactic sugar to allow for easy application of common modifications. You can declare them with the following syntax. Basically you have a function that returns an identity on the target object and also returns the operated object. This defines a way to interpolation from x to operator(x) (by interpolating between the two values returned by operator)
```
# declaring operators (a special type of function)
# must specify the identity embed and the act function
let shift = operator |target, delta| {
  let internal = |d| {
    __native_shift(target, d)
  }
  # "identity" application, and the actual transformed version
  # we can interpolation between the two since internal is the same function
  [internal([0,0,0]), internal(delta)]
};
```

This can then easily be invoked by specifying the non target as arguments, and putting the operator before the target
```
let g = shift(3l) org
## kind of same as __native_shift(org, 3l)
## can also have labels
var h = shift(d: 3l) org
h.d = 4
## labels can persist across an operator
var q = shift(d: 3l) Circle(center: 0l, radius: 1)
q.radius = 2
q.center = 1l
```

Interpolation is extremely important in animation, so Monocurl makes it very powerful. Here are the interpolation rules.
1. If a == b, then `lerp(a, b, t) = a`
2. If a and b are numbers, then `lerp(a, b, t) = (1-t)*a + t*b`
3. If a and b are function invocations of the same function `f` (i.e. a = f(x1, x2), b = f(y1, y2)), and all of the corresponding arguments in both invocations are interpolatable then lerp(a, b, t) = f(lerp(x1, y1, t), lerp(x2, y2, t), ...). For example, `lerp(Circle(center: 0l, radius: 1), Circle(center: 1l, radius: 2), t)` produces a circle whose center and radius are each linearly interpolated — this works because both values are `Circle` invocations with interpolatable arguments.
4. If a = O(a0, args) for some operator O and a0 and b are interpolatable, then lerp(a, b, t) = lerp(O(lerp(a0, b, t), args)[0], O(lerp(a0, b, t), args)[1])
5. Similar to 4, but flipped
You can see how the labels provided before allow for easy interpolation between new states.

An anim block is similar to async await in javascript. It's effectively a coroutine. You can use the play keyword to play a sequence of animations in parallel. Note that there are primitive animations that you can play as well (to end the recursion). This will be discussed shortly.
```
let a = anim {
  var y = 0
  # this is a coroutine so nothing has been done yet
  play lerp()
  play wait(1)
  # parallel invocation (toy example)
  play [wait(1), lerp()]
} 
# only now, will the code in a be executed
play a
```

Most users will mostly work with scene files, but are able to create their own library files if necessary. As mentioned, scene file can import library files. A singular scene file can be rendered either into a video or a slideshow. A scene file is divided into different slides, where each slide contains several animations. When rendered as a video, the animations of each slide are played, and then the same for next slide, and so forth. When presented as a slideshow, the animations of a slide are played upon hitting the space bar, and then the scene is paused at the end of the slide, until the next slide is played and so forth.

Within any scene, the current state is the camera position and orientation, the background color, and the list of meshes. Meshes: the core on screen primitve of Monocurl is a mesh. In fact, anything on screen is a mesh. A mesh is based on a double half edge adjacency list. That means it can contain oriented tetrahedrons, along each face of the tetrahedron, the neighbor is recorded: it can either be the index of another tetrahedron, or there's no other tetrahedron here, it will be negative, and correspond to the index of the triangle that borders it. Likewise for triangles on all of its sides, with an important note being that if a triangle is not the border of a tetrahedron, it has a partner triangle in the opposite orientation as its neighbor. Similar for lins with dots. In practice, tetrahedrons aren't rendered so they don't actually need to be represented, just tris, lins, and dots. See geo/src/mesh.rs for more details of this. Each vertex of a triangle and others can contain a color. A mesh can also have uniform information, such as color, gloss, texture. According to all of its constituent geometric objects and uniform information, a mesh can be rendered into a viewport with respect to the current camera.

The main point of Monocurl then is to control the scene state at each time. To do this, we take advantage of keyframes. The idea is you only have to set the state at various keyframes, and we can interpolate between these different keyframes in the intermediate times. The details of that are the leader-follower paradigm. 

### Leader Follower
In Monocurl, you can declare variables using let for constants, and var for mutable variables. However, there are a few other special variables used for coordinate with the visible screen. The first of these is `mesh`. This creates two effective variables, one that you interact with in your code, and one actively on screen. The on screen one is initially empty, and the code one is whatever you initialied it to. The code one is named as the leader, and the on screen one is the follower.

In any animation, you synchronize the followers to the leaders, under some specific strategy. The most basic one is set. Here the synchronization is done instantly.
```
mesh active = Circle(0l, 1)
play Wait(1)
# instantly, a circle appears on screen
play Set()
```

The most common one is lerp. In this case, the follower is interpolated to the leader after some time frame. After the animation finished, all followers match all leaders. In some cases, you can explicitly specify the subset of state variables that should be synchronized (instead of all of them), but this is usually only needed for parallel animations.
```
mesh active = Circle(0l, radius: 1)
play Set()
# detached leader
active.radius = 2
# in three seconds, the radius expands to the final radius
play lerp(3)
```

### Parameters and Stateful Values
`param` works like `mesh` — it has a leader value (what your code sees) and a follower value (the live on-screen value), synced via the same animation primitives. `param` values are also exposed as interactive sliders during presentation mode so the viewer can adjust them in real time. Parameters must be declared at the top level.

The `$` sigil (e.g. `$x`) creates a **stateful reference** to the live follower value of a param variable. A stateful reference can only appear as an argument to a labeled function or operator call; the resulting value is called a **stateful value**. A stateful value continuously re-evaluates its expression using the current follower values of all params it depends on.

Stateful values may only be stored in `mesh` leaders — assigning one to a `let`, `var`, or `param` variable is a runtime error. You can use the dereference operator `*` on a mesh variable whose leader is a stateful expression to evaluate it immediately and obtain a concrete value. Attribute access on a stateful value works the same way as on a live operator (destructures the labeled call node).

Arithmetic on `$` references is valid as a sub-expression inside a labeled call (e.g. `Circle(r: $radius * 2)`), but the outermost expression must always be a labeled call.

```
param radius = 0
# mesh that depends on radius's live follower value
mesh target = Circle(center: 0l, radius: $radius)
play Set()
# get the current concrete value of target's leader expression
let current = *target
radius = 5
play Lerp(3)
# as the follower of radius increases, the follower of target reacts live

# live operator example
param delta = 0l
mesh target2 = shift{delta: $delta} Circle(center: 0l)
# can still access labeled arguments on stateful operators
let c = target2.center   # returns the center arg of the Circle
target2.center = 1l      # mutates the center arg in place
delta = 3l
play Lerp(1)
# target2 is now updated to respect the new value of delta

# *target gives the evaluated leader value (concrete); plain mesh_center(target) errors
let current2 = *target2
```

When followers are synced to leaders (via Set or at the end of a Lerp), stateful leader expressions propagate to the follower, keeping it reactive.
