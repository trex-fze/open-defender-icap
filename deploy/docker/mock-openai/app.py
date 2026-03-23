import json
import os
from http.server import BaseHTTPRequestHandler, HTTPServer


CATEGORY = os.getenv("MOCK_LLM_CATEGORY", "Testing")
SUBCATEGORY = os.getenv("MOCK_LLM_SUBCATEGORY", "Hybrid")
RISK = os.getenv("MOCK_LLM_RISK", "medium")
ACTION = os.getenv("MOCK_LLM_ACTION", "Review")
CONFIDENCE = float(os.getenv("MOCK_LLM_CONFIDENCE", "0.87"))


class MockHandler(BaseHTTPRequestHandler):
    def _write_json(self, data, status=200):
        body = json.dumps(data).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format, *args):  # noqa: A003
        # Reduce noise in docker logs
        return

    def do_GET(self):  # noqa: N802
        if self.path == "/healthz":
            self._write_json({"status": "ok"})
        else:
            self.send_error(404)

    def do_POST(self):  # noqa: N802
        if self.path != "/v1/chat/completions":
            self.send_error(404)
            return

        # Read and ignore payload (but ensure stream consumed)
        length = int(self.headers.get("Content-Length", "0"))
        if length:
            _ = self.rfile.read(length)

        verdict = {
            "primary_category": CATEGORY,
            "subcategory": SUBCATEGORY,
            "risk_level": RISK,
            "confidence": CONFIDENCE,
            "recommended_action": ACTION,
        }
        completion = {
            "id": "mock-llm",
            "object": "chat.completion",
            "choices": [
                {
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": [
                            {
                                "type": "text",
                                "text": json.dumps(verdict),
                            }
                        ],
                    },
                    "finish_reason": "stop",
                }
            ],
        }
        self._write_json(completion)


def main():
    host = "0.0.0.0"
    port = int(os.getenv("PORT", "8080"))
    server = HTTPServer((host, port), MockHandler)
    print(f"mock-openai server listening on {host}:{port}")
    server.serve_forever()


if __name__ == "__main__":
    main()
