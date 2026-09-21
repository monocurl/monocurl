# 高级主题

本课程涵盖的主题对于日常场景创作来说不是必需的，但当您想要自定义 Monocurl 的行为或了解幕后发生的情况时非常有用。

## 自定义库

您可以将通用定义提取到库文件中。库文件的结构几乎类似于场景文件，但只有库部分导入到其他文件中。库文件的主要目的是提供可在多个场景中使用的辅助函数、自定义运算符、常量和可重用的网格定义。

您可以使用 `import` 关键字将库文件导入到场景文件中。导入路径相对于导入文件，不应包含 `.mcs` 扩展名。

导入文件时，Monocurl 仅导入该文件的第一个 `slide` 关键字之前的代码。出于导入目的，第一张 `slide` 之后的每张幻灯片都会被忽略，包括第一张幻灯片之后出现的任何导入。

这允许帮助程序文件在其库定义下方保留一个小型演示场景，并且对于单独编辑库文件非常有用。

```mcl transcript
let Double = |x| 2 * x

slide "demo"
    print Double(4)
```

导入它的另一个文件可以使用 `Double`，但演示幻灯片不会编译到导入场景中。

## 有状态值和参数

有状态值是依赖于场景状态并不断更新的值。有状态值的主要位置是相机感知覆盖。 `camera_transfer{camera, $camera}` 在相机移动时保持网格相对于框架固定（阅读文档以获取完整说明），并且 `orient_to_camera{$camera}` 不断朝相机旋转平面网格树。然而，除此之外，应该避免有状态，并且更惯用的做法是让网格体具有您显式更新的属性。

```mcl video
mesh fully_fixed =
    camera_transfer{camera, $camera}
    to_side{1u, 0.2}
    Text("fixed in frame")
mesh rotationally_fixed = 
    orient_to_camera{$camera}
    to_side{1d, 0.2}
    Text("rotationally fixed")
mesh not_fixed = Text("not fixed")

slide "Camera"
    camera = Camera([2, 1, 5])
    play CameraLerp(&camera, 1.2)
```

状态源是 `param`，其工作方式与 `mesh` 类似：它具有代码编辑的领导者值和动画播放的跟随者值。顶级参数也在演示模式下作为交互式控件公开。

最常见的参数是 `camera` 和 `background`，它们有点特殊，因为它们实际上影响视觉场景，其他参数通常用于控制网格。

通常读取参数，例如 `radius`，读取其当前的引导值。使用 `$`（例如 `$radius`）读取它，会创建对实时值的有状态引用。您可以将其视为随着值的变化而不断地重新评估表达式（尽管在实践中它更有效）。

您只能将状态值分配给网格。您可以对它们执行的操作有限。您可以将它们用作函数参数、运算符参数和列表。在大多数情况下您也可以访问属性。当您向网格体分配有状态值并通过任何动画同步网格体时，同步不是“纯粹的”同步。相反，领导者网格将使用领导者参数进行评估，而跟随者网格将使用跟随者参数进行评估。这意味着通过设置参数动画，您将更改屏幕上的相关网格。

下面是一个玩具示例，因为它可以很容易地使用属性来完成，但说明了如何使用参数/有状态。更常见的用例是当许多变量自然依赖于一种场景状态时或者当您想要在演示模式下更改参数值时。
```mcl
param radius = 0.36

let Bubble = |radius, col|
    fill{alpha{0.2} col}
    stroke{col, 2}
    Circle(radius)

slide "Parameter"
    mesh bubble = Bubble(radius: $radius, col: BLUE)
    # referencing bubble nakedly uses the CURRENT value
    # so copy does not statefully depend on bubble!
    # try doing $bubble instead to see what happens
    mesh copy = bubble
    
    # the bubble leader depends on the radius leader
    # the bubble follower depends on the radius follower
    play Fade(0.4)

    # This edits the parameter leader.
    radius = 0.8
    # if you inspect bubble (leader) right now
    # it will be a circle

    # now we'll synchronize the radius follower
    # even though bubble was not changed since the last synchronization
    # its follower depends on the radius follower
    # so the bubble will change 
    # on the other hand, `copy` "elided" the stateful value
    # and no longer depends on radius so it remains the same
    play Lerp(1.0)
```

最后，我们注意到，尽管有状态不能执行许多操作（例如加法），但它可以作为任何函数的参数。在幕后，每次重新评估时，都会使用状态参数的当前值来调用该函数。这允许您绕过此限制，尽管更详细。

```mcl image

param radius = 2

mesh circle = fill{CLEAR} Circle($radius)
# won't work, can't do * on a stateful value
# mesh square = Square(2 * $radius)
# ... but you can use it as argument to any function
let double = |x| 2 * x
mesh square = fill{CLEAR} Square(double($radius))
```

## 高级乳胶

默认情况下，Monocurl 使用捆绑的 LaTeX 后端。如果您需要系统 LaTeX 安装包或字体，桌面设置可以切换到自定义系统 `latex` 加 `dvisvgm` 后端。 CLI 具有匹配的 `--system-latex` 标志。这允许您使用默认捆绑包未提供的 Latex 中的功能。

请注意，与许多其他语言不同，Monocurl 使用 `%` 作为字符串的转义字符而不是 `\`，因此在编写 LaTeX 时不必使用双转义反斜杠。

`Text` 用于文字文本。 `Tex` 用于普通数学片段。 `Latex` 用于更完整的 LaTeX 正文片段，并接受包或字体声明的 `additional_preamble` 参数。

`Tex` 和 `Latex` 返回网格几何体，因此可以像其他网格一样对它们进行样式设置、标记、过滤和动画处理。当仅部分渲染表达式需要稳定标识时，请在文本输入中使用 `text_tag{...}`。

```mcl
mesh eq = Tex([text_tag{1} "x", " + ", text_tag{2} "1"], 0.8)

slide "Equation"
    play Write(0.8, [&eq])

    eq = Tex([text_tag{2} "1", " + ", text_tag{1} "x"], 0.8)
    play TagTrans(1.0, [&eq])
```


如果 `Tex(...)` 调用无法呈现，记录将显示 LaTeX 编译器输出。最常见的原因是缺少包和字符串参数中的 LaTeX 语法无效。

## 自定义操作符及其幕后工作原理

回想一下，运算符是接收目标并返回转换后的目标的函数。它们是在它们操作的东西之前编写的，这使得它们非常适合可重用的样式和放置管道。

```mcl
let soft_badge = operator |target, col|
    fill{alpha{0.18} col}
    stroke{col, 2}
    target

mesh markers = [
    center{1.2l} soft_badge{BLUE} Circle(0.35),
    center{1.2r} soft_badge{ORANGE} Square(0.6)
]
```

运算符的一个关键属性是，对于许多运算符，您可以在 `x` 和 `op{} x` 之间进行转换。例如，以下内容是有效的
```mcl
mesh org = Triangle(0l, 1u, 1r)
play Set()
org = rotate{180dg} org
play Lerp()
```

虽然大多数时候您可以根据 stdlib 运算符定义自己的运算符，但您也可以构建具有自己的插值行为的自定义运算符。原语运算符返回“身份”和“被操作”值。标识值应该“看起来”像未修改的操作数，但包含允许它直接用“操作”值进行插值的附加属性。

例如，这是 `rotate` 的实现方式：
```mcl
let rotate = operator |target, radians, axis = 1b, pivot = nil, filter = nil| {
    let go = |angle| __monocurl__native__ op_rotate(target, angle, axis, pivot, filter)
    return [go(angle: 0), go(angle: radians)]
}
```
为了提高效率，实际的旋转是由本机 rust 函数完成的，但要点是我们返回两个值。第一个旋转零，这是身份状态。第二个旋转所需的量。在大多数计算中，运算符仅被视为第二个返回值。但是，当在 `x` 和 `rotate{180dg} x` 之间进行 lerp 时，Monocurl 会发现这应该是一个运算符 lerp，并查看 `go(angle: 0)` 和 `go(angle: 180dg)` 之间的恒等值和“实际上”lerp，这可以通过传统插值来完成。

## 原始动画

动画模型建立在领导者/跟随者同步的基础上。代码立即编辑领导者； `play` 告诉关注者如何赶上。

最低级别的公共包装器是 `PrimitiveAnim(time, &vars, embed, lerp, rate)`。更高级别的动画，例如 `Lerp` 和 `Trans` 最终会简化为原始动画。

`PrimitiveAnim` 指定跟随者应如何同步到领导者。这个想法是，您可以提供可能与 lerp 不同的自定义插值函数。例如，以下是 CameraLerp 在 stdlib 中的定义方式。

```mcl
let CameraLerp = |&camera, time = 1, rate = smooth| {
    let embed = |start, dst| __monocurl__native__ camera_lerp_embed(start, dst)
    let value_lerp = |start, end, state, t| __monocurl__native__ camera_lerp_value(start, end, t)
    return PrimitiveAnim(time, &camera, embed, value_lerp, rate)
}
```

大部分繁重的工作都是用 Rust 完成的，但我们仍然可以了解一般流程。 embed 函数预处理开始和结束，返回 `[mod_start, mod_end, embed_state]`。这对于像 Trans 这样需要执行昂贵的匹配算法的动画很有用，因此我们宁愿在开始时只执行一次。插值函数接收 embed 的参数以及归一化的 t 值，并要求根据所需的行为进行插值。在 `CameraLerp` 的情况下，这相当于执行球面插值以使观看更加自然。
