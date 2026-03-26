# Bourbaki

> 数学の教科書PDFを一緒に読む、マルチエージェント学習アプリ

**Bourbaki**（ブルバキ）は、数学の教科書PDFをインポートし、ページ上のテキストを選択してLLMに質問できるmacOSデスクトップアプリです。複数のAIエージェントが協調して回答の精度を高めます。名前は、複数の数学者による匿名集団「ニコラ・ブルバキ」に由来し、マルチエージェントアーキテクチャと重ねています。

---

## 特徴

### ブランチ型会話キャンバス
PDFのテキストを選択してそのまま質問。回答はReact Flowのノードとして展開され、枝分かれする追加質問も自由に作れます。

### PDF → LaTeX 変換
poppler + GPT-4o Vision でPDFを1ページずつLaTeXに変換・ローカルDBへ保存。数式もそのままコンテキストとして扱えます。

### マルチエージェント協調

| エージェント | 役割 |
|-------------|------|
| **Main Agent** | 回答生成（GPT-4o、Thinking ON/OFF切替可） |
| **RAG Agent** | 書籍全体をFTS5+ベクトル検索して関連箇所を抽出 |
| **Context Agent** | 過去の解決済み解説をシステムプロンプトに注入 |
| **Prefetch Agent** | 選択ページ周辺を先読みしてレイテンシ削減 |
| **Adversarial Agent** | 回答草案の数学的厳密さ・記法を自動チェック → 修正ループ |
| **Context Compression Agent** | 長い会話を自動圧縮してトークン使用量を管理 |

### ChatGPT セッション認証（APIキー不要）
設定画面からChatGPTにOAuth 2.0 PKCEでログインし、ChatGPT Plusアカウントのセッションをそのまま利用できます。OpenAI APIキーは不要です。

### ローカルファースト
SQLite (FTS5 + ベクトル検索) をローカルに保存。解決済み会話の要約・埋め込みも全てデバイス内で完結します。

---

## 必要環境

| ツール | バージョン | 用途 |
|--------|-----------|------|
| macOS | 13+ | ターゲットプラットフォーム |
| Rust | 1.77+ | Tauriバックエンド |
| Node.js | 20+ | フロントエンドビルド |
| Python | 3.10+ | 埋め込みモデルサイドカー |
| poppler | — | PDF → PNG変換（`pdftoppm`） |

```sh
# Homebrew で一括インストール
brew install rust node python poppler
```

---

## セットアップ

```sh
# 1. リポジトリをクローン
git clone https://github.com/GaloisExtension/bourbaki.git
cd bourbaki

# 2. Node依存関係インストール
npm install

# 3. Pythonサイドカー（埋め込みモデル）
cd sidecar && pip install -r requirements.txt && cd ..

# 4. 開発サーバー起動
npm run tauri dev
```

### LLM設定

LLMの設定方法は2通りあります。詳細は **[docs/setup-llm.md](docs/setup-llm.md)** を参照してください。

| 方法 | 概要 |
|------|------|
| ChatGPT OAuth | アプリ内の「設定 → ChatGPT にログイン」でPKCEログイン |
| OpenAI API キー | 環境変数 `OPENAI_API_KEY` またはアプリ設定画面から入力 |

---

## 使い方

1. **PDF を開く** — ツールバー「PDFを開く」でファイルを選択
2. **LaTeX 取り込み** — 「LaTeX取り込み」ボタンでバックグラウンド変換開始（poppler + Vision API）
3. **ベクトル化** — 変換完了後「ベクトル化」でRAG用インデックスを構築
4. **質問する** — PDFのテキストをドラッグ選択 → 「新しいスレッド」→ チャット入力
5. **枝分かれ** — 回答ノードの「枝を追加」で追加質問を作成
6. **解決済み** — 納得したら「解決」ボタンで要約を記録（以降のRAGに活用される）

---

## 技術スタック

```
Tauri v2 (Rust + React + TypeScript)
├── PDF表示          PDF.js
├── キャンバスUI     React Flow
├── 状態管理         Zustand
├── 数式レンダリング  KaTeX
├── ローカルDB       SQLite (FTS5 + sqlite-vec)
└── 埋め込みモデル   cl-nagoya/ruri-v3-310m (Python sidecar)
```

---

## ライセンス

MIT
