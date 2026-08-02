# Lite Vote

ゲーム中や通話中の「次どうする？」を、その場のみんなで決める投票アプリです。

現在は Topcoat を使った最小画面と開発基盤を提供しています。

## 必要なもの

- [rustup](https://rustup.rs/)
- 初回ビルド時に Rust crate、Tailwind CLI、Geist フォントを取得できるネットワーク接続

コンテナでの起動にはDockerとDocker Compose、E2EテストにはNode.jsとnpmが
それぞれ必要です。Node.jsはE2Eテスト専用であり、アプリケーションのビルドや
本番コンテナの実行には使用しません。

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

ログは標準出力へ出力されます。`LITE_VOTE_ENV=local`（既定）では読みやすい
テキスト形式、`LITE_VOTE_ENV=production`ではJSON形式になります。ログレベルは
`RUST_LOG`で変更できます。稼働確認には`GET /healthz`、SQLite接続とmigrationの
適用確認には`GET /readyz`を使用できます。

新しいマイグレーションは次のコマンドで作成します。

```sh
just db-add <name>
```

`just db-add <name>`は`sqlx migrate add <name>`を実行します。migration名は必須です。

Topcoat の Tailwind 連携は、ビルド時にスタンドアロンの Tailwind CLI を実行してCSSを生成し、Topcoat のassetとして配信します。Node.jsはアプリケーションの開発サーバーにも本番実行時にも必要ありません（E2Eテストでのみ使用します）。CSS生成やasset処理に失敗した場合、ビルドまたは `topcoat dev` は失敗します。

## コンテナでのローカル起動

Linux向けOCI互換の本番イメージと、migration用の一時イメージをビルドして起動します。

```sh
just container-up
```

ブラウザで <http://127.0.0.1:3000> を開いてください。`LITE_VOTE_PORT=8080`
のように指定すると公開ポートを変更できます。終了時は`Ctrl+C`を押し、別の
ターミナルで次を実行します。

```sh
just container-down
```

Composeの`lite-vote-data` named volumeをアプリとmigrationサービスで共有し、
SQLite DBを`/data/lite-vote.sqlite3`へ保存します。`container-down`やコンテナの
再作成ではvolumeを削除しないため、再起動後も参加用URLと投票結果が残ります。
データを残したまま動作確認するには、投票後に`docker compose restart app`を実行し、
同じ参加用URLを再度開いてください。volumeを削除する`docker compose down --volumes`
はデータの破棄を意図した場合に限って使用してください。

本番イメージだけをビルドする場合は次を実行します。

```sh
just container-build
```

最終イメージはdistrolessの実行環境、コンパイル済みRustバイナリ、静的assetで構成され、
Node.js、npm、Rustツールチェーン、migration CLIを含みません。実際のデプロイでも、
永続volumeを`/data`へ割り当て、アプリを単一インスタンスで実行してください。
SSEを中継するリバースプロキシを置く場合はレスポンスバッファリングを無効化し、
15秒のheartbeatより十分長いアイドルタイムアウトを設定します。

SSEのローカル確認は、通常ウィンドウとプライベートウィンドウなどCookieを共有しない
二つのブラウザコンテキストで同じ参加用URLを開いて行います。一方から投票し、もう
一方の「リアルタイム更新」表示と結果がページ全体の再読み込みなしで更新されることを
確認してください。片方を一時的にオフラインにして復帰させると、再接続後の同期も
確認できます。

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

### Playwright E2E

最初にテスト専用依存とChromium、Firefox、WebKitを導入します。

```sh
just e2e-install
```

通常の確認ではChromiumでsmoke testを実行します。PlaywrightがmigrationとRust
アプリケーションを自動起動し、終了後に停止します。

```sh
just e2e
```

節目では3ブラウザすべてで同じテストを実行します。

```sh
just e2e-all
```

既に起動しているサーバーを対象にする場合は、`PLAYWRIGHT_BASE_URL`を指定すると
Playwrightによるサーバー起動を省略できます。

```sh
PLAYWRIGHT_BASE_URL=http://127.0.0.1:3000 just e2e
```

現在のPlaywrightテストは環境の起動を検証するsmoke scaffoldのみです。公開部屋、
匿名部屋、複数ブラウザコンテキストを使うSSEの主要シナリオは後続Issueで追加します。

## Topcoat UI

このリポジトリでは `topcoat ui init` でテーマを初期化し、現在はbuttonだけを取り込んでいます。`components.toml`、`styles.css`、`src/components.rs`、`src/components/button.rs` は生成後もリポジトリで管理します。
