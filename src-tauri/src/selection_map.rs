//! PDF 選択テキスト → 当該ページ LaTeX のスニペット推定。

/// 選択に対応しそうな LaTeX 範囲を前後 `pad_chars` 文字付きで返す。
pub fn map_selection_to_excerpt(
    page_latex: &str,
    selection: &str,
    pad_chars: usize,
) -> Option<String> {
    let sel = selection.trim();
    if sel.is_empty() {
        return None;
    }

    if let Some(i) = page_latex.find(sel) {
        return Some(extract(page_latex, i, i + sel.len(), pad_chars));
    }

    if let Some((a, b)) = find_span_no_whitespace(page_latex, sel) {
        return Some(extract(page_latex, a, b, pad_chars));
    }

    find_span_by_nonempty_lines(page_latex, selection)
        .map(|(a, b)| extract(page_latex, a, b, pad_chars))
}

fn extract(latex: &str, start: usize, end: usize, pad: usize) -> String {
    let start = start.saturating_sub(pad);
    let end = (end + pad).min(latex.len());
    latex.get(start..end).unwrap_or("").trim().to_string()
}

/// 空白を無視して部分列一致（英数字・日本語ブロック混在向け）。
fn find_span_no_whitespace(latex: &str, sel: &str) -> Option<(usize, usize)> {
    let sel_ns: String = sel.chars().filter(|c| !c.is_whitespace()).collect();
    if sel_ns.chars().count() < 2 {
        return None;
    }
    let bytes: Vec<(usize, char)> = latex
        .char_indices()
        .filter(|(_, c)| !c.is_whitespace())
        .collect();
    if bytes.is_empty() {
        return None;
    }
    let flat: String = bytes.iter().map(|(_, c)| *c).collect();
    let start_c = flat.find(&sel_ns)?;
    let span = sel_ns.chars().count();
    let end_c = start_c + span;
    if end_c > bytes.len() {
        return None;
    }
    let (start_b, _) = bytes[start_c];
    let (end_b, ec) = bytes[end_c - 1];
    Some((start_b, end_b + ec.len_utf8()))
}

fn find_span_by_nonempty_lines(latex: &str, selection: &str) -> Option<(usize, usize)> {
    let lines: Vec<&str> = selection
        .lines()
        .map(str::trim)
        .filter(|l| l.chars().count() >= 2)
        .collect();
    if lines.is_empty() {
        return None;
    }
    let mut min_b = usize::MAX;
    let mut max_b = 0usize;
    let mut any = false;
    for line in lines {
        if let Some(i) = latex.find(line) {
            any = true;
            min_b = min_b.min(i);
            max_b = max_b.max(i + line.len());
        }
    }
    if any {
        Some((min_b, max_b.max(min_b)))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_raw_substring() {
        let l = r"see \mathbb{R}^n for Euclidean space";
        let ex = map_selection_to_excerpt(l, r"\mathbb{R}^n", 6).unwrap();
        assert!(ex.contains("mathbb"), "got: {ex:?}");
    }

    #[test]
    fn finds_when_spaces_differ() {
        let l = "abc  def\nghi";
        let ex = map_selection_to_excerpt(l, "abcdef", 2);
        assert!(ex.is_some());
    }

    #[test]
    fn line_fragments() {
        let l = "定理 1.2.3 次が成り立つ";
        let ex = map_selection_to_excerpt(l, "定理 1.2.3", 4).unwrap();
        assert!(ex.contains("定理"));
    }
}
