import { getDocument } from "pdfjs-dist";

type PdfDocInit = Parameters<typeof getDocument>[0];

/**
 * 日本語など CID マップを使う PDF 向けに cMap / 標準フォントパスを渡す。
 * `scripts/copy-pdfjs-assets.mjs` で public/pdfjs に配置済みであること。
 */
export function getPdfDocumentInit(url: string): PdfDocInit {
  const base = `${import.meta.env.BASE_URL}pdfjs/`;
  return {
    url,
    cMapUrl: `${base}cmaps/`,
    cMapPacked: true,
    standardFontDataUrl: `${base}standard_fonts/`,
    useSystemFonts: true,
  };
}
