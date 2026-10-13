"""Shared synchronization helper for tests that run the session daemon in a thread."""
from __future__ import annotations

import socket
import threading
import time

from runtime.session.daemon import _socket_path

_POLL_INTERVAL_SECONDS = 0.01


def wait_for_daemon_ready(
    thread: threading.Thread,
    repo_root: str,
    name: str,
    timeout: float = 10.0,
) -> None:
    """Block until the daemon answers a ping, or raise with an actionable message.

    The socket file appears at bind(), before listen(), so its existence does not
    mean the daemon accepts connections. A ping is the only readiness signal: it
    is answered by the accept loop and does not touch the request queue or
    session state.
    """
    sock_path = _socket_path(repo_root, name)
    deadline = time.monotonic() + timeout
    last_error = "no connection attempted"
    while True:
        # Check liveness first so a daemon that exited (e.g. bind failure) fails fast.
        # A pong from a dead daemon's stale socket is impossible: nothing is listening.
        alive = thread.is_alive()
        client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        client.settimeout(1.0)
        try:
            client.connect(sock_path)
            client.sendall(b'{"action": "ping"}\n')
            if b"pong" in client.recv(4096):
                return
            last_error = "unexpected ping response"
        except OSError as exc:
            last_error = "{}: {}".format(type(exc).__name__, exc)
        finally:
            client.close()
        if not alive:
            raise AssertionError(
                "Session daemon '{}' exited before becoming ready (last error: {})".format(name, last_error)
            )
        if time.monotonic() >= deadline:
            raise AssertionError(
                "Session daemon '{}' not ready within {}s (last error: {})".format(name, timeout, last_error)
            )
        time.sleep(_POLL_INTERVAL_SECONDS)
