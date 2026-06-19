#!/usr/bin/env python3
"""Optional visual-similarity sidecar for Sauce.

Loads a CLIP-style image encoder and prints an L2-normalised embedding (a JSON
float array) for the image at the given URL or path. Sauce shells out to this
script; if it (or its dependencies) isn't installed, the tool transparently
falls back to attribute-only ranking.

Usage:
    python3 embed_image.py <image-url-or-path>

Install (one-time):
    pip install open_clip_torch torch pillow requests

The model downloads on first run and is cached afterwards. CPU is fine.
"""

import io
import json
import sys
import urllib.request


def load_image(src: str):
    from PIL import Image  # imported lazily so --help works without deps

    if src.startswith("http://") or src.startswith("https://"):
        req = urllib.request.Request(src, headers={"User-Agent": "sauce-cli"})
        with urllib.request.urlopen(req, timeout=20) as resp:
            data = resp.read()
        return Image.open(io.BytesIO(data)).convert("RGB")
    return Image.open(src).convert("RGB")


def embed(src: str) -> list[float]:
    import open_clip
    import torch

    model, _, preprocess = open_clip.create_model_and_transforms(
        "ViT-B-32", pretrained="laion2b_s34b_b79k"
    )
    model.eval()

    image = preprocess(load_image(src)).unsqueeze(0)
    with torch.no_grad():
        feats = model.encode_image(image)
        feats = feats / feats.norm(dim=-1, keepdim=True)  # L2-normalise
    return feats.squeeze(0).tolist()


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: embed_image.py <image-url-or-path>", file=sys.stderr)
        return 2
    try:
        vec = embed(sys.argv[1])
    except Exception as exc:  # noqa: BLE001 — surface any failure to the caller
        print(f"embed failed: {exc}", file=sys.stderr)
        return 1
    json.dump(vec, sys.stdout)
    return 0


if __name__ == "__main__":
    sys.exit(main())
