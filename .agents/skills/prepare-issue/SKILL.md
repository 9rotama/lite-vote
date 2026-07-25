---
name: prepare-issue
description: lite-voteのGitHub Issueを実装可能な状態へ準備する。Issueの要件整理、実装前の設計、受け入れ条件やテスト方針の明確化、grill-with-docsを使ったユーザーとの検討、実装計画のIssueへの記録を依頼されたとき、またはdeliver-issueの事前準備が必要なときに使用する。
---

# Prepare Issue

対象Issueを、別セッションの`deliver-issue`が追加の設計判断なしに実装へ進められる状態にする。ユーザーとの対話と外部状態を変更する操作は親agentが担い、subagentへ委譲しない。

## 1. Context

親agentがIssue、`docs/requirements.md`、`docs/technical-requirements.md`、関連コードとテストを確認する。Issueの要件と既存の仕様・実装に矛盾があれば明示する。

## 2. Grill

`grill-with-docs`を使い、ユーザーと次の事項を確定する。

- 実装対象
- 対象外
- 境界条件と失敗時の挙動
- 技術方針
- 受け入れ条件
- テスト方針
- 変更候補箇所

未解決事項が残っている間はIssueを準備完了としない。

## 3. Record

確定した長期的な設計判断はADR、用語は`CONTEXT.md`へ記録する。Issue固有の実装計画はIssue本文またはコメントへ、次の見出しを使って記録する。

```markdown
## 実装計画

### 実装対象

### 対象外

### 技術方針

### 受け入れ条件

### テスト方針

### 変更候補箇所

### 未解決事項

なし
```

Issueへ記録した内容とADRや`CONTEXT.md`の内容を一致させる。

## 4. Handoff

Issue上の実装計画と未解決事項がないことを確認する。準備したIssueのURLを報告し、新しいセッションでそのIssueに対して`deliver-issue`を実行するようユーザーへ案内して終了する。同じセッションで実装へ進まない。
