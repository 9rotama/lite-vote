---
name: deliver-issue
description: GitHub Issueを、調査、実装、コミット・Pull Request作成の3段階に分け、独立したagent間で引き継いで完了させる。lite-voteでIssueへの着手、Issueの実装、PR作成、または「explore→implement→commit&pr」の進行を依頼されたときに使用する。
---

# Deliver Issue

対象Issueを読み、次の3段階を別々のagentへ順番に委譲する。各agentには対象Issue、関連文書、直前段階の成果だけを渡す。複数段階を同じagentに兼任させない。

## 1. Explore

Explore agentに以下を依頼する。

- Issue、`docs/requirements.md`、`docs/technical-requirements.md`、関連コードを調査する。
- `grill-with-docs`を使い、不明点、境界条件、失敗時の挙動、技術的選択を詰める。
- 設計判断を適切な文書へ記録する。長期的な判断はADR、用語は`CONTEXT.md`、Issue固有の計画はIssue本文またはコメントへ記録する。
- 実装対象、対象外、受け入れ条件、テスト方針、変更候補ファイルを明示する。
- 実装は行わない。

設計にユーザー判断が必要なら、後続段階へ進まず質問する。

## 2. Implement

Exploreの成果をImplement agentへ渡し、以下を依頼する。

- 設計と受け入れ条件に沿って実装する。
- 必要なテストを追加し、影響範囲に応じた検証を実行する。
- 無関係な変更や、Issueの対象外となる追加機能を含めない。
- 設計の変更が必要になった場合は独断で範囲を広げず、理由と変更案を返す。
- コミット、push、PR作成は行わない。

実装後、差分とテスト結果を確認してから次へ進む。

## 3. Commit & PR

Commit & PR agentへIssue、設計、実装差分、テスト結果を渡し、以下を依頼する。

- 差分をレビューし、Issueの受け入れ条件と一致するか確認する。
- `cargo fmt --check`、警告をエラー扱いした`cargo clippy`、関連テストを実行する。
- 問題があれば必要最小限の修正を行い、再検証する。
- Issue単位でコミットを作成し、ブランチをpushする。
- 変更内容、設計判断、確認結果、未対応事項、`Closes #<issue番号>`を記載したPRを作成する。

ユーザーの明示的な許可なく、既存の変更を破棄、上書き、またはPRをマージしない。

## 完了報告

PR URL、主要な変更、実行した検証、残課題を簡潔に報告する。PRを作成できなかった場合は、完了扱いにせず阻害要因と再開地点を示す。
