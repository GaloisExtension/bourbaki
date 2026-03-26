# Phase 5 実装タスク

## バックエンド (Rust)

### db.rs 追加クエリ
- [x] `pages_fts_ranked_ids` - pagesテーブルへのFTS5検索
- [x] `list_page_rows_for_rag` - RAG用ページ一覧（embedding付き）
- [x] `get_pages_by_nums` - ページ番号指定でLaTeX取得
- [x] `concepts_by_page_nums` - ページの概念ノード取得
- [x] `concept_deps_expand` - GraphRAGグラフ探索（依存関係辿り）
- [x] `get_concepts_by_ids` - 概念IDから概念取得
- [x] `compress_messages` - コンテキスト圧縮DB更新
- [x] `list_message_pairs_with_ids` - ID付きメッセージ一覧
- [x] `estimate_message_tokens` - トークン数推定

### openai.rs 追加関数
- [x] `chat_teacher_reply_with_context` - RAG/Memory コンテキスト対応チューター
- [x] `adversarial_check` - 回答草案の数学的厳密さチェック
- [x] `compress_old_messages` - コンテキスト圧縮

### lib.rs 更新
- [x] `send_session_message` 全面更新 - エージェントオーケストレーション + `agent-status` イベント
  - 選択あり/なし自動判定
  - RAG Agent (自由質問時) + GraphRAG展開
  - Memory Agent (過去解説3件)
  - Context Agent (Memory + RAG → システムプロンプト注入)
  - Adversarial Agent (回答草案チェック、thinking ON 時のみ)
  - Context Compression (履歴6000トークン超で自動圧縮)
- [x] `run_rag_hybrid` - FTS5+ベクトルRRF (pages用)
- [x] `emit_agent_status` ヘルパー
- [x] `prefetch_pages` 新コマンド - 選択時に周辺ページを先読み
- [x] invoke_handler に prefetch_pages 登録

## フロントエンド (React)

### AgentPanel コンポーネント
- [x] `src/components/AgentPanel/AgentPanel.tsx` 作成
  - 送信中のノードにリアルタイムエージェントステータス表示
  - 完了後はサマリー表示に切り替え
  - クリックで詳細展開

### Store 更新
- [x] `agentStatuses: AgentStatus[]` 追加
- [x] `compressedSessions: string[]` 追加
- [x] `updateAgentStatus`, `clearAgentStatuses`, `addCompressedSession` アクション追加

### commands.ts 追加
- [x] `prefetchPages(bookId, centerPage)` ラッパー

### App.tsx 更新
- [x] `agent-status` イベントリスナー → ストア更新
- [x] `compression-done` イベントリスナー
- [x] 選択時に `prefetchPages` 呼び出し

### ChatNode.tsx 更新
- [x] AgentPanel をメッセージ一覧の下に表示

### FlowCanvas.tsx 更新
- [x] 送信開始時に `clearAgentStatuses` 呼び出し
- [x] 送信中のセッションにのみ `agentStatuses` を渡す

## テスト
- [ ] text_linear_algebra.pdf で自由質問 (RAG) テスト
- [ ] Adversarial Agent の動作確認
- [ ] Context Compression の動作確認
- [ ] Agent Transparency Panel の表示確認
