//! PDF → PNG（Poppler `pdftoppm`）とページ数（lopdf）。

use std::path::{Path, PathBuf};
use std::process::Command;

pub fn pdftoppm_available() -> bool {
    Command::new("pdftoppm")
        .arg("-h")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn page_count(pdf_path: &Path) -> Result<u32, String> {
    let doc = lopdf::Document::load(pdf_path).map_err(|e| format!("lopdf: {e}"))?;
    Ok(doc.get_pages().len() as u32)
}

fn pdftoppm_dpi() -> String {
    std::env::var("MATH_TEACHER_PDFTOPPM_DPI")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "200".to_string())
}

/// `pdftoppm` が生成する `{prefix}-{page}.png` のパスを返す（page は 1-based）。
pub fn render_page_png(pdf_path: &Path, page_1based: u32, out_prefix: &Path) -> Result<PathBuf, String> {
    if !pdftoppm_available() {
        return Err(
            "Poppler の pdftoppm が見つかりません。macOS: brew install poppler".into(),
        );
    }
    let out_str = out_prefix.to_string_lossy().to_string();
    let p = page_1based.to_string();
    let dpi = pdftoppm_dpi();
    let status = Command::new("pdftoppm")
        .args([
            "-png",
            "-r",
            dpi.as_str(),
            "-f",
            p.as_str(),
            "-l",
            p.as_str(),
        ])
        .arg(pdf_path.as_os_str())
        .arg(&out_str)
        .status()
        .map_err(|e| format!("pdftoppm を起動できません: {e}"))?;
    if !status.success() {
        return Err(format!("pdftoppm が失敗しました (exit {:?})", status.code()));
    }
    let png_path = PathBuf::from(format!("{}-{}.png", out_str, page_1based));
    if !png_path.is_file() {
        return Err(format!("PNG が生成されませんでした: {:?}", png_path));
    }
    Ok(png_path)
}

#[cfg(test)]
mod tests {
    use super::page_count;
    use std::path::PathBuf;

    /// リポジトリ直下のサンプル（開発・CI で配置）
    #[test]
    fn text_linear_algebra_pdf_page_count() {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../text_linear_algebra.pdf");
        assert!(
            p.is_file(),
            "テスト用にリポジトリ直下へ text_linear_algebra.pdf を置いてください: {:?}",
            p
        );
        let n = page_count(&p).expect("valid pdf");
        assert!(
            n > 8,
            "サンプル PDF は複数ページ想定です (got {n} pages): {:?}",
            p
        );
    }
}
