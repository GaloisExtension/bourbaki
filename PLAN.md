# Math Teacher App 実装計画

## 概要

数学の教科書PDFを一緒に読んでくれるmacOSデスクトップアプリ。
PDFをLaTeXに変換し、範囲選択→ブランチ型会話でLLMに質問できる。

---

## 技術スタック

| レイヤー | 技術 | 理由 |
|---------|------|------|
| デスクトップ | Tauri v2 (Rust + React) | 軽量・ネイティブ感・SQLite統合が容易 |
| UI フレームワーク | React + TypeScript | キャンバス系ライブラリが豊富 |
| キャンバスUI | React Flow | ブランチ型ノードUIに最適 |
| PDF 表示 | PDF.js | テキスト選択APIが充実 |
| メインLLM (thinking ON) | gpt-5.4 | UIトグル切替、通常プランで使える上限モデル |
| メインLLM (thinking OFF) | gpt-5.4-mini | 高速・軽量回答向け |
| サブエージェントLLM | gpt-5.4-mini | RAG/Memory/Prefetch/外部定義取得など |
| PDF→LaTeX変換 | gpt-5.4 (Vision) | 数式精度最優先、pro不要 |
| ローカルDB | SQLite (FTS5 + sqlite-vec) | sui-memory方式、外部依存なし |
| 埋め込みモデル | Ruri v3-310m (CPU実行) | 日本語特化・ローカル実行可能 |
| 数式レンダリング | KaTeX | 高速・ブラウザ完結、MathJaxより軽量 |

---

## 質問モード

| モード | トリガー | コンテキスト取得方法 |
|--------|---------|-------------------|
| **選択あり質問** | PDF上でテキストを選択してから質問 | 選択LaTeX + Prefetch Agent による周辺先読み |
| **自由質問（RAG）** | 選択なしで直接質問 | RAG Agent が書籍全体LaTeXをベクトル検索して関連箇所を抽出 |

どちらのモードでも Context Agent が過去の解決済み解説を注入する。

---

## アーキテクチャ

```
[macOS App (Tauri)]
│
├── Frontend (React)
│   ├── PDF Viewer (PDF.js)          ← ドラッグで範囲選択（任意）
│   └── Canvas (React Flow)          ← ブランチ型会話ノード
│
├── Backend (Rust / Tauri Commands)
│   ├── PDF→LaTeX 変換パイプライン
│   ├── SQLite 管理 (FTS5 + sqlite-vec)
│   └── Agent Orchestrator
│
└── Agent 群 (OpenAI API)
    ├── Main Agent      ← Q&A回答（コンテキスト汚染なし）
    ├── Prefetch Agent  ← 選択箇所周辺LaTeXの先読み [選択ありのみ]
    ├── RAG Agent       ← 自由質問時に書籍全体LaTeXを検索して関連箇所抽出
    ├── Memory Agent    ← 解決済み会話の記録・検索
    └── Context Agent   ← RAG結果 + 過去解説をMain Agentに注入
```

---

## DBスキーマ

```sql
-- LaTeXページキャッシュ（RAG用埋め込みも保持）
CREATE TABLE pages (
    id INTEGER PRIMARY KEY,
    book_id TEXT NOT NULL,
    page_num INTEGER NOT NULL,
    latex TEXT NOT NULL,
    embedding BLOB,                -- sqlite-vec用ベクトル（RAG検索用）
    created_at INTEGER DEFAULT (unixepoch())
);

-- FTS5 全文検索インデックス（ページ本文）
CREATE VIRTUAL TABLE pages_fts USING fts5(
    latex,
    content='pages',
    tokenize='trigram'
);

-- GraphRAG: 概念ノード（定義・定理・補題・例題など）
CREATE TABLE concepts (
    id INTEGER PRIMARY KEY,
    book_id TEXT NOT NULL,
    page_num INTEGER NOT NULL,
    type TEXT NOT NULL,          -- 'definition' | 'theorem' | 'lemma' | 'example' | 'proof'
    label TEXT,                  -- 例: "定義2.1", "定理3.5"
    name TEXT,                   -- 例: "極限", "連続性"
    latex TEXT NOT NULL,         -- 該当LaTeX
    embedding BLOB               -- sqlite-vec用ベクトル
);

-- GraphRAG: 概念間の依存関係エッジ
CREATE TABLE concept_edges (
    id INTEGER PRIMARY KEY,
    from_id INTEGER NOT NULL,    -- 依存元 (例: 定理3.5)
    to_id INTEGER NOT NULL,      -- 依存先 (例: 定義2.1)
    edge_type TEXT NOT NULL      -- 'uses' | 'proves' | 'extends' | 'example_of'
);

-- 会話セッション（ブランチ構造）
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,           -- UUID
    book_id TEXT NOT NULL,
    page_num INTEGER,
    selection_text TEXT,           -- 選択した原文
    selection_latex TEXT,          -- 対応するLaTeX
    parent_id TEXT,                -- 枝分かれ元のsession_id (NULLならルート)
    resolved INTEGER DEFAULT 0,   -- 0: 未解決, 1: 解決済み
    created_at INTEGER DEFAULT (unixepoch())
);

-- メッセージ
CREATE TABLE messages (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    role TEXT NOT NULL,            -- 'user' | 'assistant'
    content TEXT NOT NULL,
    is_compressed INTEGER DEFAULT 0,  -- 1: 圧縮済み（元テキストはoriginal_contentに保存）
    original_content TEXT,            -- 圧縮前の元テキスト（復元用）
    created_at INTEGER DEFAULT (unixepoch())
);

-- 解決済み解説（Memory Agent が参照）
CREATE TABLE resolved_explanations (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    summary TEXT NOT NULL,         -- 何を理解したかの要約
    embedding BLOB,                -- sqlite-vec用ベクトル
    decay_weight REAL DEFAULT 1.0, -- 時間減衰 (半減期30日)
    created_at INTEGER DEFAULT (unixepoch())
);

-- FTS5 全文検索インデックス
CREATE VIRTUAL TABLE explanations_fts USING fts5(
    summary,
    content='resolved_explanations',
    tokenize='trigram'
);
```

---

## Agent 設計

### Main Agent
- 役割: ユーザーの質問に回答しながら、必要に応じてサブエージェントへ積極的に委譲
- モデル: gpt-5.4-pro（thinking ON）/ gpt-5.4（thinking OFF）※UIトグルで切替
- コンテキスト: 選択LaTeX + Context Agentからの注入情報のみ（会話履歴を汚染しない）
- **アクティブ委譲の仕様**:
  - 回答中に「定義が必要」と判断 → Definition Agent に用語検索を委譲
  - 回答中に「関連定理がある」と判断 → Cross-Reference Agent に検索を委譲
  - 回答中に「過去の解説が参考になる」と判断 → Memory Agent に類似解説を検索委譲
  - 委譲はツール呼び出しとして実装（function calling）、結果を受け取って回答に統合

#### UIトグル仕様
- ヘッダーに「Thinking」スイッチを配置
- ON: gpt-5.4（通常プランの上限モデル）→ 深い推論、証明・定義の厳密な質問向け
- OFF: gpt-5.4-mini → 高速回答、「この記号は？」レベルの軽い質問向け
- 現在のモードをノードのバッジで表示（🧠 / ⚡）

### Prefetch Agent
- トリガー: ユーザーが選択操作をしたとき
- 処理: 選択ページ ± 2ページのLaTeXをキャッシュ確認 → 未変換ならLaTeX化
- 非同期実行（UIをブロックしない）

### Memory Agent
- トリガー: ユーザーが「解決」ボタンを押したとき
- 処理:
  1. 会話全体を要約してsummaryを生成
  2. Ruri v3-310mで埋め込みベクトル化
  3. `resolved_explanations` に保存
- 検索: 次回質問時にFTS5（trigram）+ sqlite-vec（ベクトル）でRRFスコアリング

### Math Input Normalizer（ユーザー入力の正規化）
- トリガー: ユーザーが入力を送信する直前
- モデル: gpt-5.4-mini
- 役割: ユーザーの**省略・非形式的なLaTeX入力**を正式なLaTeXに整形する
- 処理フロー:
  ```
  ユーザー入力（非形式）
       ↓ Math Input Normalizer (gpt-5.4-mini)
  正規化されたLaTeX
       ↓
  ├─ KaTeXでレンダリング → チャットノードに表示
  └─ Main Agentへ送信（正規化済みテキストを渡す）
  ```
- 対応する省略記法の例:

  | ユーザー入力 | 正規化後 |
  |------------|---------|
  | `lim_{n->inf} a_n` | `\lim_{n \to \infty} a_n` |
  | `forall eps>0, exists N` | `\forall \varepsilon > 0,\ \exists N` |
  | `f'(x) = lim (f(x+h)-f(x))/h` | `f'(x) = \lim_{h \to 0} \frac{f(x+h)-f(x)}{h}` |
  | `sum_{k=1}^{n} k^2` | `\sum_{k=1}^{n} k^2` |
  | `A ⊆ B` （Unicode記号） | `A \subseteq B` |

- 正規化時に参照するコンテキスト:
  - 教科書の記法（`\varepsilon` vs `\epsilon`、`\to` vs `\rightarrow` など）
  - Notation Adapter Agentが収集した記法ルールを共有

#### UIでの数式表示仕様
- **ユーザー入力欄**: 入力中にリアルタイムプレビュー（KaTeXでレンダリング）
- **送信後**: 正規化済みLaTeXをKaTeXで表示（生のLaTeXは折りたたみで見られる）
- **LLMの回答**: `$...$` / `$$...$$` / `\[...\]` を自動検出してKaTeXでレンダリング
- **コードブロック扱いのLaTeX**: ` ```latex ` のブロックもレンダリング対象

### External Definition Agent
- トリガー: Main Agentが「教科書内に定義がない」と判断したとき（または明示的に「定義を調べて」）
- 処理:
  1. 用語を以下の優先順位で検索
  2. **Notation Adapter Agent** に結果を渡し、教科書の記法に揃えて出力
- 参照ソースの優先順位:

| 優先度 | ソース | 適した用途 |
|--------|--------|-----------|
| 1 | **Wikipedia (ja)** | 標準的な定義、広範カバー、MediaWiki API無料 |
| 2 | **Wikipedia (en)** | 日本語版にない場合のフォールバック |
| 3 | **ProofWiki** | 形式的な定義と証明、MediaWiki API利用可 |
| 4 | **Wolfram MathWorld** | 応用数学・特殊関数、Webスクレイピング |
| 5 | **nLab** | 圏論・代数トポロジーなど高度な分野 |

### Notation Adapter Agent
- 役割: 外部から取得した定義を**教科書の記法・スタイルに揃える**
- モデル: gpt-5.4-mini
- 処理:
  1. 教科書の周辺LaTeXを参照し、記法パターンを抽出
     - 例: 教科書が $\lim_{n \to \infty}$ ではなく $\lim_{n \rightarrow \infty}$ を使っている
     - 例: ベクトルを $\vec{v}$ ではなく $\mathbf{v}$ で表記している
  2. 外部定義を教科書記法に書き直す
  3. 出典を明記して Main Agent に渡す
- 出力例:
  ```
  [Wikipedia より、教科書の記法に揃えて引用]
  数列 $\{a_n\}$ が $\alpha$ に収束するとは、
  任意の $\varepsilon > 0$ に対して...  ← 教科書がεを使っているので揃える
  ```

### RAG Agent（GraphRAG方式）
- トリガー: **選択なし**でユーザーが質問を送信したとき
- 処理:
  1. 質問テキストをRuri v3-310mで埋め込み
  2. **フラット検索**: `pages` テーブルをFTS5 + sqlite-vecでRRF検索（上位3〜5ページ）
  3. **グラフ探索**: 取得ページに含まれる概念ノードから依存関係グラフを辿り、関連する定義・定理を追加取得
  4. 抽出LaTeXをContext Agentに渡す
- 備考: 選択ありの場合はフラット検索をスキップし、グラフ探索のみ実行

### Adversarial Agent
- トリガー: Main Agentが回答草案を生成した直後（ユーザーに届く前）
- モデル: gpt-5.4-mini
- 処理:
  1. Main Agentの回答草案を受け取る
  2. 以下の観点でチェック:
     - 定義・定理の引用が教科書と一致しているか
     - 記法が教科書に揃っているか
     - 数学的に厳密か（誤魔化しや不正確な言い回しがないか）
  3. 問題があれば指摘をMain Agentに返す → 修正版を生成
  4. OKなら即座にユーザーへ配信（ラウンド上限: 2回）
- 備考: Thinkingトグルがオフの場合はスキップしてもよい（速度優先）

### Context Agent
- トリガー: ユーザーが質問を送信する直前（常に実行）
- 処理:
  1. RAG Agent の結果（または選択LaTeX）を受け取る
  2. Memory Agent に質問テキストで過去解説を検索依頼
  3. [RAG結果 or 選択LaTeX] + [過去解説上位3件] を合成
  4. Main Agentのシステムプロンプトに注入してから回答実行

### Context Compression Agent（Memori方式）
- トリガー: 会話コンテキストが閾値（例: 上限の60%）を超えたとき
- モデル: gpt-5.4-mini
- 処理:
  1. 古いメッセージ群を「重要ポイントの箇条書き」に圧縮
  2. 圧縮前の完全な内容は `messages` テーブルに保存（復元可能）
  3. 圧縮済みサマリーをコンテキストの先頭に挿入して継続
- **UI表示**:
  - トークン使用量バーをヘッダーに表示（例: `[████░░░░] 58%`）
  - 圧縮が走ったら「💾 古い会話を圧縮しました」と通知
  - 圧縮済み箇所は「▶ 展開して詳細を見る」で復元可能

### Agent Transparency Panel
- 常時表示: 各エージェントが「今何をしているか」をリアルタイムで見せる
- 場所: 回答ノードのフッター or 右サイドの折りたたみパネル
- 表示例:
  ```
  [回答生成中]
  🔍 RAG Agent     検索完了 (3件)
  🧠 Memory Agent  過去2件の解説を発見
  ⚠️ Adversarial   記法の不一致を修正中...
  ✅ Context       システムプロンプト注入完了
  ```
- 各行をクリックで、そのエージェントの入出力詳細を展開確認できる
- 完了後は折りたたまれ、「ℹ️ 3つのエージェントが動作しました」のサマリーに切り替わる

---

## 実装フェーズ

### Phase 1: コア基盤
- [ ] Tauri + React プロジェクト初期化
- [ ] PDF.js でPDF表示
- [ ] ドラッグ選択でテキスト取得
- [ ] SQLite初期化・スキーマ作成

### Phase 2: PDF→LaTeX 変換
- [ ] PDFページ → 画像変換（poppler）
- [ ] gpt-4o Vision で1ページずつLaTeX変換
- [ ] バックグラウンドキュー処理（ページ単位）
- [ ] 選択テキスト → 対応LaTeX箇所のマッピング
- [ ] LaTeX変換完了ページを Ruri v3-310m で埋め込み → `pages.embedding` に保存（RAG用インデックス構築）
- [ ] **GraphRAG構築**: LLMがページから概念ノード（定義/定理/証明など）を抽出 → `concepts` テーブルに保存
- [ ] **依存関係グラフ構築**: 概念間の参照関係を抽出 → `concept_edges` に保存

### Phase 3: Main Agent Q&A + 数式レンダリング
- [ ] 選択LaTeX → Main Agentに渡してQ&A
- [ ] React Flow でブランチ型会話キャンバス
- [ ] 枝分かれ（子セッション作成）
- [ ] 解決/未解決ボタン
- [ ] **KaTeX導入**: LLMの回答中の `$...$` / `$$...$$` を自動レンダリング
- [ ] **Math Input Normalizer**: ユーザー入力の非形式LaTeXを gpt-5.4-mini で正規化
- [ ] **入力プレビュー**: 入力欄でリアルタイムKaTeXプレビュー表示

### Phase 4: Memory Agent
- [ ] Ruri v3-310mのローカル実行環境（Python sidecar）
- [ ] 解決時の要約 + 埋め込み保存
- [ ] FTS5 + sqlite-vec ハイブリッド検索（RRF）
- [ ] 時間減衰スコアリング

### Phase 5: RAG Agent + Context Agent + Prefetch Agent + 透明性UI
- [ ] RAG Agent: 自由質問時に pages FTS5 + sqlite-vec でハイブリッド検索
- [ ] RAG Agent: GraphRAGによる概念グラフ探索（依存する定義・定理を芋づる式に取得）
- [ ] Context Agent: [RAG結果 or 選択LaTeX] + 過去解説 → システムプロンプト注入
- [ ] Prefetch Agent: 選択時に周辺ページを先読み
- [ ] **Adversarial Agent**: Main Agentの回答草案を受け取り、数学的厳密さ・記法・定義の一致をチェック → 修正ループ（最大2回）
- [ ] エージェント間の非同期オーケストレーション
- [ ] 質問モード（選択あり/なし）の自動判定ロジック
- [ ] **Agent Transparency Panel**: 各エージェントの実行状態をリアルタイム表示
- [ ] エージェントごとの入出力をクリックで展開できるUI
- [ ] **Context Compression Agent**: トークン使用量監視 → 60%超で自動圧縮
- [ ] トークン使用量バー（ヘッダー表示）
- [ ] 圧縮済みメッセージの「展開」復元UI

### Phase 6: UX 仕上げ
- [ ] LaTeX のレンダリング表示（KaTeX）
- [ ] 過去の解決済みセッション一覧
- [ ] キャンバスのズーム・パン・ミニマップ
- [ ] 教科書（本）の管理画面

---

## ディレクトリ構成（想定）

```
teacher/
├── src-tauri/          # Rust バックエンド
│   ├── src/
│   │   ├── main.rs
│   │   ├── db/         # SQLite操作
│   │   ├── pdf/        # PDF変換パイプライン
│   │   └── agents/     # Agent Orchestrator
│   └── Cargo.toml
├── src/                # React フロントエンド
│   ├── components/
│   │   ├── PdfViewer/
│   │   ├── Canvas/     # React Flow
│   │   └── ChatNode/
│   ├── store/          # Zustand
│   └── api/            # Tauri invoke wrappers
├── sidecar/            # Python (Ruri埋め込みモデル)
│   └── embedder.py
└── PLAN.md
```

---

## 後から追加できるサブエージェント例

- **Hint Agent**: 詰まっているとき自動でヒントを提案
- **Quiz Agent**: 理解度チェックのための小問を生成
- **Summary Agent**: 1章読み終わったときの要点まとめ
- **Cross-Reference Agent**: 他の章・定理との関連を検索

---

## 調査から得た追加アイデア

### 1. GraphRAG（概念グラフ型RAG）
通常のベクトルRAGより精度が高い。教科書の構造を活かせる。

```
定義2.1 ──定義する──▶ 極限
極限 ──使われる──▶ 定理3.1（連続性）
定理3.1 ──証明に使う──▶ ε-δ論法
```

- PDF→LaTeX変換時にLLMが概念間の依存関係グラフを構築
- 質問時はグラフを辿って関連する定義・定理・例題をまとめて取得
- 単純なページ検索より「なぜ？」に答えやすい

### 2. Adversarial Agent（EduPlanner方式）
Main Agentの回答品質を自動チェック。

```
Main Agent → 回答草案
    ↓
Evaluator Agent → 「この説明、ε-δの定義が曖昧」と指摘
    ↓
Main Agent → 修正版を生成
```

- ユーザーに届く前に回答を自動改善
- 数学の厳密さチェックに特に有効

### 3. Socratic Agent（ソクラテス式誘導）
直接答えを出すのではなく、問いかけで理解を誘導するモード。

- "この定義のどの部分が気になりますか？"
- "もしεを小さくしたらどうなると思いますか？"
- 解決ボタンを押すまで続く → 深い理解が記録される

### 4. Student Model Agent（知識状態の追跡）
Bayesian Knowledge Tracing的なアプローチ。

- 各概念の「習得確率」を保持
- 質問内容・解決率・反復回数から自動更新
- キャンバス上でヒートマップ表示（どの概念が弱いか一目でわかる）

### 5. コスト最適化戦略（モデル使い分け）

| タスク | モデル | 理由 |
|--------|--------|------|
| Q&A（thinking ON） | gpt-5.4 | 通常プランの上限、精度最優先 |
| Q&A（thinking OFF） | gpt-5.4-mini | 高速回答 |
| RAG検索・Memory検索 | gpt-5.4-mini | 高頻度・低コスト |
| 外部定義検索・Notation変換 | gpt-5.4-mini | 記法変換は比較的軽いタスク |
| PDF→LaTeX変換 | gpt-5.4 (Vision) | 数式精度最優先 |
| Evaluator Agent | gpt-5.4-mini | 軽量チェックで十分 |

---

## 参考

- [sui-memory: SQLite FTS5 + sqlite-vec による記憶管理](https://zenn.dev/noprogllama/articles/7c24b2c2410213)
- React Flow: https://reactflow.dev/
- Tauri v2: https://tauri.app/
- Ruri v3: cl-nagoya/ruri-v3-310m (HuggingFace)
