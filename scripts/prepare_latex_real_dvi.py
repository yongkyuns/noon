#!/usr/bin/env python3
"""Decode bundled BaKoMa TFMs for the real-DVI test."""
import argparse, base64, json, pathlib

parser = argparse.ArgumentParser()
parser.add_argument("--tfm-json", required=True, type=pathlib.Path)
parser.add_argument("--out", required=True, type=pathlib.Path)
args = parser.parse_args()

table = json.loads(args.tfm_json.read_text())
names = sorted(table)
args.out.mkdir(parents=True, exist_ok=True)
for name in names:
    (args.out / f"{name}.tfm").write_bytes(base64.b64decode(table[name]))
print(f"decoded {len(names)} TFMs to {args.out}")

