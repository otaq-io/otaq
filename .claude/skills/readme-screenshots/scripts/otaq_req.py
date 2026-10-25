#!/usr/bin/env python3
"""Send one request to the demo's background service and print the reply.

    otaq_req.py <method> ['<json params>']
    otaq_req.py theme <key>          # otaq-dark, otaq-light, daylight, tokyo-night, ...

The socket is $OTAQ_HOME/otaq.sock (default /tmp/otaqdemo/home). Exits 1 on an error reply, 2 on a timeout.
Methods worth knowing: project.add {"path","key"}, ticket.move {"key","target","confirmed":true}
(targets: backlog, ready_for_dev, dev, ready_for_review, approved, done),
ticket.merge_proposal {"key"}, ticket.local_merge {"key","message"}, ticket.start_from_scratch {"key","confirm":true},
ticket.pin {"key"}, settings.get, settings.set <whole settings object>.
"""
import json
import os
import socket
import sys

HOME = os.environ.get("OTAQ_HOME", "/tmp/otaqdemo/home")


def call(sock, i, method, params=None):
    req = {"method": method}
    if params is not None:
        req["params"] = params
    sock.sendall((json.dumps({"protocol_version": 1, "id": i, "request": req}) + "\n").encode())
    buf = b""
    while True:
        chunk = sock.recv(65536)
        if not chunk:
            raise ConnectionError("the service closed the connection")
        buf += chunk
        *lines, buf = buf.split(b"\n")
        for line in lines:
            msg = json.loads(line)
            # A malformed request is answered with an error that carries no id.
            if msg.get("id") == i or ("error" in msg and "id" not in msg):
                return msg


def main():
    if len(sys.argv) < 2:
        sys.exit(__doc__)
    sock = socket.socket(socket.AF_UNIX)
    sock.settimeout(20)
    try:
        sock.connect(os.path.join(HOME, "otaq.sock"))
    except OSError as e:
        sys.exit(f"no service at {HOME}/otaq.sock ({e.strerror}); run scripts/start.sh first")
    try:
        if sys.argv[1] == "theme":
            settings = call(sock, 1, "settings.get")["reply"]["global_settings"]["settings"]
            settings["theme"] = sys.argv[2]
            msg = call(sock, 2, "settings.set", settings)
        else:
            params = json.loads(sys.argv[2]) if len(sys.argv) > 2 else None
            msg = call(sock, 1, sys.argv[1], params)
    except socket.timeout:
        print("timeout: no reply within 20 s", file=sys.stderr)
        sys.exit(2)
    print(json.dumps(msg, indent=1)[:4000])
    sys.exit(1 if "error" in msg else 0)


if __name__ == "__main__":
    main()
