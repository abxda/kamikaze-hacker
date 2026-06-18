#!/usr/bin/env python3
"""
Tiny static server for Tower Hacker (WASM).

Why not just open index.html?  Browsers refuse to fetch/stream a .wasm file
over the file:// protocol, so the game must be served over http://.

Usage:
    python serve.py            # serves this folder on http://localhost:8080
    python serve.py 9000       # custom port

Then open the printed URL in your browser. Ctrl+C to stop.
This only serves local files on localhost - no external connections.
"""
import os
import sys
import functools
import http.server
import socketserver

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8080
# Always serve THIS script's folder (where index.html + towerhacker.wasm live),
# no matter what the current working directory is.
ROOT = os.path.dirname(os.path.abspath(__file__))


class Handler(http.server.SimpleHTTPRequestHandler):
    # Make sure .wasm is served with the correct MIME type for streaming compile.
    extensions_map = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        ".wasm": "application/wasm",
        ".js": "text/javascript",
        ".html": "text/html",
    }

    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=ROOT, **kwargs)

    def end_headers(self):
        # Avoid stale wasm during iteration.
        self.send_header("Cache-Control", "no-store")
        super().end_headers()


with socketserver.TCPServer(("127.0.0.1", PORT), Handler) as httpd:
    print(f"Serving folder: {ROOT}")
    print(f"Tower Hacker  ->  http://localhost:{PORT}")
    print("Ctrl+C to stop.")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        print("\nstopped.")
