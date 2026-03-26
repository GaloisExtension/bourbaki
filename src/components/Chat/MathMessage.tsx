import { Fragment, type ReactNode } from "react";
import katex from "katex";
import "katex/dist/katex.min.css";
import "./math-message.css";

function renderKatex(math: string, display: boolean, key: string): ReactNode {
  try {
    const html = katex.renderToString(math, {
      displayMode: display,
      throwOnError: false,
      strict: "ignore",
    });
    return (
      <span
        key={key}
        className={display ? "math-msg__display" : "math-msg__inline"}
        dangerouslySetInnerHTML={{ __html: html }}
      />
    );
  } catch {
    return (
      <code key={key} className="math-msg__fallback">
        {math}
      </code>
    );
  }
}

/** LLM 応答など、$...$ / $$...$$ を含むプレーンテキストを KaTeX 混在で描画 */
export function MathMessage({ content }: { content: string }) {
  const nodes: ReactNode[] = [];
  let key = 0;
  let rest = content;

  while (rest.length > 0) {
    const di = rest.indexOf("$$");
    if (di === -1) {
      nodes.push(...renderInlineDollars(rest, key));
      break;
    }
    if (di > 0) {
      nodes.push(...renderInlineDollars(rest.slice(0, di), key));
      key += 1000;
    }
    const end = rest.indexOf("$$", di + 2);
    if (end === -1) {
      nodes.push(...renderInlineDollars(rest.slice(di), key));
      break;
    }
    const math = rest.slice(di + 2, end).trim();
    if (math) {
      nodes.push(renderKatex(math, true, `d${key++}`));
    }
    rest = rest.slice(end + 2);
  }

  return (
    <div className="math-msg">
      {nodes.map((n, i) => (
        <Fragment key={i}>{n}</Fragment>
      ))}
    </div>
  );
}

function renderInlineDollars(text: string, keyBase: number): ReactNode[] {
  const out: ReactNode[] = [];
  const re = /\$([^$\n]+)\$/g;
  let last = 0;
  let m: RegExpExecArray | null;
  let k = keyBase;
  while ((m = re.exec(text)) !== null) {
    if (m.index > last) {
      out.push(
        <span key={`t${k++}`} className="math-msg__text">
          {text.slice(last, m.index)}
        </span>,
      );
    }
    const inner = m[1]?.trim() ?? "";
    if (inner) {
      out.push(renderKatex(inner, false, `i${k++}`));
    }
    last = m.index + m[0].length;
  }
  if (last < text.length) {
    out.push(
      <span key={`t${k++}`} className="math-msg__text">
        {text.slice(last)}
      </span>,
    );
  }
  return out;
}
