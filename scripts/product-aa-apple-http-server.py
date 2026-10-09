"""Diagnostic-only static HTTP server for headed macOS product A/A.

The standard hosted macOS Python 3.14 -m http.server CLI can stay alive
without binding a socket. Bind with socketserver.ThreadingTCPServer directly
to avoid HTTPServer.server_bind() and its hostname-resolution path. Preserve
SimpleHTTPRequestHandler's default static-file semantics and concurrency.
This is never shipped with a Noon production package.
"""
from __future__ import annotations

import argparse
import functools
import http.server
import json
from pathlib import Path
import socketserver
import tempfile
import threading
import urllib.error
import urllib.request


class ThreadedStaticServer(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


class ReadinessHandler(http.server.SimpleHTTPRequestHandler):
    ready_token: str = ""

    def do_GET(self):
        if self.path == "/__noon_aa_ready__/" + self.ready_token:
            data = (self.ready_token + "\n").encode("ascii")
            self.send_response(200)
            self.send_header("Content-Type", "text/plain; charset=utf-8")
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Cache-Control", "no-store")
            self.end_headers()
            self.wfile.write(data)
            return
        super().do_GET()


def bind_server(port: int, root: Path, ready_token: str) -> ThreadedStaticServer:
    if not (0 <= port < 65536) or len(ready_token) != 32 or any(
        ch not in "0123456789abcdef" for ch in ready_token
    ):
        raise ValueError("invalid port or random readiness token")

    class SessionHandler(ReadinessHandler):
        pass

    SessionHandler.ready_token = ready_token
    handler = functools.partial(SessionHandler, directory=str(root))
    return ThreadedStaticServer(("127.0.0.1", port), handler)


def self_test() -> None:
    """Real socket/static-file smoke, on the same Python on the Mac runner."""
    token = "a" * 32
    with tempfile.TemporaryDirectory(prefix="noon-aa-http-") as directory:
        root = Path(directory)
        web = root / "web"
        web.mkdir()
        expected = {"buildId": "smoke", "sourceRevision": "smoke"}
        (web / "runtime-build-identity.json").write_text(json.dumps(expected))
        with bind_server(0, root, token) as server:
            port = server.server_address[1]
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                base = "http://127.0.0.1:" + str(port)
                with urllib.request.urlopen(
                    base + "/__noon_aa_ready__/" + token, timeout=4
                ) as response:
                    assert response.status == 200
                    assert response.read() == (token + "\n").encode("ascii")
                with urllib.request.urlopen(
                    base + "/web/runtime-build-identity.json", timeout=4
                ) as response:
                    assert json.load(response) == expected
                try:
                    urllib.request.urlopen(
                        base + "/__noon_aa_ready__/" + ("b" * 32), timeout=4
                    )
                    raise AssertionError("foreign token accepted")
                except urllib.error.HTTPError as error:
                    assert error.code == 404, "wrong-token response was not 404"
            finally:
                server.shutdown()
                thread.join(timeout=5)
    print("NOON_AA_HTTP_SELF_TEST_PASS", flush=True)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int)
    parser.add_argument("--directory", type=Path)
    parser.add_argument("--ready-token")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        return
    if args.port is None or args.directory is None or args.ready_token is None:
        parser.error("--port, --directory, and --ready-token required")
    if not args.directory.is_dir():
        parser.error("static root is not a directory")
    with bind_server(args.port, args.directory, args.ready_token) as server:
        print("NOON_AA_HTTP_BOUND " + str(server.server_address), flush=True)
        server.serve_forever(poll_interval=0.05)


if __name__ == "__main__":
    main()
