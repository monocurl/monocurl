## Basics
```monocurl
import std.mesh
import std.util
import std.anim

let var = 10
let list = [1, 2, 3, 4, 5]
let dict = [:]
let dict2 = ["hello" -> 1, "world" -> 2]
let expr = 3 * 5 + 2
# brace and line major
for i in list {
    let q = i
    if q < 4 {
      break
    }
    else {
      continue
    }
}
while 0 {
  idk
}

if cond {
    let x = 4
} 
else if other_cond {
    let x = 5
}
else {
    let x = 6
}
```

## Function
```monocurl
let f = |arg1, arg2, arg3| arg1 + arg2 + arg3
let f = |arg1, darg1=test| arg1 + darg1
let f = |arg1, arg2| {
    return arg1 * arg2
}
let g = f(2, 3)
```

### Dot Syntax
```monocurl
var x = []
x = x .. 4
x .= 4
x .= 5
var f = || block {
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

## Operators 
In addition to being able to interpolate from structs of the same type, you can also interpolate from expressions of g to operator x, such that g and x can be interpolated.

```monocurl
# can omit parentheses for single argument operators
# right associative
let y = sticky (expr)
let z = sticky stroke{RED} (expr)

# declaring operators (a special type of function)
# must specify the identity embed and the act function
# possibly can just have a helper function for ones that have identity parameter
let shift = operator |operator, delta| {
  let internal = |d| {
    __native_shift(operator, d)
  }
  [internal(shift: [0,0,0]), internal(shift: delta)]
};

# in terms of of other operators
let composite = operator |target| {target, shift(LEFT) target}
```

## Labels
Thinking about it now, how about everything is the form of a labeled argument? Functions can be called via long syntax. And maybe we make rust style expressions? We can do yield as returning from a braced expression. Basically any function can be called comma major (with parenthesis). In either, you are able to use labeled arguments.

```monocurl
let g = Group (
  point: Circle(
      radius: origin
      children: block {
        let x = 4 + 5
        . Circle()
        . Circle() 
      }
  )
  scale: 5
)
g.point.radius = "test"
g.point = Circle(0, [])
```

## Params and Stateful Values
params are the only leader kind that can be referenced via `$`. a stateful value continuously re-evaluates using the current follower values of all depended params.

`$x` - stateful reference to param `x`'s follower; only valid where a reactive expression is intended
plain `x` - read the current concrete leader value of a param or mesh variable

stateful values can only be stored in `mesh` leaders. assigning to `let`/`var`/`param` is a runtime error.

```monocurl
# must be declared at the top level
param x = 2
mesh g = Circle(center: 0l, radius: $x)
play Set()
play Wait(5)
# user can adjust x via the gui slider; g reacts live

# arithmetic in sub-expressions is allowed
mesh h = Circle(center: 0l, radius: $x * 2)
play Set()

# plain read to get current concrete value
let current = g

# change x in code
x = 5
play Lerp(1)

# attribute access on stateful operators
param delta = 0l
mesh shifted = shift{delta: $delta} Circle(center: 0l)
shifted.center = 1l   # mutate the center arg directly
let snap = shifted    # evaluate leader to concrete
```

## Mesh
mostly same as is, except declared with mesh keyword instead of tree
```monocurl 
mesh x = []
play Set()
mesh y = Circle(0, 1)
play [slow{0.5} Set()]
play wait(1)
```

## Play / Animation Declaration
```
parallel animations (only thing left?)
let x = |&reference_var, args| anim {
  # regular anim where you can play
  play ...
}

play x(reference_var, 12)
# equivalent to below
play [x]
play [x, y]
```

parallel animations may cause contention,
in general we ask that the two animations be fully independent and ultimately commute.
If parallel animations cannot mutably write to the same variable, there is no conflict whatsoever guaranteed. We can just lerp all the variables that this animation "owns". If two parallel animations write to the same variable, it will be marked as an error. If two animations need to animate different parts of the same variable, we can simply contour separate / whatever and transfer to temporary meshes, and transfer back once done

## Print / Transcript
```monocurl
let x = 42
print x + 1
```

`print <expr>` is a statement. It evaluates the expression and appends the value's string form to the execution transcript. The editor can render transcript entries inline below the source line, and the bottom panel can show the same output as a console.

## Slides
All this means is a pause / cache point. Stuff before the first slide is special and no play statements are allowed
```monocurl
# previous animations
slide
# current slide animations

# optional same-line title for the timeline
slide "Intro"
# current slide animations
```

## Import
At some point, will allow parameterized imports? Need to decide how the diamond problem will work though 
```monocurl 
import std.mesh
```
