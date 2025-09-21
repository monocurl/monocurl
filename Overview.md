# Monocurl Rust Innovations

## Syntax
- inline rust-style lambda let u = |x| x * x
- | instead of += ? (and maybe auto ret)
- ints and doubles
- tensors (automatically deduced with vectors when size is large enough)
- optional arguments instead of the stupid union stuff we're doing now

## CLI Mode
- have a way to run using CLI

## Gui
- playback selector in presentation mode
- much better syntax highlighting
- parameter variables if applicable
- might make it just a single text blob, allows for better cli mode as well
  - we'll have to be intelligent about keystroke detection and seeing slides remove or added

## State Variables (+ Parameters)
- state uuu = 0
- need to decide if initialization is special (probably)
  - initialization is special, and any future modifications should be set explicitly?
  - yeah so basically state variables are always updated in any animation
  - and you can only specify the mesh variables to be updated (never state)
    - although background and camera may be a bit special, lets differentiate as intermediate and sentinel / visible variables

## Monads
- returns the sentinel and initial state?
- so that it can easily be interpolated
- fully figured out!

## GUI Interaction
- Manually pan / scroll camera while editing or presenting?
- We already have parameters, but placement is still annoying, open question
- gui selection can be done via special slides?
- gui positioning could maybe be a later feature by having semantic labels for certain parameters?
  - it is complex so it can be done later, just make sure it's theoretically possible
- fully planned out!

## Module system
- import functions / monads and functors (structs) from other modules
- parameters for any import allow you to set a theme
```python
import std_util
import std_mesh {
  "THEME_0": RED
}
import std_anim
```

## Open questions
[x] how do parameters work though, how to implement that is
    - purely treating it as state variable might not be correct...
    - it must only be referenced in functions or something, it shouldn't be referenced at the top level ideally, or we just assume they dont
    - warning / error? for using parameter at the top level somehow, probably the answer
      - error to reference parameters during creation time, only should be used in animation time?
[x] diamond problem for imports
  - in general use overrides, but error if there's conflicting options?
  - probably dont allow functions or anything not directly comparable for the parameters?
[x] how do braces work for syntax
  - lets just do it like rust, ensuring we also have break and continue
[ ] text (+ interpolation)
  - t2c and t2tag could be useful in general similar to manim?, something like tex to style
  - transform matching shapes can still be done
  - transform matching tex via an animation that takes in that struct specifically
  - TBD
  - easily reference a subset
  - fundamentally difficult problem
