## Basics
```monocurl
import std.mesh
import std.util
import std.anim

let var = 10
let arr = [1, 2, 3, 4, 5]
let dict = [:]
let dict2 = ["hello":1, "world":2]
let expr = 3 * 5 + 2
# brace and line major
for i in arr {
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

## Operators 
In addition to being able to interpolate from structs of the same type, you can also interpolate from expressions of g to operator x, such that g and x can be interpolated.

```monocurl
# can omit parentheses for single argument operators
# right associative
let y = sticky {expr}
let z = sticky stroke(RED) {expr}

# declaring operators (a special type of function)
# must specify the identity embed and the act function
# possibly can just have a helper function for ones that have identity parameter
let shift = operator |operator, delta| {
  let internal = |d| {
    __native_shift(operator, d)
  }
  [internal([0,0,0]), internal(delta)]
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

## State
conceptually, instead of thinking about stuff as showing meshes, we show funtions that map from state/parameters to meshes.

in the leader follower pattern, after any animation, all leaders and all followers will become synchronized. state also "subscribes" to the leader / follower pattern.

camera will actually be state, but you will be allowed to just move the camera entirely, without the meshes moving in response. Theoretically this can be changed if we set camera to a parameter though.

```monocurl
state x = 1
play Set()
let y = $x + 5 # live value
let gamma = f($x, 5) # live value = f(x, 5) at any time
let z = *x # current / dereferenced value
# this must be a non stateful value
mesh tree = Circle(2, $x)
play Write()
x = 2
// lerps x over one second, 
play Lerp

# equivalent to 
mesh tree = Circle(2, r: 1)
play Write()
tree.r = 2
play Lerp()
```

```monocurl
## must be declared on the top level, unlike state
param x = 2
mesh g = Circle($x, 2)
play Set()
play Wait(5)
# user can change x on the gui slider if they want to, and it will react live

# change x in code
x = 5
# trying to change x while the animation is running will inevitably lead to weird results
# we can honestly disable changing it while an animation owns it, in the same way that we disable
# parallel animations from acting
play Lerp
```

## Mesh
mostly same as is, except declared with mesh keyword instead of tree
```monocurl 
mesh x = {}
play Set()
mesh y = Circle(0, 1)
play {slow(0.5) Set()}
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
play {x}
play {x, y}
```

parallel animations may cause contention,
in general we ask that the two animations be fully independent and ultimately commute.
If parallel animations cannot mutably write to the same variable, there is no conflict whatsoever guaranteed. We can just lerp all the variables that this animation "owns". If two parallel animations write to the same variable, it will be marked as an error. If two animations need to animate different parts of the same variable, we can simply contour separate / whatever and transfer to temporary meshes, and transfer back once done

## Slides
All this means is a pause / cache point. Stuff before the first slide is special and no play statements are allowed
```monocurl
[slide]
```

## Import
At some point, will allow parameterized imports? Need to decide how the diamond problem will work though 
```monocurl 
import std.mesh
```
