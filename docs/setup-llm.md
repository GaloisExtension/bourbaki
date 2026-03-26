# LLM セットアップガイド

Bourbakiでは2つの方法でLLMを利用できます。どちらか一方を設定すれば動作します。

---

## 方法①：ChatGPT OAuth（推奨・APIキー不要）

ChatGPT Plus/Proアカウントのセッションを使ってLLMを呼び出します。OpenAI APIキーは不要です。

### 仕組み

[openai/codex CLI](https://github.com/openai/codex) と同じOAuth 2.0 PKCEフローを使います。

```
アプリ → システムブラウザで auth.openai.com を開く
       → ログイン完了後 localhost:1455/auth/callback にリダイレクト
       → authorization code をトークンと交換
       → access_token をアプリデータディレクトリに保存
```

### 手順

1. アプリを起動し、ツールバーの **「⚙ 設定」** をクリック
2. 「ChatGPT セッション」セクションの **「ChatGPT にログイン」** を押す
3. システムブラウザでChatGPTのログイン画面が開く
4. ChatGPTにログインする
5. 「✅ ログイン完了！このタブは閉じて構いません。」と表示されたらブラウザを閉じる
6. 設定画面のステータスが **「✅ ログイン済み」** に変わる

### 注意事項

- ChatGPT Plus または Pro アカウントが必要です（Free プランでは利用制限あり）
- トークンは `~/Library/Application Support/com.math-teacher.app/chatgpt_session.json` に保存されます
- ログアウトしたい場合は設定画面の「ログアウト」ボタンを押してください
- コールバックにはポート `1455` を使います。占有されている場合は `8080`, `8787`, `9090` の順に自動でフォールバックします

---

## 方法②：OpenAI API キー

直接OpenAI APIを呼び出します。従量課金が発生します。

### 手順

#### A. 環境変数で設定（永続）

```sh
# ~/.zshrc または ~/.zprofile に追加
export OPENAI_API_KEY="sk-..."
```

追加後に `source ~/.zshrc` またはターミナルを再起動してから `npm run tauri dev` を実行してください。

#### B. アプリ設定画面から入力

1. **「⚙ 設定」** → 「OpenAI API キー」セクション
2. `sk-...` 形式のキーを入力して **「保存」** を押す
3. ステータスが **「✅ 設定済み」** になれば完了

設定画面から入力したキーも `chatgpt_session.json` に暗号化せず保存されます（ローカルのみ）。

### API キーの取得

1. [platform.openai.com/api-keys](https://platform.openai.com/api-keys) にアクセス
2. **「Create new secret key」** をクリック
3. 生成されたキー（`sk-...`）をコピー

---

## 機能ごとの使用モデルと認証方法

| 機能 | モデル | 認証方法 |
|------|--------|---------|
| チャット（Thinking ON） | gpt-4o | ChatGPT OAuth **または** API キー |
| チャット（Thinking OFF） | gpt-4o | ChatGPT OAuth **または** API キー |
| PDF → LaTeX 変換 | gpt-4o-mini (Vision) | API キーのみ |
| Math Input Normalizer | gpt-4o-mini | API キーのみ |
| Adversarial Agent | gpt-4o-mini | API キーのみ |

> ChatGPT OAuthはチャット機能専用です。PDF変換や各種ユーティリティはOpenAI APIキーが別途必要です。

---

## 優先順位

同じ機能に対して両方の設定がある場合、以下の優先順位で使用されます。

```
1. ChatGPT OAuth のトークン（チャット機能のみ）
2. 環境変数 OPENAI_API_KEY
3. アプリ設定画面で保存したAPIキー
```

---

## トラブルシューティング

### 「ChatGPT セッション切れ」エラーが出る

アクセストークンの有効期限が切れています。設定画面からログアウトして再ログインしてください。

### コールバックポートが使えない

ポート 1455 が別プロセスに使われている場合は自動でフォールバックします。すべてのポートが使用中の場合は以下で確認してください。

```sh
lsof -i :1455
lsof -i :8080
```

### PDF変換が動かない

PDF → LaTeX 変換にはOpenAI APIキーと `poppler`（`pdftoppm` コマンド）が必要です。

```sh
# popplerの確認
which pdftoppm

# インストールされていない場合
brew install poppler
```

### 埋め込み（ベクトル化）が動かない

Pythonサイドカーのセットアップが必要です。

```sh
cd sidecar
pip install -r requirements.txt
```

初回実行時にモデル（`cl-nagoya/ruri-v3-310m`、約1GB）が自動ダウンロードされます。
