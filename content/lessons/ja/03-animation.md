# アニメーションの基礎

モノカール アニメーションは状態の変化から構築されます。あなたはリーダーを突然変異させます。 `play` ステートメントは、選択された戦略を持つリーダーにフォロワーを同期させます。

## リーダーとフォロワー

`mesh x = value` は 2 つのものを作成します。

- **リーダー** — コードが読み書きする変数。`value` に初期化されます。
- **フォロワー** — ビューポートが実際に描画するもの。`[]` (空) に初期化されます。

コード内のリーダーの変更は即座に行われ、目に見えません。 `play` ステートメントのみがフォロワーをリーダーに追いつきます。`play` に選択されたアニメーションが、そこに到達する「方法」を制御します。

エディターのビューポートには、任意の時点でのフォロワーの状態が表示されます。タイムラインをスクラブすると、「実行中のこの瞬間、フォロワーはどのように見えましたか?」と尋ねることになります。

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

>重要な特別な例外: **init** の終了時に、すべてのメッシュ リーダーがそのフォロワーに自動的に同期されます。そのため、init で定義されたメッシュは、シーンが最初に開いたときにすでに表示されています。


## セット

`Set` は、すべてのダーティ リーダーを即座にフォロワーにコピーします。ジャンプ カットに使用したり、トランジションを使用せずに新しい状態を表示したりするために使用します。

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

一般に、アニメーションでは、変更されたメッシュを確認することで、アニメーション化するメッシュを推測できます。より複雑なシナリオでは、`play Set([&ball])` 構文を使用して必要な変数を明示的に指定できます。

## ラープ

`Lerp` は、時間の経過とともに互換性のある値を補間します。 「互換性がある」とは、通常、リーダーとフォロワーが同じ構造を持つ同じ関数呼び出しであるため、引数を個別に補間できることを意味します。正確な定義はドキュメントにあります。

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

`ball` は `Ball` へのラベル付き呼び出しとして定義されているため、個々のフィールド (`ball.pos`、`ball.radius`、`ball.col`) を変更すると、`Lerp` が各引数を古い値から新しい値に独立して補間します。

>重要 これは Monocurl の核となるキーフレーム アニメーション パターンです。ラベル付きの引数を使用してコンストラクター関数を設計し、特定のフィールドを変更して各キーフレームの状態を定義します。

`Lerp` およびその他のアニメーションは、イージング用のオプションの `rate` 修飾子を受け入れます。

```mcl
play Lerp(1.5, smooth)     # default: smooth S-curve
play Lerp(1.5, ease_out)   # decelerates
play Lerp(1.5, linear)     # constant speed
```

## イントロとアウトロのアニメーション

一部のアニメーションは、メッシュが表示されたり消えたりすることを目的としています。現在のフォロワーの状態と新しいリーダーの状態を比較し、その違いをアニメーション化します。

**書き込み** — 手で描いたかのように新しい輪郭をトレースします。曲線やテキストに最適です。

```mcl video
slide "Write"
    mesh curve = stroke{BLUE, 3} ExplicitFunc(|x| sin(x * TAU), [-1.5, 1.5, 100])
mesh label = center{1.2d} color{BLUE} Text("sin(2πx)", 0.55)
    play Write(1.2)
    play Wait(0.8)
```

**フェード** — メッシュをフェードインまたはフェードアウトします。
**成長** — ジオメトリを中心から外側に拡張します。形が現れるのに適しています。

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

## トランスとタグトランス

**Trans** は汎用メッシュ変換です。コスト関数を使用して輪郭を一致させることにより、現在のフォロワーを現在のリーダーにモーフィングします。 `Lerp` が不可能な場合 (リーダーとフォロワーの構造が異なるため) に使用できます。

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

**TagTrans** は、同じタグを持つフラグメントへの一致を制限する `Trans` の特殊化です。これにより、輪郭一致プロセスをきめ細かく制御できます。

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

状態間でタグの名前が変更される場合は、`tag_map` をグループ ソース タグ リストに渡します
ターゲットタグリスト付き。マッチングにのみ影響します。メッシュは書き換えられません。

```mcl transcript
# full tag list [1, 2] matches full tag list [3, 4]
let one_to_one = [[1, 2] -> [3, 4]]
print one_to_one[[1, 2]]

# full tag lists [1] and [2] both match full tag list [3]
let many_to_one = [[[1], [2]] -> [[3]]]
print many_to_one
print many_to_one[[[1], [2]]]
```

## 演算子を使用した Lerp

`Lerp` は、ラベル付き関数の引数だけを扱うものではありません。演算子は自身のアイデンティティ状態を知っているため、演算子ベースの式も補間できます。

たとえば、`x` から `rotate{angle} x` に補間すると、メッシュがスムーズに回転します。

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

`shift`、`scale`、`color`、およびその他のほとんどの演算子でも同様です。一般的なパターンは次のとおりです。`mesh = operator{args} mesh` はリーダーを操作されたバージョンに設定し、`Lerp` はフォロワー (操作されていない) からリーダー (操作された) まで補間します。

これをチェーンして一連のオペレーター アプリケーションをアニメーション化することもできます。

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

## アニメーションを選択する

- **セット** — インスタントスナップ。ジャンプカットと最初の公開に使用
- **Lerp** — スムーズな補間。リーダーとフォロワーが同じ構造を持つ場合に使用します
- **書き込み** — トレース。新しいカーブ、パス、テキストに使用します
- **成長** — 拡大。新しい塗りつぶされた形状に使用します
- **フェード** — 不透明度。ジオメトリの表示または非表示に使用します
- **トランス** — 一般的なモーフィング。構造が大きく変化する場合に使用します
- **TagTrans** — タグ付きモーフィング。複数の独立した部分がそれぞれ独自の対応部分を必要とする場合に使用します
