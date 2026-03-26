#!/usr/bin/env node
/**
 * pdfjs-dist の cmaps / standard_fonts を public にコピー（日本語PDFの CID フォント用）。
 * npm install / postinstall で実行。
 */
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(__dirname, "..");
const dist = path.join(root, "node_modules", "pdfjs-dist");
const cmapsFrom = path.join(dist, "cmaps");
const fontsFrom = path.join(dist, "standard_fonts");
const destBase = path.join(root, "public", "pdfjs");

function cpDir(from, to) {
  if (!fs.existsSync(from)) {
    console.warn(`[copy-pdfjs-assets] skip (missing): ${from}`);
    return;
  }
  fs.mkdirSync(path.dirname(to), { recursive: true });
  fs.cpSync(from, to, { recursive: true });
}

cpDir(cmapsFrom, path.join(destBase, "cmaps"));
cpDir(fontsFrom, path.join(destBase, "standard_fonts"));
console.log("[copy-pdfjs-assets] ok → public/pdfjs/");
