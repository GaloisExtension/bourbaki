//! OpenAI Vision（ページ → LaTeX）と軽量な概念抽出（JSON）。

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::json;

const VISION_USER_PROMPT: &str = r#"You are transcribing a textbook page (may include Japanese) to LaTeX.

Rules:
- Output ONLY the LaTeX body for this single page (no preamble like \documentclass).
- **Japanese (CJK) fidelity**: Transcribe ALL visible Japanese text accurately (headings, definitions, 注, 例, body text). Use \text{...} or \textbf{...} inside math mode, or plain UTF-8 outside. Do not skip or summarize Japanese prose as "..." unless illegible.
- Preserve mathematical meaning: environments (definition, theorem, proof, equation, align, etc.) when visible.
- Use standard LaTeX; for unknown symbols, approximate with \text{} and comments.
- If the page is mostly figures with little text, describe briefly in \paragraph{} then give key formulas.
- Wrap your final answer in a single fenced block: ```latex ... ```"#;

fn extract_latex_fence(content: &str) -> String {
    let trimmed = content.trim();
    if let Some(start) = trimmed.find("```") {
        let after = trimmed[start + 3..].trim_start();
        let after = after
            .trim_start_matches("latex")
            .trim_start_matches("LaTeX")
            .trim_start();
        if let Some(end) = after.find("```") {
            return after[..end].trim().to_string();
        }
        return after.trim().to_string();
    }
    trimmed.to_string()
}

pub async fn transcribe_page_to_latex(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    png_bytes: &[u8],
) -> Result<String, String> {
    let b64 = base64::engine::general_purpose::STANDARD.encode(png_bytes);
    let url = format!("data:image/png;base64,{b64}");

    let body = json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": VISION_USER_PROMPT},
                {"type": "image_url", "image_url": {"url": url}}
            ]
        }],
        "max_tokens": 8192
    });

    let res = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenAI HTTP: {e}"))?;

    if !res.status().is_success() {
        let txt = res.text().await.unwrap_or_default();
        return Err(format!("OpenAI error: {txt}"));
    }

    let v: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "OpenAI: missing choices[0].message.content".to_string())?;
    Ok(extract_latex_fence(content))
}

#[derive(Debug, Deserialize)]
struct ConceptPayload {
    concepts: Vec<ConceptItem>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ConceptItem {
    #[serde(rename = "type")]
    pub kind: String,
    pub label: Option<String>,
    pub name: Option<String>,
    pub latex: String,
}

const CONCEPT_PROMPT: &str = r#"From the following LaTeX page fragment, extract structural math items.

Return a JSON object with key "concepts" only. Each item:
- type: one of definition, theorem, lemma, example, proof, remark, other
- label: short label if any (e.g. 定義2.1), or null
- name: concept name in Japanese or English if clear, or null
- latex: short verbatim LaTeX excerpt (max ~400 chars) from the source

Keep 0–12 items. If nothing structural, return {"concepts":[]}."#;

pub async fn extract_concepts_json(
    client: &reqwest::Client,
    api_key: &str,
    mini_model: &str,
    latex_page: &str,
) -> Result<Vec<ConceptItem>, String> {
    let clip = if latex_page.len() > 12000 {
        &latex_page[..12000]
    } else {
        latex_page
    };

    let body = json!({
        "model": mini_model,
        "messages": [{
            "role": "user",
            "content": format!("{CONCEPT_PROMPT}\n\n---\n{clip}\n---")
        }],
        "response_format": {"type": "json_object"},
        "max_tokens": 4096
    });

    let res = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenAI HTTP: {e}"))?;

    if !res.status().is_success() {
        let txt = res.text().await.unwrap_or_default();
        return Err(format!("OpenAI (concepts): {txt}"));
    }

    let v: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "missing content".to_string())?;

    let parsed: ConceptPayload =
        serde_json::from_str(content).map_err(|e| format!("concept JSON: {e}"))?;
    Ok(parsed.concepts)
}

const NORMALIZE_MATH_PROMPT: &str = r#"The user wrote informal math or mixed Japanese + math. Convert to short, valid inline LaTeX for the tutor model.
- Keep Japanese explanations as plain Unicode (outside math) where appropriate.
- Use $...$ only for pure math fragments when needed; prefer one concise line.
Return JSON only: {"normalized": "<single line or short paragraph>"}"#;

pub async fn normalize_math_input(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    user_text: &str,
) -> Result<String, String> {
    let body = json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": format!("{NORMALIZE_MATH_PROMPT}\n\n---\n{user_text}\n---")
        }],
        "response_format": {"type": "json_object"},
        "max_tokens": 1024
    });

    let res = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenAI HTTP (normalize): {e}"))?;

    if !res.status().is_success() {
        let txt = res.text().await.unwrap_or_default();
        return Err(format!("OpenAI (normalize): {txt}"));
    }

    let v: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "normalize: missing content".to_string())?;
    let obj: serde_json::Value = serde_json::from_str(content).map_err(|e| e.to_string())?;
    let n = obj["normalized"]
        .as_str()
        .unwrap_or(user_text)
        .trim()
        .to_string();
    if n.is_empty() {
        Ok(user_text.trim().to_string())
    } else {
        Ok(n)
    }
}

const TEACHER_SYSTEM: &str = r#"You are a careful mathematics tutor for a student reading a Japanese textbook.
- Answer primarily in Japanese.
- Use consistent notation with the provided LaTeX excerpt when present.
- Use $...$ for inline math and $$...$$ for display math where appropriate.
- If the question is ambiguous, ask a brief clarifying question.
- Do not fabricate theorem numbers that are not suggested by the context."#;

pub async fn chat_teacher_reply(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    selection_latex: Option<&str>,
    selection_text: Option<&str>,
    history: &[(String, String)],
    user_message: &str,
) -> Result<String, String> {
    let mut latex_block = match selection_latex {
        Some(s) if !s.trim().is_empty() => s.to_string(),
        _ => "（未取り込み）".to_string(),
    };
    if latex_block.len() > 24000 {
        latex_block = latex_block.chars().take(24000).collect::<String>();
        latex_block.push_str("\n…(truncated)");
    }
    let sel_txt = match selection_text {
        Some(s) if !s.trim().is_empty() => s,
        _ => "（なし）",
    };
    let system = format!(
        "{TEACHER_SYSTEM}\n\n--- LaTeX 抜粋（参照用） ---\n{latex_block}\n\n--- PDF 選択テキスト ---\n{sel_txt}"
    );

    let mut messages: Vec<serde_json::Value> = vec![json!({
        "role": "system",
        "content": system
    })];
    for (role, content) in history {
        if role != "user" && role != "assistant" {
            continue;
        }
        messages.push(json!({ "role": role, "content": content }));
    }
    messages.push(json!({ "role": "user", "content": user_message }));

    let body = json!({
        "model": model,
        "messages": messages,
        "max_tokens": 4096
    });

    let res = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenAI HTTP (chat): {e}"))?;

    if !res.status().is_success() {
        let txt = res.text().await.unwrap_or_default();
        return Err(format!("OpenAI (chat): {txt}"));
    }

    let v: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "chat: missing content".to_string())?;
    Ok(content.trim().to_string())
}

pub async fn chat_teacher_reply_with_context(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    selection_latex: Option<&str>,
    rag_context: Option<&str>,
    memory_context: Option<&str>,
    selection_text: Option<&str>,
    history: &[(String, String)],
    user_message: &str,
) -> Result<String, String> {
    // 参照文脈: 選択LaTeX優先、次にRAG結果
    let latex_block = match selection_latex.filter(|s| !s.trim().is_empty()) {
        Some(s) => {
            let clipped: String = s.chars().take(24000).collect();
            clipped
        }
        None => match rag_context.filter(|s| !s.trim().is_empty()) {
            Some(s) => {
                let clipped: String = s.chars().take(24000).collect();
                clipped
            }
            None => "（未取り込み）".to_string(),
        },
    };
    let sel_txt = match selection_text {
        Some(s) if !s.trim().is_empty() => s,
        _ => "（なし）",
    };

    let mut system = format!(
        "{TEACHER_SYSTEM}\n\n--- 参照文脈（教科書LaTeX） ---\n{latex_block}\n\n--- PDF選択テキスト ---\n{sel_txt}"
    );
    if let Some(mem) = memory_context.filter(|s| !s.trim().is_empty()) {
        system.push_str(&format!("\n\n--- 過去の解説メモ（参考） ---\n{mem}"));
    }

    let mut messages: Vec<serde_json::Value> = vec![json!({
        "role": "system",
        "content": system
    })];
    for (role, content) in history {
        if role != "user" && role != "assistant" {
            continue;
        }
        messages.push(json!({ "role": role, "content": content }));
    }
    messages.push(json!({ "role": "user", "content": user_message }));

    let body = json!({
        "model": model,
        "messages": messages,
        "max_tokens": 4096
    });

    let res = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenAI HTTP (chat_ctx): {e}"))?;

    if !res.status().is_success() {
        let txt = res.text().await.unwrap_or_default();
        return Err(format!("OpenAI (chat_ctx): {txt}"));
    }

    let v: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "chat_ctx: missing content".to_string())?;
    Ok(content.trim().to_string())
}

const ADVERSARIAL_PROMPT: &str = r#"You are an adversarial math reviewer checking a tutor's draft reply before it reaches the student.

Check the draft for:
1. Mathematical accuracy: definitions, theorems, proofs are correct
2. Notation consistency: matches the provided LaTeX excerpt from the textbook
3. No hallucinated theorem numbers or references not supported by the context

Reply in JSON only:
{
  "ok": true | false,
  "issues": ["issue1", "issue2"],  // empty if ok
  "corrected_draft": "..."          // improved version if ok=false, else same as draft
}"#;

pub async fn adversarial_check(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    draft: &str,
    context_latex: &str,
) -> Result<(bool, String), String> {
    let ctx_clip = if context_latex.len() > 8000 {
        &context_latex[..8000]
    } else {
        context_latex
    };
    let user_content = format!(
        "{ADVERSARIAL_PROMPT}\n\n--- LaTeX Context ---\n{ctx_clip}\n\n--- Draft Reply ---\n{draft}"
    );
    let body = json!({
        "model": model,
        "messages": [{"role": "user", "content": user_content}],
        "response_format": {"type": "json_object"},
        "max_tokens": 4096
    });

    let res = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenAI HTTP (adversarial): {e}"))?;

    if !res.status().is_success() {
        let txt = res.text().await.unwrap_or_default();
        return Err(format!("OpenAI (adversarial): {txt}"));
    }

    let v: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "adversarial: missing content".to_string())?;
    let obj: serde_json::Value =
        serde_json::from_str(content).map_err(|e| format!("adversarial JSON: {e}"))?;
    let ok = obj["ok"].as_bool().unwrap_or(true);
    let corrected = obj["corrected_draft"]
        .as_str()
        .unwrap_or(draft)
        .trim()
        .to_string();
    Ok((ok, if corrected.is_empty() { draft.to_string() } else { corrected }))
}

const COMPRESS_PROMPT: &str = r#"次の会話ログを圧縮してください。後から検索・参照できるよう重要な情報を保持してください。

要件:
- 日本語で箇条書き（各行は "- " で始める）
- 扱った数学概念・定理・記法・疑問点とその解答を具体的に記述
- 会話の流れより「何が分かったか」に焦点を当てる

応答はJSON: {"summary": "..."}"#;

pub async fn compress_old_messages(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    messages: &[(String, String)],
) -> Result<String, String> {
    let transcript: Vec<String> = messages
        .iter()
        .map(|(role, c)| format!("{role}: {c}"))
        .collect();
    let text = transcript.join("\n");
    let clip = if text.len() > 20_000 {
        text.chars().take(20_000).collect::<String>()
    } else {
        text
    };

    let body = json!({
        "model": model,
        "messages": [{"role": "user", "content": format!("{COMPRESS_PROMPT}\n\n---\n{clip}\n---")}],
        "response_format": {"type": "json_object"},
        "max_tokens": 2048
    });

    let res = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenAI HTTP (compress): {e}"))?;

    if !res.status().is_success() {
        let txt = res.text().await.unwrap_or_default();
        return Err(format!("OpenAI (compress): {txt}"));
    }

    let v: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "compress: missing content".to_string())?;
    let obj: serde_json::Value = serde_json::from_str(content).map_err(|e| e.to_string())?;
    let s = obj["summary"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();
    if s.is_empty() {
        Err("圧縮サマリーが空でした".into())
    } else {
        Ok(s)
    }
}

const MEMORY_SUMMARY_PROMPT: &str = r#"次のログは数学チューターと学生の会話です。後から検索できる「解決済みメモリ」用の要約を作ってください。

要件:
- 日本語で、5〜12 行の箇条書き（各行は "- " で始める）
- 扱った概念・定理・記法・つまずきとその解消を具体的に（定理番号が分かれば書く）
- 会話の逐語録は避け、知識の整理に重心を置く

応答は JSON のみ: {"summary": "..."}"#;

pub async fn summarize_session_memory(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    transcript: &str,
) -> Result<String, String> {
    if transcript.trim().is_empty() {
        return Err("要約する会話がありません".into());
    }
    let clip = if transcript.len() > 28_000 {
        transcript.chars().take(28_000).collect::<String>()
    } else {
        transcript.to_string()
    };

    let body = json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": format!("{MEMORY_SUMMARY_PROMPT}\n\n---\n{clip}\n---")
        }],
        "response_format": {"type": "json_object"},
        "max_tokens": 2048
    });

    let res = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("OpenAI HTTP (memory summary): {e}"))?;

    if !res.status().is_success() {
        let txt = res.text().await.unwrap_or_default();
        return Err(format!("OpenAI (memory summary): {txt}"));
    }

    let v: serde_json::Value = res.json().await.map_err(|e| e.to_string())?;
    let content = v["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| "memory summary: missing content".to_string())?;
    let obj: serde_json::Value = serde_json::from_str(content).map_err(|e| e.to_string())?;
    let s = obj["summary"]
        .as_str()
        .ok_or_else(|| "memory summary: missing summary key".to_string())?
        .trim()
        .to_string();
    if s.is_empty() {
        return Err("要約が空でした".into());
    }
    Ok(s)
}
