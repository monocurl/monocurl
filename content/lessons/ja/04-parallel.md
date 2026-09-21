# 並列アニメーション

これまでのところ、各 `play` ステートメントは 1 つのアニメーションを完了まで実行してから、次のアニメーションが開始されます。このレッスンでは、アニメーションを同時に実行する方法、再利用可能なアニメーション ヘルパーを構築する方法、および明示的な参照を渡すタイミングについて説明します。

## 並行してプレイする

リストを `play` に渡すと、すべての項目が同時に実行されます。シーンはすべてのブランチが終了するまで待機します。

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

各ブランチは独自のメッシュを所有する必要があります。同じメッシュ上で複数のアニメーションを同時に動作させると、ランタイム エラーが発生します。

## アニメーションブロック

`anim {}` は、コードと `play` ステートメントの遅延シーケンスであるアニメーション ブロックを作成します。ブロックは定義されても何もしません。 `play` に渡された場合にのみ実行されます。

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

アニメーション ブロックはコルーチン (他の言語では Promise とも呼ばれます) のように動作します。再生されると、`play` ステートメントごとに段階的に実行されます。このため、`anim {}` ブロック内の `print` は、再生ヘッドがその行を通過した後にのみトランスクリプトに表示されます。

## 進歩者

**プログレッサー** は、ラムダが参照によってリーダーを受け入れ、アニメーション ブロック内でそれを変更するイディオムです。 `&` 構文は、コピーではなく参照を渡します。

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

`MoveBy` は `&ball` (リーダーへの参照) と `delta` を受け取ります。ブロック内で、`m = shift{delta} m` は参照を通じてリーダーを変更します。 `MoveBy` が並列リストの一部として再生される場合、`pulse_ring` と同時に実行されます。

プログレッサーを使用すると、アニメーションと並行してシーンの状態を変更できます。

## 明示的な参照

通常、アニメーション エンジンは、どのリーダーがダーティでアニメーション化すべきかを推論します。しかし、それは攻撃的であり、すべての汚いリーダーをまとめて活気づけます。 2 つの場合、明示的にする必要があります。

**1.引出線を変更しましたが、このアニメーションでは変更したくありません。** アニメーションをその引出線だけに制限するには、`[&specific_leader]` を渡します。

**2.平行な分岐はどちらも関連するジオメトリを変更します。** どの分岐がどの引出線を所有するかを明示することで、偶発的な干渉を防ぐことができます。

```mcl
# Without explicit refs, Lerp would animate both ball and label together
ball.pos = 1.4r
label = next_to{ball, 1d, 0.2} Text("moved", 0.55)

play [
    Lerp(1.2, [&ball]),    # ball moves smoothly
    Set([&label])           # label snaps to new position instantly
]
```

## 遅れ

`delay{}` モディファイアは、アニメーションが並列ブロック内で開始されるときにオフセットします。

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

遅延をどのように実装できるかを考えられるかどうかを確認してください。

## ウォークスルー: 3D カメラ アニメーション

これらのパターンが連携して動作することを確認するには、Monocurl に同梱されている 3D カメラ アニメーションの例を検討してください。構築を通じて推論する方法は次のとおりです。

目標は、平らな色付きグリッドを表示し、同時にそれをサーフェスに持ち上げて、その周りでカメラを周回させることです。

**ステップ 1 — init で初期状態を構築します**

グリッドは平らに始まり、カメラは原点を見てデフォルトの位置から始まります。カラー関数を定義し、メッシュを設定します。

```mcl
let samples = 24
let height = |x, y| 1.15 * ((x - 0.5)^2 + (y - 0.5)^2)
let color_keys = [0 -> BLUE, 0.15 -> YELLOW, 0.3 -> ORANGE, 0.55 -> RED]

let color_at = |pos, idx| keyframe_lerp(color_keys, height(pos[0], -pos[1]))
```

**ステップ 2 — 最初のスライド: 最初のシーンを公開します。**

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

**ステップ 3 — 2 番目のスライド: グリッドを持ち上げてカメラを平行に動かします。**

各アクションは、独自の内部ステップを持つ `anim {}` ブロックです。これらは 1 つの `play [...]` で一緒に実行されます。

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

重要な洞察: `lift_grid` と `move_camera` は完全に独立しています。 `lift_grid` は `grid` を所有します。 `move_camera` は `camera` を所有しています。リーダーを共有しないため、干渉することなく並行して実行できます。

`CameraLerp` は、カメラの動きに特化したアニメーションで、カメラ位置に関して単純な `Lerp` よりも視覚的に好ましい円弧を生成します。

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
