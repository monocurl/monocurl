# 并行动画

到目前为止，每个 `play` 语句都会在下一个动画开始之前运行一个动画直至完成。本课程介绍如何同时运行动画、如何构建可重用的动画助手以及何时传递显式引用。

## 并行播放

将列表传递给 `play` 同时运行所有项目。该场景会等待每个分支完成。

```mcl video

mesh circle = center{1.4l} fill{soft{} BLUE} stroke{BLUE, 2} Circle(0.42)
mesh label = center{0.9d} color{BLUE} Text("parallel", 0.55)

slide "Parallel"
    play [
        Write(1.2, [&label]),
        Grow(1.0, [&circle])
    ]
    play Wait(0.5)

    circle = center{1.4r} fill{soft{} ORANGE} stroke{ORANGE, 2} Circle(0.55)
    label = center{0.9d} color{ORANGE} Text("done", 0.55)
    play [
        Trans(1.2, [&circle]),
        Write(1.0, [&label])
    ]
```

每个分支都应该拥有自己的网格。在同一个网格上运行多个并发动画将导致运行时错误。

## 动画块

`anim {}` 创建一个动画块 - 代码和 `play` 语句的延迟序列。该块在定义时不执行任何操作；它仅在传递给 `play` 时运行。

```mcl transcript
mesh ball = Square(1)

slide "Animation Block"
  let DoSomething = anim {
      print "inside block: start"
      ball = shift{2r} ball
      play Lerp(1.2)
      play Wait(0.4)
      ball = shift{2l} ball
      play Lerp(1.2)
      print "inside block: done"
  }
  
  print "block has been defined"
  play DoSomething
  print "slide code resumed"
```

动画块的行为类似于协程（在其他语言中也称为 Promise）：播放时，它们逐步执行，在每个 `play` 语句处产生。这就是为什么 `anim {}` 块内的 `print` 仅在播放头经过该行后出现在脚本中。

## 进步者

**progressor** 是一种习惯用法，其中 lambda 通过引用接受领导者并在动画块中对其进行变异。 `&` 语法传递引用而不是副本。

```mcl video

let MoveBy = |&m, delta| anim {
    m = shift{delta} m
    play Lerp(1.2, [&m])
}

mesh ball = center{1.5l} fill{soft{} BLUE} stroke{BLUE, 2} Circle(0.42)
mesh pulse = []

slide "Progressors"
    play Set()
    play Wait(0.5)

    let pulse_ring = anim {
        pulse = fill{CLEAR} stroke{CYAN, 3} Circle(0.35)
        play Grow(1.3)
        pulse = fill{CLEAR} stroke{CLEAR, 3} Circle(0.9)
        play Trans(1.5)
    }

    play [
        MoveBy(&ball, 3r),
        pulse_ring
    ]
```

`MoveBy` 采用 `&ball` （对领导者的引用）和 `delta`。在块内， `m = shift{delta} m` 通过引用改变领导者。当 `MoveBy` 作为并行列表的一部分播放时，它与 `pulse_ring` 同时执行。

进度条允许您与动画并行修改场景状态。

## 显式引用

动画引擎通常会推断哪些领导者是脏的并且应该进行动画处理。但它具有侵略性，并且将所有肮脏的领导人聚集在一起。在两种情况下你应该明确：

**1.您更改了领导者，但不希望在此动画中出现它。** 传递 `[&specific_leader]` 将动画限制为仅该领导者。

**2.并行分支都会修改相关的几何图形。**明确哪个分支拥有哪个领导者可以防止意外干扰。

```mcl
# Without explicit refs, Lerp would animate both ball and label together
ball.pos = 1.4r
label = next_to{ball, 1d, 0.2} Text("moved", 0.55)

play [
    Lerp(1.2, [&ball]),    # ball moves smoothly
    Set([&label])           # label snaps to new position instantly
]
```

## 延迟

当动画在并行块内开始时，`delay{}` 修改器会偏移。

```mcl video

slide "Stagger"
    mesh a = center{1.4l} fill{soft{} BLUE} stroke{BLUE, 2} Circle(0.38)
    mesh b = center{0r} fill{soft{} ORANGE} stroke{ORANGE, 2} Circle(0.38)
    mesh c = center{1.4r} fill{soft{} GREEN} stroke{GREEN, 2} Circle(0.38)
  
    play [
        Fade(0.8, [&a]),
        delay{0.2} Fade(0.8, [&b]),
        delay{0.4} Fade(0.8, [&c])
    ]
    play Wait(0.6)
```

看看你是否能想到如何实施延迟。

## 演练：3D 相机动画

要查看这些模式协同工作，请考虑 Monocurl 附带的 3D 相机动画示例。以下是您如何构建它的推理。

目标是：显示一个平坦的彩色网格，然后同时将其提升到一个表面并围绕它运行相机。

**第 1 步 — 在 init 中构建初始状态。**

网格开始平坦，相机从默认位置开始观察原点。定义颜色函数并设置网格。

```mcl
let samples = 24
let height = |x, y| 1.15 * ((x - 0.5)^2 + (y - 0.5)^2)
let color_keys = [0 -> BLUE, 0.15 -> YELLOW, 0.3 -> ORANGE, 0.55 -> RED]

let color_at = |pos, idx| keyframe_lerp(color_keys, height(pos[0], -pos[1]))
```

**第 2 步 — 第一张幻灯片：展示初始场景。**

```mcl
slide "Flat Grid"
    mesh grid = stroke{BLACK, 1.5} ColorGrid(
        |pos, idx| BLACK,
        [0, 1, samples],
        [-1, 0, samples]
    )
    mesh axis = Axis3d(basis: [1r, 1d, 1b], color: BLACK, [1u, 1u, 1b])

    play [Fade(0.8, [&axis, &grid]), Write(0.8, [&title])]
```

**第 3 步 - 第二张幻灯片：提起网格并平行移动相机。**

每个操作都是一个 `anim {}` 块，具有自己的内部步骤。他们在一个 `play [...]` 中一起运行。

```mcl
slide "Surface And Camera"
    let lift_grid = anim {
        # First, recolor the flat grid
        grid = stroke{BLACK, 1.5} ColorGrid(color_at, [0, 1, samples], [-1, 0, samples])
        play Trans(0.8, [&grid])

        # Then, lift vertices to match the height function
        grid = point_map{|p| [p[0], p[1], height(p[0], p[1] + 1)]} grid
        play Trans(1.8, [&grid])
    }

    let move_camera = anim {
        camera = Camera([2.2, -2.1, 1.45], [0.5, -0.5, 0.35], [0, 0, 1])
        play CameraLerp(&camera, 2.6)
    }

    play [lift_grid, move_camera]
```

关键见解：`lift_grid` 和 `move_camera` 是完全独立的。 `lift_grid` 拥有 `grid`； `move_camera` 拥有 `camera`。因为它们不共享领导者，所以它们可以并行运行而不会受到干扰。

`CameraLerp` 是一种专门用于相机移动的动画，它比普通的 `Lerp` 相机位置产生更美观的弧线。

```mcl video
let samples = 24
let height = |x, y| 1.15 * ((x - 0.5) ^ 2 + (y - 0.5) ^ 2)
let color_keys = [0 -> BLUE, 0.15 -> YELLOW, 0.3 -> ORANGE, 0.55 -> RED]

let color_at = |pos, idx| {
    let value = height(pos[0], -pos[1])
    # keyframe_lerp turns a scalar field into a smooth multi-stop surface gradient.
    return keyframe_lerp(color_keys, value)
}

slide "Flat Grid"
    mesh grid = stroke{BLACK, 1.5} ColorGrid(
        |pos, idx| BLACK,
        [0, 1, samples],
        [-1, 0, samples]
    )
    mesh axis =
        shift{[0, 0, -0.01]} # draw below function
        axis_style{"x", 0, 1, "x"}
        axis_style{"y", 0, 1, "y"}
        axis_style{"z", 0, 1, "z"}
        Axis3d(
            basis: [1r, 1d, 1b],
            color: BLACK,
            grid_color: LIGHT_GRAY,
            [1u, 1u, 1b]
        )
    mesh monocurl = center{0.7u} Text("Monocurl", 2)

    play [Fade(0.8, [&axis, &grid]), Write(0.8, &monocurl)]

slide "Surface And Camera"
    # an anim block is analogous to a coroutine in other languages
    # it does nothing until it is played
    # (you can try experiment with the playhead to see when the
    # "Got here" is actually printed)
    let lift_grid = anim {
        grid = stroke{BLACK, 1.5} ColorGrid(
            color_at,
            [0, 1, samples],
            [-1, 0, samples]
        )
        play Trans(0.8, [&grid])

        print "Got here"

        grid =
            point_map{|point| [point[0], point[1], height(point[0], point[1] + 1)]}
            grid
        play Trans(1.8, [&grid])
    }

    # camera lerp is a specialize anim for interpolating 
    # camera positions, lerp "works" as well 
    # but is visually less pelaseing
    let move_camera = anim {
        camera = Camera([2.2, -2.1, 1.45], [0.5, -0.5, 0.35], [0, 0, 1])
        play CameraLerp(&camera, 2.6)
    }


    play [lift_grid, move_camera]
    play Wait(0.4)
```
