# 高度なトピック

このレッスンでは、日常的なシーンのオーサリングには必要ありませんが、Monocurl の動作をカスタマイズしたり、内部で何が起こっているのかを理解したい場合に役立つトピックについて説明します。

## カスタムライブラリ

共通定義をライブラリファイルに抽出できます。ライブラリ ファイルはシーン ファイルとほぼ同じ構造ですが、ライブラリ部分のみが他のファイルにインポートされます。ライブラリ ファイルの主な目的は、複数のシーンで使用できるヘルパー関数、カスタム オペレーター、定数、および再利用可能なメッシュ定義を提供することです。

`import` キーワードを使用して、ライブラリ ファイルをシーン ファイルにインポートできます。インポート パスはインポート ファイルに対する相対パスであり、`.mcs` 拡張子を含めることはできません。

ファイルがインポートされるとき、Monocurl はそのファイルの最初の `slide` キーワードの前のコードのみをインポートします。最初の `slide` 以降のすべてのスライドは、最初のスライドの後に表示されるインポートも含め、インポートの目的では無視されます。

これにより、ヘルパー ファイルはライブラリ定義の下に小さなデモ シーンを保持できるようになり、ライブラリ ファイルを単独で編集する場合に便利です。

```mcl transcript
let Double = |x| 2 * x

slide "demo"
    print Double(4)
```

これをインポートする別のファイルでは `Double` を使用できますが、デモ スライドはインポート シーンにコンパイルされません。

## ステートフルな値とパラメータ

ステートフルな値は、シーンの状態に依存し、継続的に更新される値です。ステートフルな値が使用される主な場所は、カメラ対応のオーバーレイです。 `camera_transfer{camera, $camera}` は、カメラが移動している間、フレームに対してメッシュを固定したままにし (完全な説明についてはドキュメントを参照してください)、`orient_to_camera{$camera}` は、平面メッシュ ツリーをカメラに向かって継続的に回転させます。ただし、それとは別に、ステートフルは避けるべきであり、明示的に更新する属性をメッシュに持たせる方がより慣用的です。

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

状態のソースは `param` で、これは `mesh` と同様に機能します。これには、コードを編集するリーダー値と、アニメーションが再生されるフォロワー値があります。トップレベルのパラメータは、プレゼンテーション モードの対話型コントロールとしても公開されます。

最も一般的なパラメータは `camera` と `background` です。これらは実際にビジュアル シーンに影響を与えるため、少し特殊です。他のパラメータは通常、メッシュの制御に使用されます。

通常、`radius` などのパラメータを読み取ると、その現在のリーダー値が読み取られます。 `$radius` などの `$` を使用してそれを読み取ると、ライブ値へのステートフル参照が作成されます。値が変化するにつれて式が継続的に再評価されるかのように考えることができます (ただし、実際にはその方が効率的です)。

メッシュにはステートフルな値のみを割り当てることができます。それらに対して実行できる操作は限られています。これらは、関数の引数、演算子の引数、およびリスト内で使用できます。ほとんどの場合、属性にもアクセスできます。ステートフルな値をメッシュに割り当て、アニメーションを通じてメッシュを同期する場合、その同期は「純粋な」同期ではありません。代わりに、リーダー メッシュはリーダー パラメーターを使用して評価し、フォロワー メッシュはフォロワー パラメーターを使用して評価します。これは、パラメータをアニメーション化することで、画面上の依存メッシュを変更することを意味します。

以下は、属性を使用して簡単に実行できるため、ややおもちゃの例ですが、パラメーター/ステートフルの使用方法を示しています。より一般的な使用例は、多くの変数が 1 つのシーンの状態に自然に依存する場合、またはプレゼンテーション モードでパラメータ値を変更する場合です。
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

最後に、ステートフルは多くの操作 (加算など) を実行できませんが、任意の関数の引数として使用できることに注意してください。内部的には、再評価のたびに、ステートフル引数の現在の値を使用して関数が呼び戻されます。これにより、冗長ではありますが、この制限を回避できます。

```mcl image

param radius = 2

mesh circle = fill{CLEAR} Circle($radius)
# won't work, can't do * on a stateful value
# mesh square = Square(2 * $radius)
# ... but you can use it as argument to any function
let double = |x| 2 * x
mesh square = fill{CLEAR} Square(double($radius))
```

## 高度な LaTeX

デフォルトでは、Monocurl はバンドルされた LaTeX バックエンドを使用します。パッケージまたはフォント用にシステム LaTeX インストールが必要な場合は、デスクトップ設定をカスタム システム `latex` と `dvisvgm` バックエンドに切り替えることができます。 CLI には、対応する `--system-latex` フラグがあります。これにより、デフォルトのバンドルでは提供されない Latex の機能を使用できるようになります。

他の多くの言語とは異なり、Monocurl は文字列のエスケープ文字として `\` ではなく `%` を使用するため、LaTeX を記述するときにバックスラッシュを二重にエスケープする必要がないことに注意してください。

`Text` はリテラル テキストです。 `Tex` は通常の数学フラグメント用です。 `Latex` は、より完全な LaTeX 本体フラグメント用であり、パッケージまたはフォント宣言の `additional_preamble` 引数を受け入れます。

`Tex` と `Latex` はメッシュ ジオメトリを返すため、他のメッシュと同様にスタイル設定、タグ付け、フィルター処理、アニメーション化を行うことができます。レンダリングされた式の一部のみに安定した ID が必要な場合は、テキスト入力内で `text_tag{...}` を使用します。

```mcl
mesh eq = Tex([text_tag{1} "x", " + ", text_tag{2} "1"], 0.8)

slide "Equation"
    play Write(0.8, [&eq])

    eq = Tex([text_tag{2} "1", " + ", text_tag{1} "x"], 0.8)
    play TagTrans(1.0, [&eq])
```


`Tex(...)` 呼び出しがレンダリングに失敗した場合、トランスクリプトには LaTeX コンパイラの出力が表示されます。最も一般的な原因は、パッケージが見つからないことと、文字列引数内の無効な LaTeX 構文です。

## カスタム演算子とその内部での仕組み

演算子はターゲットを受け取り、変換されたターゲットを返す関数であることを思い出してください。これらは、操作対象となるものの前に記述されるため、再利用可能なスタイルおよび配置パイプラインに適しています。

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

演算子の重要な特性は、多くの演算子について `x` と `op{} x` の間で lerp できることです。たとえば、次は有効です
```mcl
mesh org = Triangle(0l, 1u, 1r)
play Set()
org = rotate{180dg} org
play Lerp()
```

ほとんどの場合、stdlib 演算子に関して独自の演算子を定義できますが、独自の補間動作を実行するカスタム 演算子を構築することもできます。プリミティブ演算子は、「identity」値と「operated」値を返します。 ID 値は、未変更のオペランドのように「見える」必要がありますが、「演算された」値を直接補間できる追加の属性が含まれています。

たとえば、`rotate` は次のように実装されます。
```mcl
let rotate = operator |target, radians, axis = 1b, pivot = nil, filter = nil| {
    let go = |angle| __monocurl__native__ op_rotate(target, angle, axis, pivot, filter)
    return [go(angle: 0), go(angle: radians)]
}
```
実際の回転は効率化のためにネイティブの Rust 関数によって行われますが、重要な点は 2 つの値を返すということです。最初のものは 0 ずつ回転します。これがアイデンティティ状態です。 2 番目のボタンは、必要な量だけ回転します。ほとんどの計算では、演算子は 2 番目の戻り値として扱われます。しかし、`x` と `rotate{180dg} x` の間で lerp を実行する場合、Monocurl はこれが演算子 lerp であるべきであることを認識し、アイデンティティ値を調べて、`go(angle: 0)` と `go(angle: 180dg)` の間で「実際に」lerp を実行します。これは従来の補間によって実行できます。

## プリミティブアニメーション

アニメーション モデルは、リーダーとフォロワーの同期に基づいて構築されています。コードはリーダーを即座に編集します。 `play` はフォロワーに追いつく方法を教えます。

最下位レベルのパブリック ラッパーは `PrimitiveAnim(time, &vars, embed, lerp, rate)` です。 `Lerp` や `Trans` などの高レベルのアニメーションは、最終的にはプリミティブ アニメーションに縮小されます。

`PrimitiveAnim` は、フォロワーをリーダーと同期させる方法を指定します。考え方としては、lerp とは異なるカスタム補間関数を提供できるということです。たとえば、stdlib で CameraLerp がどのように定義されているかを次に示します。

```mcl
let CameraLerp = |&camera, time = 1, rate = smooth| {
    let embed = |start, dst| __monocurl__native__ camera_lerp_embed(start, dst)
    let value_lerp = |start, end, state, t| __monocurl__native__ camera_lerp_value(start, end, t)
    return PrimitiveAnim(time, &camera, embed, value_lerp, rate)
}
```

重労働のほとんどは Rust で行われますが、それでも一般的な流れを理解することができます。埋め込み関数は開始と終了を前処理し、`[mod_start, mod_end, embed_state]` を返します。これは、高価なマッチング アルゴリズムを実行する必要がある Trans のようなアニメーションに便利なので、最初に 1 回だけ実行することをお勧めします。補間関数は、embed の引数と正規化された t 値を受け取り、目的の動作に従って補間するように求められます。 `CameraLerp` の場合、これは、表示をより自然にするために球面補間を実行することになります。
