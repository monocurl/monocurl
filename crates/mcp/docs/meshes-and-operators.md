# Meshes And Operators

## Mesh Values

Meshes are the visible values. Primitive constructors usually create canonical,
origin-based geometry. Place and style them with operators.

Lists of meshes, including nested lists, are valid mesh values:

```monocurl
let mesh_tree = [Circle(1), [Square(1), Circle(1)]]
```

Treat mesh lists as "trees" of meshes.

## Tags And Filters

Tags are stable identities attached to mesh leaves. Use `tag{...}` when later
animations, filters, or styles need to know which subpart is which. Tags should
be numbers or lists of numbers, not strings.

Many operators take an optional filter as their last argument. A filter receives
the tag list of the candidate mesh leaf and returns whether that subpart should be affected by the current operator:

```monocurl
# creating a custom operator that can be invoked
let tag_palette = operator |target|
    fill{alpha{0.28} MAGENTA, |tags| 3 in tags}
    fill{alpha{0.28} ORANGE, |tags| 2 in tags}
    fill{alpha{0.28} BLUE, |tags| 1 in tags}
    stroke{BLACK, 2}
    target

let subset = tag_filter{|tag| len(tag) > 0} data
```

## Text Tags

`text_tag{...}` is the text/Tex/Latex version of `tag{...}`. It wraps a string
or fragment before LaTeX is converted into mesh geometry. In the resultant mesh, the contours that were generated from that specific text will be assigned the tag.

```monocurl
let algebra_line = |left, middle, right| [
    text_tag{1} left,
    " + ",
    text_tag{2} middle,
    " = ",
    text_tag{3} right
]

mesh eq =
    color{BLUE, |tags| 1 in tags}
    color{ORANGE, |tags| 2 in tags}
    Tex(algebra_line("x^2", "2x", "(x+1)^2 - 1"), 0.9)
```

Internally, `text_tag{2} "2x"` simply aliases to
`\text_tag{2}{2x}`; `\tag2{2x}` is a custom latex shortcut as well. Use tagged fragments for
equation transforms and selective formula styling. They're also used for animations. You can add multiple tags via `text_tag{1, 2, 3} "x^2"`.

Use `Text` for plain text, `Tex` for math fragments, and `Latex` for full
LaTeX fragments. Text is still mesh geometry, so it can be styled, tagged, and
animated.
