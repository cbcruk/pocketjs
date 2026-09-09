#!/usr/bin/env python3
"""Run one command on a Kobo over its debug-services telnet and print the output.

    tools/kobo-sh.py <ip> "<command>" [timeout-seconds]
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from kobo_telnet import ShellError, run


def main() -> int:
    if len(sys.argv) < 3:
        sys.stderr.write(__doc__.split("\n\n", 1)[1])
        return 2
    host, command = sys.argv[1], sys.argv[2]
    timeout = float(sys.argv[3]) if len(sys.argv) > 3 else 30.0
    try:
        print(run(host, command, timeout))
    except ShellError as error:
        sys.stderr.write(f"kobo-sh: {error}\n")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
