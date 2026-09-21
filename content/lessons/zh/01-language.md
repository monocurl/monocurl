# 语言基础知识

本课可供参考。查看重要部分，但在构建某些内容之前，您不需要记住这里的所有内容。当您遇到以前从未见过的功能时请回来。


## 绑定

用户绑定有以下三种形式：

- `let` — 不可变名称；不能重新分配
- `var` — 可变局部值
- `mesh` — 可见场景状态；创建一个领导者/跟随者对（动画课程中深入介绍）

```mcl
let radius = 0.45
var count = 0
mesh dot = fill{CYAN} Circle(radius)
```

## 动态打字

Monocurl 是动态类型的。变量可以保存任何值，并且可以重新分配给不同的类型。

```mcl transcript
var x = 7
print type_of(x)
x = "hello"
print x
x = [1, 2, 3]
print [type_of(x), len(x)]
```

创作时使用 `print` 来检查值。打印输出内联显示在编辑器和控制台中（与时间线交替）。

## 字符串和转义符

Monocurl 字符串使用 `%` 作为转义标记，而不是 `\`。这样 LaTeX 仍然易于编写：反斜杠是普通的字符串字符。

```mcl 
let newline = "first%nsecond"
let quoted = "say %"hello%""
let percent = "100%%"
```
## 深拷贝

分配始终执行**深复制**。

```mcl transcript
var a = [[1, 2], [3, 4]]
var b = a
b[0][0] = 99
print [a, b]
```

## 方向文字

Monocurl 具有用于公共方向的紧凑向量文字，通常用于定位。

```mcl transcript
let p = 1.5r + 0.8u       # [1.5, 0.8, 0]
let q = 2l + 1d           # [-2, -1, 0]
let behind = 3b           # [0, 0, 3]
print p
print q
print behind
```

- `l` / `r` — 左/右（负/正 x）
- `u` / `d` — 上/下（正/负 y）
- `b` / `f` — 向后/向前（正/负 z）

方向文字只是三元素列表。您可以将它们传递给 `shift{}` 或 `center{}`，并在需要向量的任何地方使用它们。

## 列表和地图

列表使用从零开始的索引。地图使用 `key -> value` 对。

>重要事项 追加是通过 `..` 和 `.=` 运算符支持的常见操作。

```mcl transcript
var pts = []
pts .= 1l
pts .= 1u
pts = pts .. 1r      # .. appends; .= is shorthand for x = x .. y

let labels = ["origin" -> ORIGIN, "right" -> 1r, "up" -> 1u]
print [pts, labels["origin"]]
```

## 块语法

`block {}` 是一个累积列表的多行表达式。以 `.` 开头的行附加到隐式返回值；整个块的计算结果为该列表。

```mcl
let row = block {
    for (i in range(0, 4)) {
        . center{[i - 1.5, 0, 0]} fill{CYAN} Square(0.4)
    }
    . center{2r} color{ORANGE} Text("end", 0.55)
}
```

## Lambda 和标记参数

函数是 lambda，被视为第一类。参数可以有默认值，支撑体可以使用 `return`。

```mcl transcript
let square = |x| x * x
let capture = 3
let weighted = |x, weight = 1| x * weight + capture
let hyp = |a, b| {
    return sqrt(a * a + b * b)
}
print square(5)
print weighted(4, weight: 2)
print hyp(3, 4)
```

>重要 **标记参数** 是为 Monocurl 动画系统提供支持的功能。当您调用带有命名参数的函数时，结果会记住这些标签，并且可以稍后逐个字段进行更改。然后使用新参数重新评估整个调用。

```mcl video
let Ball = |pos, radius, col|
    center{pos} fill{alpha{0.2} col} stroke{col, 2} Circle(radius)

mesh ball = Ball(pos: 1.4l, radius: 0.35, col: BLUE)

slide "move"
    # Mutate individual fields; the Ball(...) call is recomputed from them
    ball.pos = 1.4r
    ball.radius = 0.6
    ball.col = ORANGE
    play Lerp(1.5)
```

这不是通常意义上的对象突变。 `ball.pos = 1.4r` 编辑存储在实时 `Ball(...)` 调用中的带标签参数，然后整个调用使用新参数重新运行。这就是 `Lerp` 知道如何在两个状态之间平滑插值的方式：它独立地插值每个参数并在每一帧重建网格。

## 运营商

>重要操作符是在目标之前而不是在目标周围写入的函数。它们形成一个**管道**：每个操作员接收网格并将其转换到其右侧。它们与属性系统交互良好。

```mcl
mesh shape =
    center{pos: 1r}       # positions the result
    fill{alpha{0.18} BLUE} stroke{BLUE, 2}
    Circle(radius: 0.5)      # base constructor

# you can access inner attributes
shape.radius = 1.0
# or attributes of the operators
shape.pos = 1l
```

从右到左阅读：`Circle(0.5)` 创建几何体； `stroke`、`fill` 和 `center` 依次对其进行变换。您可以根据现有的运算符使用 `operator` 定义自己的运算符：

```mcl
let soft_style = operator |target, col|
    fill{alpha{0.2} col}
    stroke{col, 2}
    target

mesh shapes = [
    center{1.3l} soft_style{BLUE} Circle(0.45),
    center{1.3r} soft_style{ORANGE} Square(0.75)
]
```

内置运算符处理样式（`fill`、`stroke`、`color`、`alpha`）、定位（`center`、`shift`、`rotate`、`scale`）、布局（`next_to`、`to_side`）和标识（`tag`）。

运算符还带有动画语义，这将在稍后讨论。

## 控制流程

标准控制流：`if`/`else if`/`else`、`for`、`while`、`break`、`continue`。在将数据转换为网格之前，使用它们来构建数据。

```mcl transcript
var above = []
let values = [0.75, 1.35, 0.92, 1.52, 1.05]
for (v in values) {
    if (v > 1.1) {
        above .= v
    }
}
print above

# range(start, stop) or range(start, stop, step)
for (i in range(0, 5)) {
    print i
}

# destructuring
for ([i, v] in enumerate(["a", 1, 3.14])) {
    print ["index" -> i, "value" -> v]
}
```

递归 lambda 将自身视为显式 `self` 参数。这是 lambda 演算中的标准，但如果您以前没有见过它，它可能看起来很奇怪。

```mcl transcript
let fib = |self, n| {
    if (n <= 1) { return n }
    return self(self, n - 1) + self(self, n - 2)
}
print fib(fib, 8)
```

## 把它放在一起

典型的辅助函数会构建带标签的网格并使用运算符进行样式设置。带标签的参数成为动画关键帧字段。

```mcl image
background = LIGHT_GRAY

let Bar = |height, col, x|
    center{[x, height / 2, 0]}
    fill{soft{} col}
    stroke{col, 2}
    Rect([0.55, height])

mesh chart = block {
    let data = [0.6, 1.2, 0.9, 1.5, 1.1]
    for (i in range(0, len(data))) {
        let x = (i - 2) * 0.72
        let col = keyframe_lerp([0 -> BLUE, 0.5 -> CYAN, 1 -> GREEN], i / 4)
        . Bar(height: data[i], col: col, x: x)
    }
}
```
