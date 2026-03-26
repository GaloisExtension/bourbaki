# Phase 6 実装タスク: UX 仕上げ

## 確認済み（実装済み）
- [x] KaTeX レンダリング（MathMessage.tsx, ChatNode.tsx）
- [x] キャンバスのズーム・パン・ミニマップ（FlowCanvas.tsx）

## 1. 解決済みセッション一覧パネル
- [ ] `src/components/ResolvedPanel/ResolvedPanel.tsx` 作成
  - 右サイドバーとして折りたたみ可能
  - resolved=1 のセッション一覧を表示
  - 各セッション: ページ番号・選択テキスト・解説サマリー・作成日時
  - クリックでキャンバス上の対象ノードをフォーカス
  - セッションを検索できる（フィルター入力）
  - 解決済み → 未解決に戻すボタン
- [ ] `src/api/commands.ts` に `listResolvedSessions` 追加
- [ ] `src-tauri/src/lib.rs` に `list_resolved_sessions` コマンド追加
- [ ] FlowCanvas と連携（選択ノードへスクロール）

## 2. 教科書（本）管理画面
- [ ] `src/components/BookManager/BookManager.tsx` 作成
  - モーダルダイアログ形式
  - 登録済み書籍一覧（book_id, pdf_path, page_count, 取り込み状況）
  - 書籍を選択して切り替え（setBookId）
  - 書籍を削除（DB から pages/sessions/concepts を削除）
  - 新規書籍追加（= PDF を開く）
- [ ] `src-tauri/src/lib.rs` に必要なコマンド追加
  - `list_books` → books テーブル一覧
  - `delete_book` → 書籍と関連データを削除
- [ ] `src-tauri/src/db.rs` に `list_books`, `delete_book_cascade` 追加
- [ ] App.tsx にモーダルトリガーボタン追加

## 3. UX 細部改善
- [ ] 解決済みノードの視覚的区別（緑枠・✅バッジ）→ chat-node.css 更新
- [ ] 圧縮済みメッセージの「展開」ボタン UI（is_compressed=1 のメッセージ）
- [ ] ヘッダーのトークン使用量バーを実際の値に接続（現在はモック）
