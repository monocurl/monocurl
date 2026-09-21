# メッシュとオペレーター

メッシュとは、シーン内に表示される任意の値、つまり円、テキスト行、数式、またはそれらのリストです。ほとんどの図は、プリミティブな形状を構築し、演算子を適用してスタイルと位置を設定することによって構築されます。

>重要 メッシュのリスト (またはメッシュのネストされたリストのリスト) はメッシュ ツリーと呼ばれます。ほとんどの関数は、各構成メッシュ上で動作することにより、メッシュ ツリー上で動作します。

## コンストラクター

コンストラクターは原点または原点付近にジオメトリを作成します。オペレーターは、ジオメトリをどこに配置するか、どのように見えるかを決定します。分離により、コンストラクターはシンプルかつ構成可能に保たれます。


```mcl image

mesh demo = [
    center{1.6l} fill{soft{} BLUE} stroke{BLUE, 2} Capsule(0.5l, 0.5r, 0.22),
    center{ORIGIN} color{ORANGE} Vector(0.9r + 0.25u, 0.45l + 0.12d),
    center{1.6r} fill{soft{} GREEN} stroke{GREEN, 2} RegularPolygon(5, 0.44),
    center{1.6l + 0.75d} color{BLUE} Text("Capsule", 0.55),
    center{ORIGIN + 0.75d} color{ORANGE} Text("Vector", 0.55),
    center{1.6r + 0.75d} color{GREEN} Text("RegularPolygon", 0.55)
]
```

参照用の多くのコンストラクターのリスト。詳細とオプションについてはドキュメントを参照してください。

- `Circle(radius)` — XY 平面内の黒丸
- `Annulus(inner, outer)` — 穴のある埋められたリング
- `Square(width)` — 中央揃えの正方形
- `Rect([width, height])` — 軸に揃えられた長方形
- `Arrow(start, end)` — 矢印付きの有向矢印
- `Vector(delta, tail)` — デルタによる尾部からの矢印のようなベクトル
- `Line(start, end)` — 単純な線分
- `Polyline(vertices)` — 点のリストを通るパスを開く
- `Capsule(start, end, radius)` — 丸いカプセル形状
- `RegularPolygon(n, circumradius)` — n 角形の多角形
- `Arc(radius, [start_angle, end_angle])` — 円弧
- `Triangle(p, q, r)` — 三点三角形
- `LineGrid(x_bounds, y_bounds)` — 長方形の線グリッド
- `ColorGrid(color_at, x_bounds, y_bounds)` — サンプリングされた色付きグリッド
- `Axis1d(...)`、`Axis2d(...)`、`Axis3d(...)` — 座標軸
- `ExplicitFunc(func, [x_min, x_max, samples])` — 関数からの曲線
- `Field(glyph_func, x_bounds, y_bounds)` — 各グリッド点でグリフを繰り返す
- `Label(target, string, direction)` — メッシュの横に配置された TeX ラベル
- `Text(string, size)` — レンダリングされたテキスト
- `Tex(string_or_list, size)` — レンダリングされた LaTeX 式
- `Dot(point)` — 単一の画面に面した点

## スタイリング演算子

オペレータはメッシュを右側に変形します。スタイリング演算子は視覚的なプロパティを設定します。

```mcl image

mesh demo = [
    center{1.4l} fill{soft{} BLUE} stroke{BLUE, 3} Annulus(0.22, 0.46),
    center{0r} fill{alpha{0.4} soft{} ORANGE} stroke{ORANGE, 3} Capsule(0.42l, 0.42r, 0.24),
    center{1.4r} color{GREEN} ExplicitFunc(|x| sin(x * TAU), [-0.5, 0.5, 32])
]
```

- `fill{color}` — 閉じたメッシュの内部を塗りつぶします
- `stroke{color, width}` — 色付きのストロークでメッシュの輪郭を描きます
- `color{color}` — ストロークと塗りつぶしを設定します
- `alpha{opacity}` — メッシュの透明度をスケールします。その右にあるものすべてに適用されます
- `scale{factor}` — 均一なスケール。または `scale{[sx, sy, sz]}` 軸ごと

## 位置決めオペレータ

位置決めオペレータはメッシュを空間に配置します。

```mcl image

mesh demo = [
    center{1.5l + 0.5u} fill{soft{} BLUE} stroke{BLUE, 2} Triangle(0.45l + 0.35d, 0.45r + 0.35d, 0.45u),
    center{0r} rotate{PI / 4} fill{soft{} ORANGE} stroke{ORANGE, 2} Square(0.6),
    shift{1.5r} scale{1.2} fill{soft{} GREEN} stroke{GREEN, 2} RegularPolygon(3, 0.42)
]
```

- `center{position}` — 境界ボックスの中心が `position` になるようにメッシュを移動します。
- `shift{delta}` — ベクトルオフセットによってメッシュを変換します
- `rotate{angle}` — Z 軸を中心に回転します (または全軸ベクトルを渡します)
- `in_space{origin, x_basis, y_basis}` — メッシュをローカル座標フレームに配置します

## レイアウト演算子

レイアウト オペレータは、あるメッシュを別のメッシュに対して相対的に配置するため、座標をハードコーディングする必要はありません。

```mcl image

let box = center{0.6l} fill{soft{} BLUE} stroke{BLUE, 2} Rect([1.2, 0.65])

mesh demo = [
    box,
    next_to{box, 1r, 0.25} stroke{ORANGE, 2} Capsule(0.45l, 0.45r, 0.24),
    to_side{1d, 0.2} color{GRAY} Text("caption", 0.55)
]
```

- `next_to{base, direction, spacing}` — `direction` に沿って `base` の隣にメッシュを配置します
- `to_side{direction, spacing}` — 表示されているフレームの端近くにメッシュを配置します

## カスタム演算子

複数のメッシュがビジュアル レシピを共有する場合、それをオペレーターに抽出します。

```mcl image

let soft_style = operator |target, col|
    fill{alpha{0.2} col}
    stroke{col, 2}
    target

mesh demo = [
    center{1.3l} soft_style{BLUE} Annulus(0.2, 0.45),
    center{0r} soft_style{ORANGE} Capsule(0.45l, 0.45r, 0.25),
    center{1.3r} soft_style{GREEN} RegularPolygon(5, 0.48)
]

slide "custom"
    play Grow(1)
```

## メッシュツリー
メッシュ ツリーのリスト自体がメッシュ ツリーであることを思い出してください。ほとんどの図はこのようにして組み立てられます。

```mcl image

let Row = |items| block {
    for (i in range(0, len(items))) {
        let x = (i - (len(items) - 1) / 2) * 1.0
        let kind = mod(i, 3)
        var shape = []
        if (kind == 0) {
            shape = Capsule(0.3l, 0.3r, 0.18)
        } else if (kind == 1) {
            shape = RegularPolygon(5, 0.34)
        } else {
            shape = Annulus(0.16, 0.34)
        }
        . center{[x, 0, 0]} fill{soft{} items[i]} stroke{items[i], 2} shape
    }
}

mesh demo = Row([RED, ORANGE, YELLOW, GREEN, CYAN, BLUE])
```

`block {}` と `.=` を使用してループ内にメッシュを構築するのは、データ駆動型ダイアグラムの標準パターンです。

## タグとフィルター

>重要 タグは、メッシュに番号のリスト (デフォルトでは `[]`) を割り当てることで、メッシュに ID を付加します。フィルターと演算子は、タグ付けされたサブセットをスタイル設定したり、分離したりできます。

```mcl image

let muted = |tags| not (2 in tags)

mesh demo =
    fill{soft{} LIGHT_GRAY, muted} # only applies to edge meshes!
    stroke{LIGHT_GRAY, 2, muted}
    [
        tag{1} center{1.2l} fill{soft{} BLUE} Circle(0.45),
        tag{2} center{0r} fill{soft{} ORANGE} Circle(0.45),
        tag{3} center{1.2r} fill{soft{} GREEN} Circle(0.45)
    ]

# selects a subset of the input
let edges = tag_filter{muted} demo
```

フィルター `|tags| not (2 in tags)` は、タグ セットに `2` が含まれていないフラグメントと一致します。 `fill` および `stroke` 演算子は、このフィルターを使用してミュートされたフラグメントのみをスタイル設定し、オレンジ色の円をフルカラーのままにします。

タグはアニメーションにおいて、変換中もピースのアイデンティティを維持する必要がある場合に重要になります。

## テキストと TeX

`Text` と `Tex` は、他の幾何学的コンストラクターと同様にメッシュを生成します。同じオペレータとアニメーションを使用して、スタイル設定、配置、タグ付け、およびアニメーション化を行うことができます。

```mcl image
let palette = operator |target|
    color{BLUE, |tag| 1 in tag}
    color{ORANGE, |tag| 2 in tag}
    color{GREEN, |tag| 3 in tag}
    target

mesh formula = center{0.2u} palette{} Tex([
    text_tag{1} "a^2",
    " + ",
    "\text_tag{2}{b^2}",
    " = ",
    "\tag3{c^2}" # all three are valid ways to tagging tect
], 1.0)

mesh caption = center{0.8d} color{GRAY} Text("Pythagorean theorem", 0.55)

slide "formula"
    play [Write(1, [&formula]), Fade(0.8, [&caption])]
```

`text_tag{}` は、`tag{}` に相当するテキストです。それはフォーミュラの個々の部分に安定したアイデンティティを与えます。これは内部的に文字列 `\text_tag{tags}{text}` に展開され、`\tag1{}`、`\tag2{}` などのエイリアスが付けられます。

## 実用例

### 関数プロット

```mcl image
let unit = 2
let f = |x| 0.65 * sin(x * PI)
let g = |x| 0.35 * cos(2 * x)
let graph_space = operator |target|
    in_space{0l, unit * 1r, unit * 1u}
    target

let sine = z_index{2} graph_space{} stroke{BLUE, 3} ExplicitFunc(f, [-1.5, 1.5, 120])
let cosine = z_index{2} graph_space{} stroke{ORANGE, 3} dashed{[0.12, 0.08]} ExplicitFunc(g, [-1.5, 1.5, 120])
let area =
    z_index{1}
    graph_space{}
    ExplicitFuncDiff(
        f,
        g,
        [-1.5, 1.5, 120],
        [alpha{0.22} BLUE, alpha{0.22} ORANGE],
        [[1], [2]]
    )

mesh plot = [
    axis_style{"x", -1.5, 1.5, nil, 1, 1}
    axis_style{"y", -1, 1, nil, 0.5, 2}
    Axis2d([unit * 1r, unit * 1u], BLACK, LIGHT_GRAY),
    area,
    sine,
    cosine,
    color{BLUE} center{1.5u + 1r} Tex("0.65 \sin(\pi x)", 0.55),
    color{ORANGE} center{1u + 0.5l} Tex("0.35 \cos(2x)", 0.55)
]
```

### ベクトルフィールド

```mcl image
let palette = [0 -> BLUE, 0.45 -> CYAN, 0.75 -> ORANGE, 1 -> RED]

let Needle = |pos, idx| {
    let vx = sin(pos[1] * PI)
    let vy = -cos(pos[0] * PI)
    let strength = norm([vx, vy, 0])
    let col = keyframe_lerp(palette, strength / 1.42)
    return color{col} Vector(0.28 * [vx, vy, 0], pos)
}

mesh grid = stroke{LIGHT_GRAY, 1} LineGrid([-1.8, 1.8, 9], [-1.1, 1.1, 7])
mesh field = Field(Needle, [-1.8, 1.8, 10], [-1.1, 1.1, 7])
mesh center_dot = fill{BLACK} Dot(ORIGIN)
mesh title = center{1.45u} color{GRAY} Text("sampled vector field", 0.55)
```

### 3D サーフェス

```mcl image
let h = |x, y| 1.2 * (0.5 - (x - 0.5)^2 - (y - 0.5)^2)
let keys = [0 -> BLUE, 0.3 -> CYAN, 0.6 -> ORANGE, 1 -> RED]
let col_at = |pos, idx| keyframe_lerp(keys, h(pos[0], -pos[1]))

# here in space would just be a negation, so we do it manually
mesh surface =
    stroke{BLACK, 1}
    point_map{|p| [p[0], p[1], h(p[0], -p[1])]}
    ColorGrid(col_at, [0, 1, 18], [-1, 0, 18])

mesh wire =
    stroke{GRAY, 1}
    point_map{|p| [p[0], p[1], h(p[0], -p[1]) + 0.01]}
    LineGrid([0, 1, 7], [-1, 0, 7])

mesh axis =
    axis_style{"x", 0, 1, "x"}
    axis_style{"y", 0, 1, "y"}
    axis_style{"z", 0, 1, "z"}
    Axis3d(basis: [1r, 1d, 1b], color: BLACK, grid_color: LIGHT_GRAY, [1u, 1u, 1b])

mesh peak = color{RED} shift{pos:[0.5, -0.5, h(0.5, 0.5)]} Sphere(0.05)
    
camera = Camera([2.2, -2.1, 1.45], [0.5, -0.5, 0.35], [0, 0, 1])
```
