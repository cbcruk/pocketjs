#!/usr/bin/env python3
"""Copy files to a Kobo over the network, ending the card-reader round trip.

    tools/kobo-push.py <ip> <dest-dir> <file> [file...]

The device has busybox wget but no ssh and no listening FTP, so this serves the
files from a throwaway HTTP server bound to this machine's address on the
device's subnet and has the device fetch them. Each file is compared by md5 on
both sides before it replaces the one already there, so a half-written transfer
never becomes the launcher the device boots.
"""

import functools
import hashlib
import http.server
import socket
import sys
import tempfile
import threading
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from kobo_telnet import ShellError, run


def local_address_for(host: str) -> str:
    """This machine's address on the route to the device."""
    probe = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    try:
        probe.connect((host, 23))
        return probe.getsockname()[0]
    finally:
        probe.close()


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, fmt: str, *args: object) -> None:
        pass


def serve(directory: Path, address: str) -> http.server.ThreadingHTTPServer:
    # SimpleHTTPRequestHandler takes the root as a constructor argument and
    # overwrites any class attribute of the same name, so bind it here.
    handler = functools.partial(QuietHandler, directory=str(directory))
    server = http.server.ThreadingHTTPServer((address, 0), handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server


def main() -> int:
    if len(sys.argv) < 4:
        sys.stderr.write(__doc__.split("\n\n", 1)[1].split("\n\n")[0] + "\n")
        return 2

    host, dest = sys.argv[1], sys.argv[2].rstrip("/")
    sources = [Path(p) for p in sys.argv[3:]]
    for source in sources:
        if not source.is_file():
            sys.stderr.write(f"kobo-push: not a file: {source}\n")
            return 2

    address = local_address_for(host)
    with tempfile.TemporaryDirectory() as staging:
        stage = Path(staging)
        digests = {}
        for source in sources:
            data = source.read_bytes()
            (stage / source.name).write_bytes(data)
            digests[source.name] = hashlib.md5(data).hexdigest()

        server = serve(stage, address)
        base = f"http://{address}:{server.server_port}"
        try:
            for name, digest in digests.items():
                landing = f"/tmp/kobo-push.{name}"
                # Fetch beside the target, prove it, then move it into place.
                # A truncated download must never be what the device runs.
                command = (
                    f"wget -q -O {landing} {base}/{name} && "
                    f"md5sum {landing} | cut -d' ' -f1"
                )
                try:
                    got = run(host, command, 90.0).strip()
                except ShellError as error:
                    sys.stderr.write(f"kobo-push: {name}: {error}\n")
                    return 1
                if got != digest:
                    sys.stderr.write(
                        f"kobo-push: {name}: md5 {got or '(no output)'} "
                        f"does not match local {digest}; left it in {landing}\n"
                    )
                    return 1
                run(
                    host,
                    f"chmod 755 {landing} && mv -f {landing} {dest}/{name}",
                    60.0,
                )
                print(f"kobo-push: {name} -> {dest}/{name} ({digest[:8]})")
        finally:
            server.shutdown()
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
