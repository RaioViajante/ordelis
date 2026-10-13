"""Regression tests for session daemon readiness (issue #21).

The daemon creates its socket file at bind() and only accepts connections after
listen(). These tests hold the daemon inside that window deterministically and
verify that the test-side readiness helper does not treat the file as readiness.
"""
from __future__ import annotations

import contextlib
import io
import os
import shutil
import socket
import tempfile
import threading
import types
import unittest
from unittest.mock import patch

from runtime.session.client import queue_status, send_prompt, stop_session
from runtime.session.daemon import run_daemon, _socket_path
from runtime.session.state import SessionState, load_state, save_state
from tests.session_helpers import wait_for_daemon_ready

_NAME = "test-ready"
_WAIT = 10.0


def _socket_module_with(socket_class: type) -> types.SimpleNamespace:
    """Stand-in for the `socket` module as seen by runtime.session.daemon."""
    return types.SimpleNamespace(
        socket=socket_class,
        AF_UNIX=socket.AF_UNIX,
        SOCK_STREAM=socket.SOCK_STREAM,
        timeout=socket.timeout,
    )


class _DaemonReadinessTestCase(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.mkdtemp()
        save_state(self.tmp, SessionState(name=_NAME, provider="claude", repo_root=self.tmp))
        self.sock_path = _socket_path(self.tmp, _NAME)
        self.threads: list = []

    def tearDown(self) -> None:
        for t in self.threads:
            t.join(timeout=_WAIT)
            self.assertFalse(t.is_alive(), "thread leaked: {}".format(t))
        shutil.rmtree(self.tmp, ignore_errors=True)

    def _spawn(self, target, *args) -> threading.Thread:
        t = threading.Thread(target=target, args=args, daemon=True)
        t.start()
        self.threads.append(t)
        return t

    def _refused(self) -> bool:
        client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            client.connect(self.sock_path)
        except ConnectionRefusedError:
            return True
        finally:
            client.close()
        return False


class TestBindBeforeListenWindow(_DaemonReadinessTestCase):
    def test_socket_file_is_not_readiness_until_listen(self) -> None:
        bound = threading.Event()
        release = threading.Event()

        class DelayedListenSocket(socket.socket):
            def listen(self, *args):
                bound.set()
                release.wait(_WAIT)
                super().listen(*args)

        with patch("runtime.session.daemon.socket", _socket_module_with(DelayedListenSocket)):
            t = self._spawn(run_daemon, self.tmp, _NAME)
            self.assertTrue(bound.wait(_WAIT), "daemon never reached listen()")

            # The old readiness signal is already true while connections are refused.
            self.assertTrue(os.path.exists(self.sock_path))
            self.assertTrue(self._refused())
            with self.assertRaisesRegex(AssertionError, "not ready within"):
                wait_for_daemon_ready(t, self.tmp, _NAME, timeout=0.3)

            # Readiness is reached only after listen() succeeds.
            release.set()
            wait_for_daemon_ready(t, self.tmp, _NAME, timeout=_WAIT)
            self.assertFalse(self._refused())

            stop_session(self.tmp, _NAME)
            t.join(timeout=_WAIT)
            self.assertEqual(load_state(self.tmp, _NAME).status, "stopped")


class TestReadinessFailures(_DaemonReadinessTestCase):
    def test_listener_never_starts_times_out(self) -> None:
        never = threading.Event()
        t = self._spawn(never.wait, _WAIT)
        try:
            with self.assertRaisesRegex(AssertionError, "not ready within"):
                wait_for_daemon_ready(t, self.tmp, _NAME, timeout=0.2)
        finally:
            never.set()

    def test_bind_error_fails_fast_with_cause(self) -> None:
        class FailingBindSocket(socket.socket):
            def bind(self, *args):
                raise OSError("simulated bind failure")

        with patch("runtime.session.daemon.socket", _socket_module_with(FailingBindSocket)), \
                contextlib.redirect_stderr(io.StringIO()):
            t = self._spawn(run_daemon, self.tmp, _NAME)
            with self.assertRaisesRegex(AssertionError, "exited before becoming ready"):
                wait_for_daemon_ready(t, self.tmp, _NAME, timeout=_WAIT)
        self.assertEqual(load_state(self.tmp, _NAME).status, "crashed")

    def test_daemon_exit_before_readiness_fails_fast(self) -> None:
        # run_daemon returns immediately when the session state is missing.
        os.unlink(os.path.join(os.path.dirname(self.sock_path), "state.json"))
        t = self._spawn(run_daemon, self.tmp, _NAME)
        with self.assertRaisesRegex(AssertionError, "exited before becoming ready"):
            wait_for_daemon_ready(t, self.tmp, _NAME, timeout=_WAIT)


class TestStaleSocketPath(_DaemonReadinessTestCase):
    def _leave_stale_socket(self) -> None:
        stale = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        stale.bind(self.sock_path)
        stale.close()
        self.assertTrue(os.path.exists(self.sock_path))

    def test_stale_file_alone_is_not_readiness(self) -> None:
        self._leave_stale_socket()
        never = threading.Event()
        t = self._spawn(never.wait, _WAIT)
        try:
            with self.assertRaisesRegex(AssertionError, "not ready within"):
                wait_for_daemon_ready(t, self.tmp, _NAME, timeout=0.2)
        finally:
            never.set()

    def test_daemon_replaces_stale_file_and_becomes_ready(self) -> None:
        self._leave_stale_socket()
        t = self._spawn(run_daemon, self.tmp, _NAME)
        wait_for_daemon_ready(t, self.tmp, _NAME, timeout=_WAIT)
        stop_session(self.tmp, _NAME)
        t.join(timeout=_WAIT)


class TestSequentialStartups(_DaemonReadinessTestCase):
    def test_repeated_start_ready_stop_leaves_clean_state(self) -> None:
        for _ in range(5):
            t = self._spawn(run_daemon, self.tmp, _NAME)
            wait_for_daemon_ready(t, self.tmp, _NAME, timeout=_WAIT)

            # The readiness ping must not enqueue or run anything.
            status = queue_status(self.tmp, _NAME)
            self.assertEqual(status["running"], None)
            self.assertEqual(status["queued"], 0)

            # The first real request still gets the first request id and is dispatched once.
            with patch("runtime.session.daemon._dispatch_prompt") as mock_dispatch:
                mock_dispatch.return_value = {"success": True, "response": "ok", "wall_clock_seconds": 0.0}
                result = send_prompt(self.tmp, _NAME, "hello")
            self.assertEqual(result["status"], "ok")
            self.assertEqual(result["request_id"], 1)
            self.assertEqual(mock_dispatch.call_count, 1)

            stop_session(self.tmp, _NAME)
            t.join(timeout=_WAIT)
            self.assertFalse(t.is_alive())
            self.assertFalse(os.path.exists(self.sock_path))
            self.assertEqual(load_state(self.tmp, _NAME).status, "stopped")


if __name__ == "__main__":
    unittest.main()
