# Repository instructions

## UI components

- UIを実装・変更するときは、適用できる箇所でTopcoat UIコンポーネントを使用する。
- まずリポジトリへコピー済みの`src/components`を再利用する。必要なコンポーネントがない場合は、`topcoat ui` CLIで必要なものだけを取り込み、リポジトリ内で保守する。
- Topcoat UIで表現できるUIについて、同等のコンポーネントを独自に作らない。
- Topcoat UIコンポーネントを変更した場合は、機能要件に加えてアクセシビリティ要件も確認する。
