import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  getSettings,
  logoutChatgpt,
  openChatgptLogin,
  saveVisionApiKey,
} from "../../api/commands";
import "./Settings.css";

type Props = {
  onClose: () => void;
};

export function Settings({ onClose }: Props) {
  const [chatgptLoggedIn, setChatgptLoggedIn] = useState(false);
  const [hasVisionKey, setHasVisionKey] = useState(false);
  const [visionKey, setVisionKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [loginPending, setLoginPending] = useState(false);

  const refresh = useCallback(() => {
    getSettings()
      .then((s) => {
        setChatgptLoggedIn(s.chatgptLoggedIn);
        setHasVisionKey(s.hasVisionKey);
      })
      .catch(console.error);
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // chatgpt-login-done イベントでログイン完了を検知
  const refreshRef = useRef(refresh);
  refreshRef.current = refresh;
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen<{ ok: boolean; error?: string }>("chatgpt-login-done", (e) => {
      setLoginPending(false);
      if (e.payload.ok) {
        refreshRef.current();
      }
    }).then((fn) => { unlisten = fn; });
    return () => { unlisten?.(); };
  }, []);

  const handleLogin = useCallback(async () => {
    setLoginPending(true);
    await openChatgptLogin();
    // ブラウザが開くだけなので pending のまま待機 (chatgpt-login-done で解除)
  }, []);

  const handleLogout = useCallback(async () => {
    await logoutChatgpt();
    refresh();
  }, [refresh]);

  const handleSaveVisionKey = useCallback(async () => {
    setSaving(true);
    try {
      await saveVisionApiKey(visionKey.trim());
      setVisionKey("");
      refresh();
    } finally {
      setSaving(false);
    }
  }, [visionKey, refresh]);

  return (
    <div className="settings-backdrop" onClick={onClose}>
      <div className="settings-modal" onClick={(e) => e.stopPropagation()}>
        <div className="settings-header">
          <h2 className="settings-title">設定</h2>
          <button className="settings-close" onClick={onClose}>✕</button>
        </div>

        <div className="settings-body">
          {/* ── ChatGPT セクション ── */}
          <section className="settings-section">
            <h3 className="settings-section-title">ChatGPT セッション（チャット用）</h3>
            <p className="settings-desc">
              ChatGPTにログインして、アカウントのセッションをチャットに利用します。
              Vision APIキー不要でチャットが動きます。
            </p>

            {chatgptLoggedIn ? (
              <div className="settings-row">
                <span className="settings-badge settings-badge--ok">✅ ログイン済み</span>
                <button
                  className="settings-btn settings-btn--danger"
                  onClick={handleLogout}
                >
                  ログアウト
                </button>
              </div>
            ) : (
              <div className="settings-row">
                <span className="settings-badge settings-badge--ng">
                  {loginPending ? "ブラウザでログイン中…" : "未ログイン"}
                </span>
                <button
                  className="settings-btn settings-btn--primary"
                  disabled={loginPending}
                  onClick={handleLogin}
                >
                  {loginPending ? "待機中…" : "ChatGPT にログイン"}
                </button>
              </div>
            )}

            <div className="settings-hint">
              ボタンを押すとシステムブラウザで ChatGPT が開きます。
              ログイン後、ブラウザを閉じると自動で連携されます。（OAuth 2.0 PKCE）
            </div>
          </section>

          {/* ── Vision API セクション ── */}
          <section className="settings-section">
            <h3 className="settings-section-title">OpenAI APIキー（PDF取り込み・ユーティリティ用）</h3>
            <p className="settings-desc">
              PDF → LaTeX 変換（Vision API）、正規化、Adversarial チェックに使用します。
              環境変数 <code>OPENAI_API_KEY</code> がある場合は不要です。
            </p>

            <div className="settings-row">
              {hasVisionKey ? (
                <span className="settings-badge settings-badge--ok">✅ 設定済み</span>
              ) : (
                <span className="settings-badge settings-badge--ng">未設定</span>
              )}
            </div>

            <div className="settings-input-row">
              <input
                type="password"
                className="settings-input"
                placeholder="sk-..."
                value={visionKey}
                onChange={(e) => setVisionKey(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" && visionKey.trim()) void handleSaveVisionKey();
                }}
              />
              <button
                className="settings-btn settings-btn--primary"
                disabled={!visionKey.trim() || saving}
                onClick={() => void handleSaveVisionKey()}
              >
                {saving ? "保存中…" : "保存"}
              </button>
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}
