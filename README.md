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

日常の開発では、バージョンを固定した `just`、Topcoat CLI、sqlx-cli を使用します。

```sh
cargo install just --version 1.57.0 --locked
cargo install topcoat-cli --version 0.4.0 --locked
cargo install sqlx-cli --version 0.9.0 --no-default-features --features sqlite --locked
```

利用可能な開発コマンドと説明は、リポジトリのルートで引数なしの
`just`を実行すると確認できます。

```sh
just
```

## ローカル起動

リポジトリのルートで開発サーバーを起動します。

```sh
just dev
```

ブラウザで <http://127.0.0.1:3000> を開いてください。停止するには `Ctrl+C` を押します。
`just dev`は最初に`just db-migrate`を実行し、migrationに成功した場合だけ
`topcoat dev`を起動します。`just db-migrate`はDBの親ディレクトリを作成し、
`sqlx database create --database-url sqlite://<DB path>`と
`sqlx migrate run --database-url sqlite://<DB path>`を順に実行します。

DBは既定で `var/lite-vote.sqlite3` に保存されます。パスは
`LITE_VOTE_DATABASE_PATH`、書き込み競合の待機時間（既定5000ms）は
`LITE_VOTE_DATABASE_BUSY_TIMEOUT_MS` で変更できます。アプリは起動時にWAL、
外部キー、busy timeoutを有効化し、マイグレーション未適用ならHTTPサーバーを
起動せず終了します。適用履歴はSQLx標準の`_sqlx_migrations`テーブルで管理されます。
マイグレーションはアプリ起動時には適用されないため、デプロイ前にも
`just db-migrate`を実行してください。`LITE_VOTE_DATABASE_PATH`を指定すると、
`just db-migrate`と`just dev`のmigrationおよびアプリケーションが同じDBを使用し、
指定先の親ディレクトリも自動で作成されます。

新しいマイグレーションは次のコマンドで作成します。

```sh
just db-add <name>
```

`just db-add <name>`は`sqlx migrate add <name>`を実行します。migration名は必須です。

Topcoat の Tailwind 連携は、ビルド時にスタンドアロンの Tailwind CLI を実行してCSSを生成し、Topcoat のassetとして配信します。Node.jsは開発時にも本番実行時にも必要ありません。CSS生成やasset処理に失敗した場合、ビルドまたは `topcoat dev` は失敗します。

## 検証

整形、静的解析、テストを順に実行します。

```sh
just check
```

`just check`は`cargo fmt --check`、
`cargo clippy --all-targets --all-features -- -D warnings`、
`cargo test --all-features`をこの順序で実行し、途中で失敗した場合は後続の処理を
実行しません。

起動後の最小smoke testは別のターミナルから実行できます。

```sh
curl --fail http://127.0.0.1:3000/
```

レスポンスがHTTP 200で、`Lite Vote`と`準備中`を含むことを確認してください。

## Topcoat UI

このリポジトリでは `topcoat ui init` でテーマを初期化し、現在はbuttonだけを取り込んでいます。`components.toml`、`styles.css`、`src/components.rs`、`src/components/button.rs` は生成後もリポジトリで管理します。
