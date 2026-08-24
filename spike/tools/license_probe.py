#!/usr/bin/env python3
"""ASU-008b — KWS model agirliklarinin lisans kaynaklarini sorgular."""
import json
import urllib.request

HF = "https://huggingface.co/api/models/"
MS = "https://www.modelscope.cn/api/v1/models/"

HF_IDS = [
    "csukuangfj/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01",
    "pkufool/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01",
    "pkufool/sherpa-onnx-kws-zipformer-wenetspeech-3.3M-2024-01-01",
]
MS_IDS = [
    "pkufool/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01",
    "pkufool/sherpa-onnx-kws-zipformer-zh-en-3M-2025-12-20",
    "csukuangfj/sherpa-onnx-kws-zipformer-zh-en-3M-2025-12-20",
]


def get(url):
    try:
        with urllib.request.urlopen(url, timeout=25) as r:
            return json.load(r)
    except Exception as e:  # noqa: BLE001
        return {"__error__": str(e)}


print("=== HuggingFace ===")
for i in HF_IDS:
    d = get(HF + i)
    if "__error__" in d:
        print(f"{i}: HATA {d['__error__']}")
    else:
        cd = d.get("cardData") or {}
        print(f"{i}: license={cd.get('license')} tags={[t for t in d.get('tags', []) if 'licen' in t]}")

print()
print("=== ModelScope ===")
for i in MS_IDS:
    d = get(MS + i)
    if "__error__" in d:
        print(f"{i}: HATA {d['__error__']}")
        continue
    data = d.get("Data") or {}
    print(
        f"{i}: License={data.get('License')!r} "
        f"LicenseName={data.get('LicenseName')!r} "
        f"LicenseLink={data.get('LicenseLink')!r} "
        f"CreatedBy={data.get('CreatedBy')!r} Downloads={data.get('Downloads')}"
    )
