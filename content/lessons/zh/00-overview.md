# 概述

本课程将介绍编辑器、场景的结构以及如何从编写代码到导出或展示您的作品。

## 编辑

![The Monocurl editor](/img/home/monocurl-editor.png)

编辑器具有三个工作界面：

- **源编辑器**（左）——您在其中编写 `.mcs` 场景文件
- **视口**（右上角）- 显示当前时间线位置处的渲染帧
- **时间轴**（右下）- 浏览幻灯片以及每张幻灯片中的各个 `play` 步骤

当您编辑代码时，Monocurl 会重新计算，并且视口预览会根据时间轴中的当前位置进行更新。

## 场景结构

每个场景都包含三个部分：导入、初始化部分和幻灯片。为了简洁起见，我们在大多数示例中省略了导入。

```mcl 
import std.util
import std.math
import std.color
import std.mesh
import std.anim
import std.scene

# --- init section ---
# runs first; sets up helpers and the initial visible state

mesh dot = center{ORIGIN} fill{soft{} CYAN} stroke{CYAN, 2} Circle(0.4)

slide "intro"
    # slide containing some animations
    mesh title = center{0.8u} Text("Hello", 0.7)
    play Write(0.9)

slide
    dot = center{1.2r} fill{soft{} ORANGE} stroke{ORANGE, 2} Circle(0.5)
    play Lerp(1.2)
```

第一个 `slide` 之前的代码是 **init**。这是实时导入的地方，还有常量、辅助函数和起始可见状态。在第一个 `slide` 关键字之后，代码属于该幻灯片并且可以包含 `play` 动画。

## 第一个场景

这是一个完整的小场景。模式是：在 init 中设置某些 helpers，然后在每张幻灯片中通过播放动画以连续的方式改变场景状态。

```mcl video
slide "intro"
    mesh circle = 
      center{pos: 1.4l} 
      color{col: BLUE}
      Circle(0.4)
    mesh label = 
      center{pos:1.4l + 0.75d} 
      Text(text: "hello", 0.65)
    # introduce the newly created meshes in an animated fashion
    play [Write(0.9, [&label]), Fade(0.9, [&circle])]

slide "transform"
    circle.pos = 1.4r
    circle.col = ORANGE
    label.pos = 1.4r + 0.75d
    label.text = "world"
    # transform both meshes into new state
    play Trans(1.2)
```

## 时间轴导航

对于细粒度的控制，您可以单击寻找。但一般来说，使用键盘在场景中移动：

- `,` / `.` — 上一张/下一张幻灯片
- `<` / `>` — 跳转到场景开始/结束
- `;` / `'` — 向后/向前小步


创作时的一个有用习惯是在添加新幻灯片后清理时间线，以验证每个 `play` 步骤是否符合您的预期。

## 呈现和导出

可以通过三种方式使用相同的 `.mcs` 源文件：

- **视频导出** — 文件菜单 → 导出视频。将整个场景渲染为 `.mp4`。
- **图像导出** — 文件菜单 → 导出图像。将单个帧渲染为 `.png`。
- **演示模式** — `Cmd/Ctrl-P`。将幻灯片变成导航检查点，在每个 `slide` 边界处暂停。

## 交互式工作流程

!vid[](/video/interactive-development.mp4)


在演示模式下，`Cmd/Ctrl-T` 打开**参数面板**，您可以在其中使用滑块编辑一些场景状态。这是更先进/利基的，但也很强大。

在预览和演示模式下，您可以拖动光标来移动相机。如果按下 Shift 并拖动光标，则可以平移摄像机。这些对于构建 3D 场景特别有帮助。

## 网络上的单卷发

[Monocurl Essays](https://www.monocurl.com/monocurl-essays/) 演示了 Monocurl 场景如何直接在 Web 上运行。底层运行时也可以作为 [NPM 包](https://www.npmjs.com/package/monocurl) 提供。
