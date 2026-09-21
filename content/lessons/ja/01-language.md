# 言語の基礎

このレッスンは参考になります。重要なセクションを参照してください。ただし、何かを構築する前にここですべてを暗記する必要はありません。まだ見たことのない機能が見つかったら、また戻ってきてください。


## バインディング

次の 3 つのユーザー バインディング フォームがあります。

- `let` — 不変の名前。再割り当てはできません
- `var` — 変更可能なローカル値
- `mesh` — 表示されるシーンの状態。リーダー/フォロワーのペアを作成します (アニメーションのレッスンで詳しく説明します)

```mcl
let radius = 0.45
var count = 0
mesh dot = fill{CYAN} Circle(radius)
```

## ダイナミックタイピング

Monocurl は動的に型付けされます。変数は任意の値を保持でき、別の型に再割り当てできます。

```mcl transcript
var x = 7
print type_of(x)
x = "hello"
print x
x = [1, 2, 3]
print [type_of(x), len(x)]
```

オーサリング中に `print` を使用して値を検査します。印刷出力は、エディターおよびコンソール (タイムラインの代わり) にインラインで表示されます。

## 文字列とエスケープ

モノカール文字列は、エスケープ マーカーとして `\` ではなく `%` を使用します。これは、LaTeX を書きやすくするためです。バックスラッシュは通常の文字列文字です。

```mcl 
let newline = "first%nsecond"
let quoted = "say %"hello%""
let percent = "100%%"
```
## ディープコピー

割り当てでは常に **ディープ コピー**が実行されます。

```mcl transcript
var a = [[1, 2], [3, 4]]
var b = a
b[0][0] = 99
print [a, b]
```

## 方向リテラル

Monocurl には、位置決めによく使用される一般的な方向のコンパクトなベクトル リテラルがあります。

```mcl transcript
let p = 1.5r + 0.8u       # [1.5, 0.8, 0]
let q = 2l + 1d           # [-2, -1, 0]
let behind = 3b           # [0, 0, 3]
print p
print q
print behind
```

- `l` / `r` — 左/右 (負の x または正の x)
- `u` / `d` — 上/下 (正/負の y)
- `b` / `f` — 後方 / 前方 (正 / 負の z)

方向リテラルは、単なる 3 要素のリストです。これらを `shift{}` または `center{}` に渡して、ベクトルが必要な場所で使用できます。

## リストとマップ

リストではゼロベースのインデックスが使用されます。マップは `key -> value` ペアを使用します。

>重要 追加は、`..` および `.=` 演算子を通じてサポートされる一般的な操作です。

```mcl transcript
var pts = []
pts .= 1l
pts .= 1u
pts = pts .. 1r      # .. appends; .= is shorthand for x = x .. y

let labels = ["origin" -> ORIGIN, "right" -> 1r, "up" -> 1u]
print [pts, labels["origin"]]
```

## ブロック構文

`block {}` は、リストを蓄積する複数行の式です。 `.` で始まる行は、暗黙的な戻り値に追加されます。ブロック全体がそのリストに対して評価されます。

```mcl
let row = block {
    for (i in range(0, 4)) {
        . center{[i - 1.5, 0, 0]} fill{CYAN} Square(0.4)
    }
    . center{2r} color{ORANGE} Text("end", 0.55)
}
```

## ラムダとラベル付き引数

関数はラムダであり、ファーストクラスとして扱われます。引数にはデフォルトを指定でき、中括弧付き本体では `return` を使用できます。

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

>重要 **ラベル付き引数** は、Monocurl のアニメーション システムを強化する機能です。名前付き引数を使用して関数を呼び出すと、結果はそれらのラベルを記憶し、後でフィールドごとに変更できます。その後、呼び出し全体が新しい引数を使用して再評価されます。

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

これは通常の意味でのオブジェクトの突然変異ではありません。 `ball.pos = 1.4r` は、ライブ `Ball(...)` 呼び出し内に保存されているラベル付き引数を編集し、新しい引数を使用して呼び出し全体が再実行されます。これは、`Lerp` が 2 つの状態間をスムーズに補間する方法を知る方法です。各引数を独立して補間し、各フレームでメッシュを再構築します。

## オペレーター

>重要 演算子は、ターゲットの周囲ではなく、ターゲットの前に書き込む関数です。これらは **パイプライン** を形成します。各オペレーターはメッシュを受け取り、その右側にメッシュを変換します。これらは属性システムとうまく相互作用します。

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

右から左に読む: `Circle(0.5)` はジオメトリを作成します。 `stroke`、`fill`、`center` は順番に変換します。既存の演算子に関して `operator` を使用して独自の演算子を定義できます。

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

組み込み演算子は、スタイル (`fill`、`stroke`、`color`、`alpha`)、位置決め (`center`、`shift`、`rotate`、`scale`)、レイアウト (`next_to`、`to_side`)、およびアイデンティティ (`tag`) を処理します。

オペレーターはアニメーション セマンティクスも保持しますが、これについては後で説明します。

## 制御フロー

標準制御フロー: `if`/`else if`/`else`、`for`、`while`、`break`、`continue`。メッシュに変換する前に、これらを使用してデータを構築します。

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

再帰ラムダは、それ自体を明示的な `self` 引数として受け取ります。これはラムダ計算では標準ですが、これまで見たことがない場合は奇妙に見えるかもしれません。

```mcl transcript
let fib = |self, n| {
    if (n <= 1) { return n }
    return self(self, n - 1) + self(self, n - 2)
}
print fib(fib, 8)
```

## まとめる

一般的なヘルパー関数は、ラベル付きメッシュを構築し、スタイル設定に演算子を使用します。ラベル付き引数はアニメーション キーフレーム フィールドになります。

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
