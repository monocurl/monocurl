# 动画基础知识

Monocurl 动画是根据状态变化构建的。你改变了领导者； `play` 语句将追随者与具有所选策略的领导者同步。

## 领导者和追随者

`mesh x = value` 创建了两件事：

- **leader** — 您的代码读取和写入的变量，初始化为 `value`
- **follower** — 视口实际绘制的内容，初始化为 `[]` （空）

更改代码中的领导者是即时且无形的。只有 `play` 语句才能使跟随者赶上领导者，并且为 `play` 选择的动画控制“如何”到达那里。

编辑器中的视口显示任意时间点的关注者状态。当你清理时间线时，你是在问：“在执行过程中，追随者此时看起来是什么样子？”

```mcl video

mesh ball =
    center{1.6l}
    fill{soft{} BLUE}
    stroke{BLUE, 2}
    Circle(0.45)

slide "Leader And Follower"
    play Wait(0.5)

    # This assignment changes the leader immediately.
    # At this point viewport still shows the follower which hasn't caught up yet.
    ball =
        center{1.6r}
        fill{soft{} ORANGE}
        stroke{ORANGE, 2}
        Circle(0.45)

    play Wait(0.5)

    # Now Set() syncs the follower to the leader — the ball jumps.
    play Set()
    play Wait(0.8)
```

>重要特殊例外：在 **init** 结束时，所有网格领导者都会自动同步到其追随者。这就是为什么在场景首次打开时 init 中定义的网格已经可见。


## 放

`Set` 立即将每个肮脏的领导者复制给其追随者。使用它进行跳切或无需过渡即可显示新状态。

```mcl video

mesh ball =
    center{1.4l + 0.15d}
    fill{soft{} GREEN}
    stroke{GREEN, 2}
    Circle(0.45)

slide "Set"
    play Wait(1.0)

    ball = center{1.4r} fill{soft{} MAGENTA} stroke{MAGENTA, 2} Circle(0.55)
    play Set()
    play Wait(1.0)
```

一般来说，动画可以通过查看哪个网格体发生了变化来推断出您想要设置动画的网格体。在更复杂的场景中，您可以使用 `play Set([&ball])` 语法明确说出所需的变量。

## 莱普

`Lerp` 随着时间的推移插入兼容的值。 “兼容”通常意味着领导者和追随者是具有相同结构的相同函数调用，因此可以单独插入参数。确切的定义可以在文档中找到。

```mcl video

let Ball = |pos, radius, col|
    center{pos} fill{soft{} col} stroke{col, 2} Circle(radius)

mesh ball = Ball(pos: 1.4l, radius: 0.35, col: BLUE)

slide "Lerp"
    play Wait(0.6)

    ball.pos = 1.4r
    ball.radius = 0.6
    ball.col = ORANGE
    play Lerp(1.5)
    play Wait(0.8)
```

由于 `ball` 被定义为对 `Ball` 的带标签调用，因此您可以更改各个字段（`ball.pos`、`ball.radius`、`ball.col`），然后 `Lerp` 将每个参数独立地从旧值插值到新值。

>重要这是 Monocurl 中的核心关键帧动画模式：使用带标签的参数设计构造函数，然后改变特定字段以定义每个关键帧状态。

`Lerp` 和其他动画接受可选的 `rate` 修饰符以进行缓动：

```mcl
play Lerp(1.5, smooth)     # default: smooth S-curve
play Lerp(1.5, ease_out)   # decelerates
play Lerp(1.5, linear)     # constant speed
```

## 片头和片尾动画

有些动画适用于出现或消失的网格。他们将当前的追随者状态与新的领导者状态进行比较并动画化差异。

**写入** — 描绘新轮廓，就像手工绘制一样。最适合曲线和文本。

```mcl video
slide "Write"
    mesh curve = stroke{BLUE, 3} ExplicitFunc(|x| sin(x * TAU), [-1.5, 1.5, 100])
mesh label = center{1.2d} color{BLUE} Text("sin(2πx)", 0.55)
    play Write(1.2)
    play Wait(0.8)
```

**淡入淡出** — 使网格淡入或淡出。
**增长** — 从中心向外扩展几何体。有利于形状的出现。

```mcl video

slide "Grow And Fade"
    mesh shapes = [
        center{1.3l} fill{soft{} BLUE} stroke{BLUE, 2} Circle(0.42),
        center{0r} fill{soft{} ORANGE} stroke{ORANGE, 2} Square(0.7),
        center{1.3r} fill{soft{} GREEN} stroke{GREEN, 2} RegularPolygon(5, 0.44)
    ]
    play Grow(1.0)
    
    mesh label = center{1.1d} color{GRAY} Text("three shapes", 0.55)
    play Fade(0.8)
    play Wait(0.6)

    # can also be used for hiding!
    shapes = []
    label = []
    play Fade(0.9)
```

## 跨式和标签跨式

**Trans** 是通用网格变换。它通过使用成本函数匹配轮廓，将当前追随者转变为当前领导者。当 `Lerp` 不可能时（因为领导者和跟随者结构不同），可以使用它。

```mcl video

mesh shape = fill{soft{} CYAN} stroke{CYAN, 2} Triangle(1.2l, 1.2r, 1.0u)

slide "Trans"
    play Wait(0.8)

    shape = fill{soft{} ORANGE} stroke{ORANGE, 2} Circle(0.75)
    play Trans(1.2)
    play Wait(0.8)

    shape = Rect([2.0, 1.2])
    play Trans(1.2)
    play Wait(0.6)
```

**TagTrans** 是 `Trans` 的特化，它限制匹配具有相同标签的片段。这使您可以对轮廓匹配过程进行细粒度控制。

```mcl video

mesh pair = [
    tag{1} center{1.1l} fill{soft{} BLUE} stroke{BLUE, 2} Square(0.55),
    tag{2} center{1.1r} fill{soft{} ORANGE} stroke{ORANGE, 2} Circle(0.45)
]

slide "TagTrans"
    play Wait(0.8)

    # Without TagTrans, Trans might match the square to the right circle
    # and the circle to the left square. TagTrans forces tag-1 to tag-1
    # and tag-2 to tag-2, so each shape moves and transforms independently.
    pair = [
        tag{2} center{1.1l} fill{soft{} ORANGE} stroke{ORANGE, 2} Circle(0.5),
        tag{1} center{1.1r} fill{soft{} BLUE} stroke{BLUE, 2} Circle(0.5)
    ]
    play TagTrans(1.4)
    play Wait(0.8)
```

当标签在状态之间更改名称时，将 `tag_map` 传递给组源标签列表
与目标标签列表。仅影响匹配；它不会重写网格。

```mcl transcript
# full tag list [1, 2] matches full tag list [3, 4]
let one_to_one = [[1, 2] -> [3, 4]]
print one_to_one[[1, 2]]

# full tag lists [1] and [2] both match full tag list [3]
let many_to_one = [[[1], [2]] -> [[3]]]
print many_to_one
print many_to_one[[[1], [2]]]
```

## Lerp 与操作员

`Lerp` 不仅仅适用于带标签的函数参数。基于运算符的表达式也可以进行插值，因为运算符知道自己的身份状态。

例如，从 `x` 插值到 `rotate{angle} x` 可以使网格平滑旋转。

```mcl video

mesh tri =
    center{ORIGIN}
    fill{soft{} BLUE}
    stroke{BLUE, 2}
    Triangle(0.8l, 0.8r, 0.9u)

slide "Rotate With Lerp"
    play Set()
    play Wait(0.5)

    tri = rotate{PI} tri
    play Lerp(1.8)
    play Wait(0.8)
```

这同样适用于 `shift`、`scale`、`color` 和大多数其他运算符。一般模式是：`mesh = operator{args} mesh` 将领导者设置为已操作版本，`Lerp` 从跟随者（未操作）插值到领导者（已操作）。

您还可以将其链接起来以动画显示一系列操作员应用程序：

```mcl video

mesh box =
    center{1.5l}
    fill{soft{} BLUE}
    stroke{BLUE, 2}
    Square(0.55)

slide "Shift And Rotate"
    play Set()
    play Wait(0.5)

    box = shift{3r} rotate{PI / 3} box
    play Lerp(1.8)
    play Wait(0.6)
```

## 选择动画

- **设置**——即时捕捉；用于跳接和初始展示
- **Lerp** — 平滑插值；当领导者和追随者具有相同结构时使用
- **写** — 追踪；用于新曲线、路径和文本
- **成长**——扩张；用于新的填充形状
- **淡入淡出** — 不透明度；用于出现或消失的几何体
- **Trans** — 一般变形；当结构发生显着变化时使用
- **TagTrans** — 标记变形；当多个独立部分各自需要各自的对应部分时使用
