# Lite Vote

ゲーム中や通話中の「次どうする？」を、その場のみんなで決める投票アプリです。

現在は Topcoat を使った最小画面と開発基盤を提供しています。

## 必要なもの

- [rustup](https://rustup.rs/)
- 初回ビルド時に Rust crate、Tailwind CLI、Geist フォントを取得できるネットワーク接続

Rust 1.95.0、rustfmt、clippy は `rust-toolchain.toml` に固定されています。リポジトリを取得後、次のコマンドで導入できます。

```sh
rustup show
```

日常の開発では、バージョンを固定した Topcoat CLI を使用します。

```sh
cargo install topcoat-cli --version 0.4.0 --locked
```

## ローカル起動

リポジトリのルートで開発サーバーを起動します。

```sh
topcoat dev
```

ブラウザで <http://127.0.0.1:3000> を開いてください。停止するには `Ctrl+C` を押します。

Topcoat の Tailwind 連携は、ビルド時にスタンドアロンの Tailwind CLI を実行してCSSを生成し、Topcoat のassetとして配信します。Node.jsは開発時にも本番実行時にも必要ありません。CSS生成やasset処理に失敗した場合、ビルドまたは `topcoat dev` は失敗します。

## 検証

整形、静的解析、テストを順に実行します。

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

起動後の最小smoke testは別のターミナルから実行できます。

```sh
curl --fail http://127.0.0.1:3000/
```

レスポンスがHTTP 200で、`Lite Vote`と`準備中`を含むことを確認してください。

## Topcoat UI

このリポジトリでは `topcoat ui init` でテーマを初期化し、現在はbuttonだけを取り込んでいます。`components.toml`、`styles.css`、`src/components.rs`、`src/components/button.rs` は生成後もリポジトリで管理します。
