"""Drive a Kobo's debug-services telnet from a machine with no telnet client.

busybox telnetd insists on option negotiation before it will hand over a login
prompt, which is why `nc` gets nothing but IAC bytes, and Python 3.13 dropped
telnetlib. This refuses every option by hand (DO -> WONT, WILL -> DONT), logs
in as root with the empty password the firmware ships, and runs one command.
"""

import socket
import time

IAC, DONT, DO, WONT, WILL, SB, SE = 255, 254, 253, 252, 251, 250, 240

MARK_BEGIN = "__KOBO_B_7f3a__"
MARK_END = "__KOBO_E_7f3a__"


class ShellError(RuntimeError):
    """The session never reached a prompt, or the command never finished."""


def negotiate(raw: bytes, sock: socket.socket) -> bytes:
    """Strip telnet control sequences, refusing every offered option."""
    out = bytearray()
    index = 0
    while index < len(raw):
        byte = raw[index]
        if byte != IAC:
            out.append(byte)
            index += 1
            continue
        if index + 1 >= len(raw):
            break
        command = raw[index + 1]
        if command in (DO, DONT, WILL, WONT):
            if index + 2 >= len(raw):
                break
            option = raw[index + 2]
            if command == DO:
                sock.sendall(bytes([IAC, WONT, option]))
            elif command == WILL:
                sock.sendall(bytes([IAC, DONT, option]))
            index += 3
        elif command == SB:
            end = raw.find(bytes([IAC, SE]), index)
            index = len(raw) if end < 0 else end + 2
        else:
            index += 2
    return bytes(out)


def run(host: str, command: str, timeout: float = 30.0) -> str:
    """Run one shell command on the device and return what it printed.

    Raises ShellError if the login never completed or the command did not
    finish inside the timeout, with the tail of the session in the message.
    """
    sock = socket.create_connection((host, 23), timeout=10)
    sock.settimeout(1.0)
    seen = ""
    sent_user = sent_pass = sent_cmd = hushed = False
    deadline = time.time() + timeout

    try:
        while time.time() < deadline:
            try:
                chunk = sock.recv(4096)
            except socket.timeout:
                chunk = b""
            if chunk:
                seen += negotiate(chunk, sock).decode("utf-8", "replace")

            tail = seen[-200:]
            if not sent_user and "login:" in tail:
                sock.sendall(b"root\r\n")
                sent_user = True
                time.sleep(0.3)
                continue
            if sent_user and not sent_pass and "assword" in tail:
                sock.sendall(b"\r\n")
                sent_pass = True
                time.sleep(0.3)
                continue
            if sent_user and not hushed and ("#" in tail or "$" in tail):
                # The pty echoes typed input and wraps it at 80 columns, which
                # splits a marker in half and makes it unfindable. Turn the
                # echo off first and each marker then appears exactly once.
                sock.sendall(b"stty -echo\r\n")
                hushed = True
                time.sleep(0.6)
                seen = ""
                continue
            if hushed and not sent_cmd:
                sock.sendall(
                    f"echo {MARK_BEGIN}; {command}; echo {MARK_END}\r\n".encode()
                )
                sent_cmd = True
                time.sleep(0.3)
                continue
            if sent_cmd and MARK_END in seen:
                break
            if not chunk:
                time.sleep(0.2)
    finally:
        sock.close()

    if not sent_cmd:
        raise ShellError(f"never reached a shell prompt: {seen[-800:]!r}")
    if MARK_BEGIN not in seen or MARK_END not in seen:
        raise ShellError(f"command did not finish in {timeout}s: {seen[-800:]!r}")

    body = seen.split(MARK_BEGIN, 1)[1].split(MARK_END, 1)[0]
    return body.replace("\r\n", "\n").strip()
