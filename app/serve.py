"""The local generator behind the shell's "generate" button — the keyless
describe->generate->verify->run loop, all on this machine.

    python serve.py [checkpoint] [port]     # defaults: the C5 fine-tune, 8765

POST /generate  {"spec": "...", "k": 4}  ->  {"candidates": ["<applite src>", ...]}

The server only SAMPLES (from the fine-tune trained against the a8
behavioral reward); it verifies nothing. The page holds the verifier — the
applite wasm — and keeps the first candidate that compiles and checks, so
selection happens exactly where the user can see rejections. CORS is open
for localhost pages. Uses the trainer's own Policy/card so the prompt here
is byte-identical to the prompt the model was trained on.
"""

from __future__ import annotations

import json
import os
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
TRAIN = os.path.join(HERE, "..", "experiment", "train")
sys.path.insert(0, TRAIN)
os.environ.setdefault("PYTORCH_CUDA_ALLOC_CONF", "max_split_size_mb:256")

from admission import extract_source  # noqa: E402
from train import Config, Policy, card  # noqa: E402

CKPT = sys.argv[1] if len(sys.argv) > 1 else os.path.join(TRAIN, "checkpoints_apps", "C5")
PORT = int(sys.argv[2]) if len(sys.argv) > 2 else 8765
A8 = os.path.join(HERE, "..", "experiment", "appbench", "target", "release", "a8.exe")

cfg = Config(base_model=CKPT, lang="applite", s5_bin=A8)
policy = Policy(cfg, card(cfg))
print(f"serving {CKPT} on http://localhost:{PORT}", flush=True)


class Handler(BaseHTTPRequestHandler):
    def _send(self, code: int, body: dict) -> None:
        data = json.dumps(body).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Access-Control-Allow-Headers", "Content-Type")
        self.end_headers()
        self.wfile.write(data)

    def do_OPTIONS(self) -> None:  # CORS preflight
        self._send(204, {})

    def do_POST(self) -> None:
        if self.path != "/generate":
            return self._send(404, {"error": "POST /generate"})
        try:
            req = json.loads(self.rfile.read(int(self.headers.get("Content-Length", 0))))
            spec = str(req["spec"])[:500]
            k = min(int(req.get("k", 4)), 8)
        except (ValueError, KeyError) as e:
            return self._send(400, {"error": f"bad request: {e}"})
        completions = policy.sample(f"an app that {spec}", k)
        self._send(200, {"candidates": [extract_source(c) for c in completions]})

    def log_message(self, fmt: str, *args) -> None:
        print(f"[serve] {fmt % args}", flush=True)


if __name__ == "__main__":
    HTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
