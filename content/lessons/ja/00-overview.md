# 概要

このレッスンでは、エディター、シーンの構造、コードの作成から作業のエクスポートまたは表示までの手順を説明します。

## 編集者

![The Monocurl editor](/img/home/monocurl-editor.png)

エディターには 3 つの作業面があります。

- **ソース エディタ** (左) — `.mcs` シーン ファイルを作成する場所
- **ビューポート** (右上) — 現在のタイムライン位置でレンダリングされたフレームを表示します
- **タイムライン** (右下) — スライドと各スライド内の個々の `play` ステップをスクラブします。

コードを編集すると、Monocurl が再評価し、タイムライン内の現在位置に応じてビューポートのプレビューが更新されます。

## シーンの構造

すべてのシーンには、インポート、初期セクション、スライドの 3 つの部分があります。簡潔にするために、ほとんどの例ではインポートを省略しています。

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

最初の `slide` より前のコードは **init** です。ここには、定数、ヘルパー関数、開始時の表示状態と並んで、インポートが存在します。最初の `slide` キーワードの後のコードはそのスライドに属し、`play` アニメーションを含めることができます。

## 最初のシーン

これは完全な小さなシーンです。パターンは次のとおりです。init で特定のヘルパーをセットアップし、各スライドでアニメーションの再生を介してシーンの状態を連続的に変更します。

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

## タイムラインナビゲーション

きめ細かい制御を行うには、クリックしてシークすることができます。ただし、一般に、シーン内を移動するにはキーボードを使用します。

- `,` / `.` — 前/次のスライド
- `<` / `>` — シーンの開始/終了にジャンプします
- `;` / `'` — 少しずつ後退/前進


オーサリング中に役立つ習慣は、新しいスライドを追加した後にタイムラインをスクラブして、`play` の各ステップが意図したとおりに動作することを確認することです。

## プレゼンテーションとエクスポート

同じ `.mcs` ソース ファイルは、次の 3 つの方法で使用できます。

- **ビデオのエクスポート** — [ファイル] メニュー → [ビデオのエクスポート]。シーン全体を `.mp4` としてレンダリングします。
- **画像のエクスポート** — [ファイル] メニュー → [画像のエクスポート]。単一フレームを `.png` としてレンダリングします。
- **プレゼンテーション モード** — `Cmd/Ctrl-P`。スライドをナビゲーション チェックポイントに変え、`slide` の各境界で一時停止します。

## インタラクティブなワークフロー

!vid[](/video/interactive-development.mp4)


プレゼンテーション モードでは、`Cmd/Ctrl-T` によって **パラメータ パネル**が開き、スライダーを使用してシーンの状態の一部を編集できます。これはより高度でニッチですが、強力な可能性があります。

プレビュー モードとプレゼンテーション モードでは、カーソルをドラッグしてカメラを移動できます。 Shift キーを押しながらカーソルをドラッグすると、カメラをパンできます。これらは、3D シーンを構築する場合に特に役立ちます。

## ウェブ上のモノカール

[Monocurl Essays](https://www.monocurl.com/monocurl-essays/) は、Monocurl シーンを Web 上で直接実行する方法を示しています。基盤となるランタイムは、[NPM パッケージ](https://www.npmjs.com/package/monocurl) としても入手できます。
