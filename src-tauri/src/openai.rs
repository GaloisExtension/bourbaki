//! OpenAI Vision（ページ → LaTeX）と軽量な概念抽出（JSON）。

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::json;

const VISION_USER_PROMPT: &str = r#"You are transcribing a scanned textbook page to LaTeX.

Rules:
- Output ONLY the LaTeX body for this single page (no preamble like \documentclass).
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
