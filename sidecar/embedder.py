#!/usr/bin/env python3
"""
stdin: 1 行 JSON — {"text": "...", "query": false}
  query true のとき E5 系は "query: " プレフィックス（検索クエリ用）
stdout: 1 行 JSON — {"dim": N, "b64": "<f32 little-endian base64>"}

環境変数:
  MATH_TEACHER_EMBED_MODEL — 既定 intfloat/multilingual-e5-small（日本語可・軽量）
  フル Ruri に切り替える例: cl-nagoya/ruri-v3-310m（初回 DL 大）

初回実行: pip install -r sidecar/requirements.txt
"""

from __future__ import annotations

import base64
import json
import os
import sys


def main() -> None:
    line = sys.stdin.readline()
    if not line:
        sys.exit(1)
    obj = json.loads(line)
    text = obj.get("text") or ""
    is_query = bool(obj.get("query"))
    model_name = os.environ.get(
        "MATH_TEACHER_EMBED_MODEL", "intfloat/multilingual-e5-small"
    )

    try:
        from sentence_transformers import SentenceTransformer
    except ImportError as e:
        print(json.dumps({"error": f"sentence_transformers 未インストール: {e}"}))
        sys.exit(2)

    try:
        model = SentenceTransformer(model_name)
        mlow = model_name.lower()
        prefix = ""
        if "e5" in mlow:
            prefix = "query: " if is_query else "passage: "
        elif is_query and "ruri" in mlow:
            # Ruri はクエリ指示がモデル依存のため、必要なら環境変数で上書き
            prefix = os.environ.get("MATH_TEACHER_RURI_QUERY_PREFIX", "")
        to_encode = prefix + text
        vec = model.encode(
            to_encode,
            normalize_embeddings=True,
            convert_to_numpy=True,
            show_progress_bar=False,
        )
    except Exception as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(3)

    flat = vec.astype("float32").flatten()
    raw = flat.tobytes()
    out = {"dim": int(flat.shape[0]), "b64": base64.b64encode(raw).decode("ascii")}
    print(json.dumps(out), flush=True)


if __name__ == "__main__":
    main()
