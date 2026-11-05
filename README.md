# CausalMed

医学的因果関係を研究するための、出典を重視したデスクトップワークスペース。

**0.1 は初期実装です。** 同梱データはすべて合成テストデータです。AI による治療標的の発見、実データの因果推論、医学的な予測性能の検証はまだ行っていません。

## 起動

Node.js 22.12 以降、Rust 1.88 以降、Tauri 2 の OS 開発環境が必要です。Python 参照評価器は Python 3.10 以降、追加パッケージ不要です。

```sh
npm ci
npm run tauri dev
```

ブラウザーで UI のみ確認する場合：

```sh
npm run dev
```

`http://127.0.0.1:1420` は閲覧専用プレビューです。証拠の取り込み・仮説の保存・評価実行・パネル分離は Tauri アプリで行います。

macOS の開発用アプリを作成：

```sh
npm run tauri build -- --debug --bundles app
```

出力：`target/debug/bundle/macos/CausalMed.app`。開発ビルドであり、配布用の署名・公証は未実施です。

## 最初の操作

1. 「証拠」で関係を選択し、出典・実験条件・反証を確認します。
2. 「仮説を作成」で介入計画と反証条件を入力し、下書きを保存します。
3. インスペクターのアイコンで右／下にドッキング、または別ウインドウへ分離できます。見出しをドラッグして配置することもできます。
4. 「ベンチマーク」から合成 M0 を実行し、ドナー単位の誤差と評価分割を確認します。
5. 「証拠を取り込む」で `fixtures/evidence.synthetic.json` と同形式の JSON を取り込めます。同梱ファイルそのものは初回起動で登録済みのため、重複として拒否されます。

日本語・English・简体中文は右上から切り替えます。研究データは OS のアプリデータ領域の `org.causalmed.research/research.sqlite3` に保存します。macOS では通常 `~/Library/Application Support/org.causalmed.research/` です。

## 検証

```sh
npm run build
npm test
cargo test -p causalmed-core
cargo check -p causalmed
PYTHONPATH=python python3 -m unittest discover -s python/tests
```

Python 参照評価器：

```sh
PYTHONPATH=python python3 -m causalmed_worker \
  --samples fixtures/benchmark.synthetic.json \
  --held-out-dataset held-out \
  --held-out-perturbation GENE-E --synthetic
```

このフィクスチャの対照群平均は `1.2`、ドナー MAE は `0.6` です。医学的な性能を意味しません。

[実装状況](docs/ROADMAP.md) · [設計・制約](docs/ARCHITECTURE.md) · [画面設計](docs/UI.md)

## H5AD worker（開発接続）

`python3 -m venv .venv` の後、`.venv/bin/pip install -r python/requirements.txt` で環境を用意します。`CAUSALMED_PYTHON` に仮想環境の Python の絶対パスを設定して Tauri を起動してください。未設定時は `python3` を使用します。

データセット画面の H5AD ボタンでローカルファイルを選択します。worker は backed モードで開き、SHA-256、細胞／遺伝子数、重複 ID、対応する obs 列のカテゴリ別上位20件・欠損数を返します。行列は IPC に送りません。直近1ファイルのメタデータを Rust がアプリデータ領域に保存します。原本は移動しません。現在の処理はメタデータ取り込みまでで、発現値の品質管理、列マッピング、評価への接続は今後の段階です。
