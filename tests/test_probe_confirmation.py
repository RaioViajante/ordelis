from __future__ import annotations

import inspect
import json
import os
import pty
import re
import select
import sqlite3
import subprocess
import sys
import threading
import time
import unittest
from dataclasses import asdict, replace
from pathlib import Path
from unittest.mock import patch

from runtime import probe_confirmation as confirmation
from runtime import codex_probe_runner as runner
from runtime import probe_authorization_ledger as ledger
from runtime.codex_probe_runner import ALLOWED_COMMANDS, ProbeAuthorization, ProbeError
from runtime.probe_confirmation import (
    _ConfirmedProbe as ConfirmedProbe,
    _confirm_probe as confirm_probe,
    _run_confirmed_probe as run_confirmed_probe,
    run_controlled_probe,
)
from runtime.synapse_plan import PlanError
from tests.test_codex_probe_runner import RunnerCase, run_codex_probe

ROOT = Path(__file__).resolve().parent.parent
PROMPT_END = b"\n> "
CHILD = r'''
import fcntl, json, os, signal, sys, termios
signal.signal(signal.SIGINT, signal.default_int_handler)  # a shell may start us with SIGINT ignored
sys.path.insert(0, sys.argv[1])
mode, spec_json, repo, out = sys.argv[2:6]
if mode != "no_tty":
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)
from pathlib import Path
from runtime import probe_confirmation as pc
from runtime.codex_probe_runner import ProbeAuthorization, ProbeError, _preflight
spec = json.loads(spec_json)
spec["protected_roots"] = tuple(spec["protected_roots"])
auth = ProbeAuthorization(**spec)
if mode == "tty_background":
    os.tcgetpgrp = lambda fd: os.getpgrp() + 1
if mode == "tty_stdin_consent":
    checkout, protected, *_ = _preflight(auth, Path(repo))
    read_end, write_end = os.pipe()
    os.write(write_end, (pc._phrase(pc._digest(auth, checkout, protected)) + "\n").encode())
    os.dup2(read_end, 0)
try:
    pc._confirm_probe(auth, repo_root=Path(repo))
    result = "approved"
except ProbeError as exc:
    result = str(exc)
open(out, "w").write(result)
'''

CONTROLLED_CHILD = r'''
import json, os, re, sys
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0, sys.argv[1])
from runtime import codex_probe_runner as runner
from runtime import probe_confirmation as pc
from runtime.codex_probe_runner import ProbeAuthorization, ProbeError

mode, spec_json, repo = sys.argv[2:5]
spec = json.loads(spec_json)
spec["protected_roots"] = tuple(spec["protected_roots"])
auth = ProbeAuthorization(**spec)
def approve(prompt):
    return re.search(r"type:  (authorize [0-9a-f]{12})\n", prompt).group(1)
try:
    with patch.object(pc, "_ask", approve):
        if mode == "crash_after_consumption":
            with patch.object(runner, "_create_scratch", side_effect=lambda *a: os._exit(91)):
                pc.run_controlled_probe(auth, repo_root=Path(repo))
        else:
            pc.run_controlled_probe(auth, repo_root=Path(repo))
    print("ran")
except ProbeError as exc:
    print(str(exc))
'''


def phrase_in(prompt: str) -> bytes:
    found = re.search(r"type:  (authorize [0-9a-f]{12})\n", prompt)
    assert found, prompt
    return found.group(1).encode() + b"\n"


class ConfirmationCase(RunnerCase):
    def setUp(self) -> None:
        super().setUp()
        self.write_fake()
        confirmation._ISSUED.clear()
        self.addCleanup(confirmation._ISSUED.clear)
        self.prompts: list[str] = []
        self.opened = 0

    def confirm(self, reply, authorization: ProbeAuthorization | None = None) -> ConfirmedProbe:
        """Run confirm_probe against a pty pair standing in for /dev/tty; reply(prompt) -> bytes | None."""
        master, slave = pty.openpty()
        closed = threading.Event()

        def drive() -> None:
            data = b""
            end = time.monotonic() + 10
            while PROMPT_END not in data and time.monotonic() < end:
                if select.select([master], [], [], 0.1)[0]:
                    try:
                        data += os.read(master, 4096)
                    except OSError:
                        return
            text = data.decode("utf-8", "replace").replace("\r\n", "\n")
            self.prompts.append(text)
            try:
                answer = reply(text)
            except BaseException:
                answer = None  # never leave confirm_probe blocked on a read
                raise
            finally:
                if answer is None:
                    closed.set()
                    os.close(master)
            if answer is not None:
                os.write(master, answer)

        def opener() -> int:
            self.opened += 1
            threading.Thread(target=drive, daemon=True).start()
            return os.dup(slave)

        try:
            with patch.object(confirmation, "_open_terminal", opener):
                return confirm_probe(authorization or self.authorization(), repo_root=self.repo)
        finally:
            os.close(slave)
            if not closed.is_set():
                os.close(master)

    def approve(self, authorization: ProbeAuthorization | None = None) -> ConfirmedProbe:
        return self.confirm(lambda text: phrase_in(text), authorization)

    def assertNoExecution(self) -> None:
        self.assertEqual(self.argv_log(), [])
        self.assertEqual(list(self.parent.iterdir()), [])

    def assertDenied(self, reason: str, reply, authorization: ProbeAuthorization | None = None) -> None:
        with self.assertRaises(ProbeError) as caught:
            self.confirm(reply, authorization)
        self.assertEqual(str(caught.exception), reason)
        self.assertNoExecution()


class ApprovalTests(ConfirmationCase):
    def test_explicit_approval_runs_the_three_allowlisted_commands_once(self) -> None:
        confirmed = self.approve()
        self.assertNoExecution()  # confirming alone executes nothing
        result = run_confirmed_probe(confirmed)
        self.assertEqual(self.argv_log(), [list(args) for args in ALLOWED_COMMANDS])
        self.assertEqual(result["authorization"]["origin"], "terminal_confirmed_private_ledger")
        self.assertIs(result["evidence_authenticated"], False)
        self.assertIs(result["dispatch_authorized"], False)

    def test_private_runner_result_keeps_the_confirmed_origin(self) -> None:
        result = run_codex_probe(self.authorization(), repo_root=self.repo)
        self.assertEqual(result["authorization"]["origin"], "terminal_confirmed_private_ledger")

    def test_typeahead_before_the_prompt_is_discarded(self) -> None:
        master, slave = pty.openpty()
        os.write(master, b"n\n")  # typed before the prompt appeared; must not be read as the answer

        def answer_after_prompt() -> None:
            data = b""
            while PROMPT_END not in data:
                if select.select([master], [], [], 0.1)[0]:
                    data += os.read(master, 4096)
            os.write(master, phrase_in(data.decode("utf-8", "replace").replace("\r\n", "\n")))

        def opener() -> int:
            threading.Thread(target=answer_after_prompt, daemon=True).start()
            return os.dup(slave)

        try:
            with patch.object(confirmation, "_open_terminal", opener):
                confirmed = confirm_probe(self.authorization(), repo_root=self.repo)
        finally:
            os.close(slave)
            os.close(master)
        self.assertIsInstance(confirmed, ConfirmedProbe)


    def test_leftover_typed_lines_are_discarded_after_the_answer(self) -> None:
        master, slave = pty.openpty()

        def answer_with_extra_line() -> None:
            data = b""
            while PROMPT_END not in data:
                if select.select([master], [], [], 0.1)[0]:
                    data += os.read(master, 4096)
            os.write(master, phrase_in(data.decode("utf-8", "replace").replace("\r\n", "\n")) + b"leftover\n")

        def opener() -> int:
            threading.Thread(target=answer_with_extra_line, daemon=True).start()
            return os.dup(slave)

        try:
            with patch.object(confirmation, "_open_terminal", opener):
                confirm_probe(self.authorization(), repo_root=self.repo)
            self.assertEqual(select.select([slave], [], [], 0.3)[0], [])  # nothing left for the shell
        finally:
            os.close(slave)
            os.close(master)


class DenialTests(ConfirmationCase):
    def test_declining_answers_deny(self) -> None:
        for answer in (b"n\n", b"no\n", b"NO\n"):
            with self.subTest(answer=answer):
                self.assertDenied("declined", lambda text, a=answer: a)

    def test_ambiguous_or_invalid_answers_deny(self) -> None:
        wrong = [b"yes\n", b"y\n", b"\n", b"authorize\n", b"authorize 000000000000\n", b"ok\n",
                 b"\xff\xfe\n", b"x" * 400 + b"\n"]
        for answer in wrong:
            with self.subTest(answer=answer[:20]):
                self.assertDenied("not_confirmed", lambda text, a=answer: a)

    def test_phrase_must_match_exactly(self) -> None:
        variants = {
            "uppercase": lambda text: phrase_in(text).upper(),
            "trailing space": lambda text: phrase_in(text).rstrip(b"\n") + b" \n",
            "leading space": lambda text: b" " + phrase_in(text),
            "short digest": lambda text: phrase_in(text)[:-6] + b"\n",
        }
        for name, build in variants.items():
            with self.subTest(name=name):
                self.assertDenied("not_confirmed", build)

    def test_end_of_input_denies(self) -> None:
        with self.assertRaises(ProbeError) as caught:
            self.confirm(lambda text: None)
        self.assertIn(str(caught.exception), {"eof", "terminal_io_failed"})
        self.assertNoExecution()

    def test_partial_line_without_newline_denies(self) -> None:
        # The correct phrase, but ended with EOF (^D, twice to flush then signal EOF) instead of a newline.
        self.assertDenied("not_confirmed", lambda text: phrase_in(text)[:-1] + b"\x04\x04")

    def test_terminal_write_failure_denies(self) -> None:
        with patch.object(confirmation, "_open_terminal", lambda: os.open(os.devnull, os.O_RDONLY)):
            with self.assertRaises(ProbeError) as caught:
                confirm_probe(self.authorization(), repo_root=self.repo)
        self.assertEqual(str(caught.exception), "terminal_io_failed")
        self.assertNoExecution()

    def test_terminal_read_failure_denies(self) -> None:
        with patch.object(confirmation, "_open_terminal", lambda: os.open(os.devnull, os.O_WRONLY)):
            with self.assertRaises(ProbeError) as caught:
                confirm_probe(self.authorization(), repo_root=self.repo)
        self.assertEqual(str(caught.exception), "terminal_io_failed")
        self.assertNoExecution()

    def test_unsupported_platform_denies(self) -> None:
        with patch.object(confirmation.os, "name", "nt"):
            with self.assertRaises(ProbeError) as caught:
                confirm_probe(self.authorization(), repo_root=self.repo)
        self.assertEqual(str(caught.exception), "unsupported_platform")


class RealTerminalTests(ConfirmationCase):
    """Run confirm_probe in a child process so /dev/tty semantics are real, not patched."""

    def spawn(self, mode: str, **kwargs) -> tuple[subprocess.Popen, Path, int | None]:
        out = self.side / f"result-{mode}.txt"
        spec = asdict(self.authorization())
        master = slave = None
        if mode != "no_tty":
            master, slave = pty.openpty()
            kwargs.update(stdin=slave, stdout=slave, stderr=slave)
        process = subprocess.Popen(
            [sys.executable, "-c", CHILD, str(ROOT), mode, json.dumps(spec), str(self.repo), str(out)],
            start_new_session=True, **kwargs)
        self.addCleanup(process.kill)
        if slave is not None:
            os.close(slave)
            self.addCleanup(lambda: os.close(master) if self._fd_open(master) else None)
        return process, out, master

    @staticmethod
    def _fd_open(fd: int) -> bool:
        try:
            os.fstat(fd)
        except OSError:
            return False
        return True

    def wait_for_prompt(self, master: int) -> str:
        data = b""
        end = time.monotonic() + 15
        while PROMPT_END not in data and time.monotonic() < end:
            if select.select([master], [], [], 0.1)[0]:
                data += os.read(master, 4096)
        self.assertIn(PROMPT_END, data, data)
        return data.decode("utf-8", "replace").replace("\r\n", "\n")

    def finish(self, process: subprocess.Popen, out: Path, master: int | None = None) -> str:
        # Keep draining the terminal: a pty slave cannot close while its echo is unread.
        end = time.monotonic() + 15
        while process.poll() is None and time.monotonic() < end and master is not None:
            if select.select([master], [], [], 0.1)[0]:
                try:
                    os.read(master, 4096)
                except OSError:
                    break
        process.wait(timeout=5)
        return out.read_text()

    def test_real_terminal_approval_is_accepted(self) -> None:
        process, out, master = self.spawn("tty")
        os.write(master, phrase_in(self.wait_for_prompt(master)))
        self.assertEqual(self.finish(process, out, master), "approved")

    def test_missing_controlling_terminal_denies_even_with_piped_input_and_env_flags(self) -> None:
        env = {**os.environ, "SYNAPSE_PROBE_AUTHORIZED": "1", "SYNAPSE_CONFIRM": "yes", "CI": "1"}
        process, out, _ = self.spawn("no_tty", stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.PIPE, env=env)
        process.communicate(b"authorize 000000000000\nyes\ny\n", timeout=15)
        self.assertEqual(out.read_text(), "no_controlling_terminal")
        self.assertNoExecution()

    def test_consent_piped_through_stdin_is_ignored_when_a_terminal_exists(self) -> None:
        process, out, master = self.spawn("tty_stdin_consent")
        self.wait_for_prompt(master)
        os.write(master, b"\x04")  # the terminal user says nothing: EOF
        self.assertIn(self.finish(process, out, master), {"eof", "terminal_io_failed"})

    def test_interrupt_denies(self) -> None:
        process, out, master = self.spawn("tty")
        self.wait_for_prompt(master)
        os.write(master, b"\x03")
        self.assertEqual(self.finish(process, out, master), "interrupted")

    def test_background_process_group_denies(self) -> None:
        process, out, master = self.spawn("tty_background")
        self.assertEqual(self.finish(process, out, master), "terminal_not_foreground")


class ProvenanceTests(ConfirmationCase):
    def test_replayed_confirmation_is_refused(self) -> None:
        confirmed = self.approve()
        run_confirmed_probe(confirmed)
        calls = len(self.argv_log())
        with self.assertRaises(ProbeError) as caught:
            run_confirmed_probe(confirmed)
        self.assertEqual(str(caught.exception), "confirmation_invalid_or_reused")
        self.assertEqual(len(self.argv_log()), calls)

    def test_hand_built_or_wrong_type_confirmations_are_refused(self) -> None:
        auth = self.authorization()
        for forged in (ConfirmedProbe(auth, "0" * 32), ConfirmedProbe(auth, ""), auth, None,
                       {"authorization": auth}, ConfirmedProbe(auth, 5)):
            with self.subTest(forged=type(forged).__name__):
                with self.assertRaises(ProbeError):
                    run_confirmed_probe(forged)
        self.assertNoExecution()

    def test_stateful_confirmation_subclass_is_rejected_before_attribute_access(self) -> None:
        class StatefulConfirmation(ConfirmedProbe):
            armed = False
            reads = 0

            def __getattribute__(self, name: str):
                if type(self).armed and name == "nonce":
                    type(self).reads += 1
                return super().__getattribute__(name)

        confirmed = self.approve()
        stateful = StatefulConfirmation(confirmed.authorization, confirmed.nonce)
        StatefulConfirmation.armed = True
        with self.assertRaisesRegex(ProbeError, "confirmation_missing"):
            run_confirmed_probe(stateful)
        self.assertEqual(StatefulConfirmation.reads, 0)
        self.assertNoExecution()

    def test_wrong_registered_confirmation_digest_is_refused(self) -> None:
        confirmed = self.approve()
        digest, checkout, expiry = confirmation._ISSUED[confirmed.nonce]
        confirmation._ISSUED[confirmed.nonce] = ("0" * len(digest), checkout, expiry)
        with self.assertRaisesRegex(ProbeError, "specification_changed_after_confirmation"):
            run_confirmed_probe(confirmed)
        self.assertNoExecution()

    def test_authorization_swapped_into_a_real_confirmation_is_refused(self) -> None:
        confirmed = self.approve()
        other = self.authorization()
        with self.assertRaises(ProbeError) as caught:
            run_confirmed_probe(ConfirmedProbe(other, confirmed.nonce))
        self.assertEqual(str(caught.exception), "specification_changed_after_confirmation")
        self.assertNoExecution()

    def test_changed_specification_after_approval_is_refused_and_consumes_the_confirmation(self) -> None:
        confirmed = self.approve()
        changes = [{"max_output_bytes": 8192}, {"command_timeout_seconds": 6}, {"total_timeout_seconds": 21},
                   {"probe_id": "other-id"}, {"scratch_parent": str(self.root)}]
        for change in changes:
            with self.subTest(change=change):
                confirmed = self.approve()
                with self.assertRaises(ProbeError) as caught:
                    run_confirmed_probe(ConfirmedProbe(replace(confirmed.authorization, **change), confirmed.nonce))
                self.assertEqual(str(caught.exception), "specification_changed_after_confirmation")
                with self.assertRaises(ProbeError):
                    run_confirmed_probe(confirmed)  # the nonce is spent
        self.assertNoExecution()

    def test_mutating_the_frozen_authorization_in_place_is_refused(self) -> None:
        confirmed = self.approve()
        object.__setattr__(confirmed.authorization, "max_output_bytes", 9999)
        with self.assertRaises(ProbeError) as caught:
            run_confirmed_probe(confirmed)
        self.assertEqual(str(caught.exception), "specification_changed_after_confirmation")
        self.assertNoExecution()

    def test_protected_root_retargeted_after_approval_is_refused(self) -> None:
        link = self.root / "vault-link"
        link.symlink_to(self.vault)
        confirmed = self.approve(self.authorization(protected_roots=(str(link),)))
        link.unlink()
        other = self.root / "other-vault"
        other.mkdir()
        link.symlink_to(other)
        with self.assertRaises(ProbeError) as caught:
            run_confirmed_probe(confirmed)
        self.assertEqual(str(caught.exception), "specification_changed_after_confirmation")
        self.assertNoExecution()

    def test_executable_changed_after_approval_never_runs(self) -> None:
        confirmed = self.approve()
        self.fake.write_text(self.fake.read_text() + "# tampered\n")
        with self.assertRaises(ProbeError) as caught:
            run_confirmed_probe(confirmed)
        self.assertEqual(str(caught.exception), "executable_hash_mismatch")
        self.assertNoExecution()

    def test_probe_id_cannot_be_reused_across_confirmations(self) -> None:
        auth = self.authorization()
        run_confirmed_probe(self.approve(auth))
        with self.assertRaises(ProbeError) as caught:
            run_confirmed_probe(self.approve(auth))
        self.assertEqual(str(caught.exception), "authorization_reused")

    def test_confirmation_registry_is_bounded(self) -> None:
        with patch.object(confirmation, "MAX_PENDING", 1):
            self.approve()
            with self.assertRaises(ProbeError) as caught:
                self.approve()
        self.assertEqual(str(caught.exception), "confirmation_registry_full")

    def test_expired_confirmations_are_pruned_from_the_registry(self) -> None:
        confirmation._ISSUED.clear()
        self.approve()
        for nonce, (digest, checkout, _) in list(confirmation._ISSUED.items()):
            confirmation._ISSUED[nonce] = (digest, checkout, time.time() - 1)
        self.approve()
        self.assertEqual(len(confirmation._ISSUED), 1)


class ReviewRegressionTests(ConfirmationCase):
    def test_concurrent_confirmations_never_share_the_terminal(self) -> None:
        active = peak = 0
        guard = threading.Lock()
        pairs: list[tuple[int, int]] = []

        def drive(master: int) -> None:
            nonlocal active, peak
            data = b""
            while PROMPT_END not in data:
                if select.select([master], [], [], 0.1)[0]:
                    data += os.read(master, 4096)
            with guard:
                active += 1
                peak = max(peak, active)
            time.sleep(0.3)
            with guard:
                active -= 1
            os.write(master, phrase_in(data.decode("utf-8", "replace").replace("\r\n", "\n")))

        def opener() -> int:  # patched once: each call gets its own pty pair
            master, slave = pty.openpty()
            pairs.append((master, slave))
            threading.Thread(target=drive, args=(master,), daemon=True).start()
            return os.dup(slave)

        results: list[object] = []
        auths = [self.authorization() for _ in range(3)]

        def attempt(auth: ProbeAuthorization) -> None:
            try:
                results.append(confirm_probe(auth, repo_root=self.repo))
            except BaseException as exc:  # noqa: BLE001 - recorded for the assertion below
                results.append(exc)

        with patch.object(confirmation, "_open_terminal", opener):
            threads = [threading.Thread(target=attempt, args=(auth,)) for auth in auths]
            for thread in threads:
                thread.start()
            for thread in threads:
                thread.join(30)
        for master, slave in pairs:
            os.close(master)
            os.close(slave)
        self.assertEqual(peak, 1)  # prompts are serialized, one at a time
        self.assertEqual(len(results), 3)
        self.assertTrue(all(isinstance(item, ConfirmedProbe) for item in results), results)

    def test_already_consumed_probe_id_is_refused_before_prompting(self) -> None:
        auth = self.authorization()
        run_codex_probe(auth, repo_root=self.repo)
        opened = self.opened
        with self.assertRaises(ProbeError) as caught:
            self.confirm(lambda text: phrase_in(text), auth)
        self.assertEqual(str(caught.exception), "authorization_reused")
        self.assertEqual(self.opened, opened)  # the user was never asked to approve a doomed run

    def test_terminal_not_in_line_mode_denies(self) -> None:
        import termios

        master, slave = pty.openpty()
        attrs = termios.tcgetattr(slave)
        attrs[3] &= ~termios.ICANON  # e.g. left raw by another program: Enter sends \r, no line ever ends
        termios.tcsetattr(slave, termios.TCSANOW, attrs)
        try:
            with patch.object(confirmation, "_open_terminal", lambda: os.dup(slave)):
                with self.assertRaises(ProbeError) as caught:
                    confirm_probe(self.authorization(), repo_root=self.repo)
        finally:
            os.close(slave)
            os.close(master)
        self.assertEqual(str(caught.exception), "terminal_not_line_mode")
        self.assertNoExecution()


class ExpiryAndConflictTests(ConfirmationCase):
    def test_expired_authorization_is_refused_before_any_prompt(self) -> None:
        now = time.time()
        auth = self.authorization(issued_at=now - 10, expires_at=now - 1)
        with self.assertRaises(ProbeError) as caught:
            self.confirm(lambda text: phrase_in(text), auth)
        self.assertEqual(str(caught.exception), "authorization_expired")
        self.assertEqual(self.opened, 0)
        self.assertNoExecution()

    def test_authorization_that_expires_while_the_prompt_is_open_is_refused(self) -> None:
        now = time.time()
        auth = self.authorization(issued_at=now - 1, expires_at=now + 1.5)

        def slow(text: str) -> bytes:
            time.sleep(2)
            return phrase_in(text)

        self.assertDenied("authorization_expired", slow, auth)

    def test_confirmation_that_expires_before_the_run_is_refused(self) -> None:
        now = time.time()
        confirmed = self.approve(self.authorization(issued_at=now - 1, expires_at=now + 1.2))
        time.sleep(1.5)
        with self.assertRaises(ProbeError) as caught:
            run_confirmed_probe(confirmed)
        self.assertEqual(str(caught.exception), "authorization_expired")
        self.assertNoExecution()

    def test_not_yet_valid_authorization_is_refused_before_any_prompt(self) -> None:
        now = time.time()
        auth = self.authorization(issued_at=now + 60, expires_at=now + 120)
        with self.assertRaises(ProbeError) as caught:
            self.confirm(lambda text: phrase_in(text), auth)
        self.assertEqual(str(caught.exception), "authorization_not_yet_valid")
        self.assertEqual(self.opened, 0)

    def test_changed_executable_hash_is_refused_before_any_prompt(self) -> None:
        auth = self.authorization(executable_sha256="0" * 64)
        with self.assertRaises(ProbeError) as caught:
            self.confirm(lambda text: phrase_in(text), auth)
        self.assertEqual(str(caught.exception), "executable_hash_mismatch")
        self.assertEqual(self.opened, 0)
        self.assertNoExecution()

    def test_executable_replaced_while_the_prompt_is_open_is_refused(self) -> None:
        def tamper_then_approve(text: str) -> bytes:
            self.fake.write_text(self.fake.read_text() + "# tampered\n")
            return phrase_in(text)

        self.assertDenied("executable_hash_mismatch", tamper_then_approve)

    def test_conflicting_fields_are_refused_before_any_prompt(self) -> None:
        now = time.time()
        with self.assertRaises(PlanError):
            self.authorization(issued_at=now, expires_at=now - 5)
        for name, overrides in {
                "executable inside protected root": {"protected_roots": (str(self.bin),)},
                "scratch parent inside protected root": {"protected_roots": (str(self.root),)},
                "scratch parent inside checkout": {"scratch_parent": str(self.repo)},
                "relative executable": {"executable_path": "codex"},
        }.items():
            with self.subTest(name=name):
                try:
                    auth = self.authorization(**overrides)
                except PlanError:
                    continue
                with self.assertRaises(PlanError):
                    self.confirm(lambda text: phrase_in(text), auth)
        self.assertEqual(self.opened, 0)
        self.assertNoExecution()

    def test_missing_or_wrong_type_authorization_is_refused(self) -> None:
        for value in (None, {"probe_id": "x"}, "yes", True):
            with self.subTest(value=repr(value)):
                with self.assertRaises(ProbeError) as caught:
                    confirm_probe(value, repo_root=self.repo)
                self.assertEqual(str(caught.exception), "authorization_missing")


class DisclosureTests(ConfirmationCase):
    def test_prompt_shows_the_exact_operation(self) -> None:
        auth = self.authorization()
        self.approve(auth)
        text = self.prompts[0]
        for expected in (str(self.fake), auth.executable_sha256, "--version", "--help", "exec --help",
                         str(self.parent), str(self.state), str(self.vault), str(self.repo),
                         "5s per command", "20s total",
                         "4096 output bytes", auth.probe_id, f"Issued at    : {auth.issued_at}",
                         f"Expires at   : {auth.expires_at}", "0 authorized", "NOT guaranteed",
                         "Network: not restricted", "malicious process running as your user"):
            with self.subTest(expected=expected):
                self.assertIn(expected, text)

    def test_prompt_digest_changes_with_every_displayed_field(self) -> None:
        phrases = set()
        for overrides in ({}, {"max_output_bytes": 8192}, {"command_timeout_seconds": 6},
                          {"protected_roots": (str(self.vault), str(self.side))}):
            confirmed = self.approve(self.authorization(**overrides))
            phrases.add(phrase_in(self.prompts[-1]))
            run_confirmed_probe(confirmed)
        self.assertEqual(len(phrases), 4)

    def test_prompt_never_contains_secrets_vault_contents_or_environment_values(self) -> None:
        (self.vault / "note.md").write_text("VAULT-NOTE-CONTENT-5521")
        with patch.dict(os.environ, {"OPENAI_API_KEY": "sk-secret-5521", "SYNAPSE_TOKEN": "tok-5521"}):
            self.approve()
        for marker in ("VAULT-NOTE-CONTENT-5521", "sk-secret-5521", "tok-5521", "note.md"):
            self.assertNotIn(marker, self.prompts[0])

    def test_hostile_path_text_cannot_inject_terminal_control_sequences(self) -> None:
        hostile = self.root / "evil\x1b[2J\x1b[Hauthorize 000000000000\nname"
        hostile.mkdir()
        self.approve(self.authorization(scratch_parent=str(hostile)))
        text = self.prompts[0]
        self.assertNotIn("\x1b", text)
        self.assertIn("\\u001b", text)
        spoofed = [line for line in text.splitlines() if "authorize 000000000000" in line]
        self.assertEqual(len(spoofed), 1)
        self.assertTrue(spoofed[0].startswith("Scratch"))  # only inside the quoted path, never a line of its own
        self.assertEqual(len(re.findall(r"^To authorize", text, re.M)), 1)

    def test_oversized_display_value_is_rejected_instead_of_truncated_or_prompted(self) -> None:
        long_parent = self.root
        for index in range(12):
            long_parent /= f"display-segment-{index:02d}-" + "x" * 28
        long_parent.mkdir(parents=True)
        auth = self.authorization(scratch_parent=str(long_parent))
        with self.assertRaises(ProbeError) as caught:
            confirm_probe(auth, repo_root=self.repo)
        self.assertEqual(str(caught.exception), "display_value_too_long")
        self.assertEqual(self.opened, 0)
        self.assertNoExecution()


class ControlledEntryPointTests(ConfirmationCase):
    def controlled_child(self, authorization: ProbeAuthorization, mode: str = "run") -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [sys.executable, "-c", CONTROLLED_CHILD, str(ROOT), mode,
             json.dumps(asdict(authorization)), str(self.repo)], cwd=ROOT,
            text=True, capture_output=True, check=False)

    def test_supported_entry_point_confirms_and_runs_the_synthetic_probe(self) -> None:
        def approve(text: str) -> str:
            self.prompts.append(text)
            return phrase_in(text).decode().strip()

        with patch.object(confirmation, "_ask", approve):
            result = confirmation.run_controlled_probe(self.authorization(), repo_root=self.repo)
        self.assertEqual(self.argv_log(), [list(args) for args in ALLOWED_COMMANDS])
        self.assertEqual(result["authorization"]["origin"], "terminal_confirmed_private_ledger")
        self.assertIs(result["evidence_authenticated"], False)
        self.assertIs(result["dispatch_authorized"], False)

    def test_supported_entry_point_denies_without_exact_confirmation(self) -> None:
        with patch.object(confirmation, "_ask", return_value="no"):
            with self.assertRaisesRegex(ProbeError, "declined"):
                confirmation.run_controlled_probe(self.authorization(), repo_root=self.repo)
        self.assertNoExecution()

    def test_supported_entry_point_defaults_to_deny_without_authorization(self) -> None:
        with self.assertRaisesRegex(ProbeError, "authorization_missing"):
            run_controlled_probe(None, repo_root=self.repo)  # type: ignore[arg-type]
        self.assertNoExecution()

    def test_stateful_authorization_subclass_is_rejected_before_attribute_access(self) -> None:
        class StatefulAuthorization(ProbeAuthorization):
            armed = False
            reads = 0

            def __getattribute__(self, name: str):
                if type(self).armed and name == "expires_at":
                    type(self).reads += 1
                return super().__getattribute__(name)

        auth = StatefulAuthorization(**asdict(self.authorization()))
        StatefulAuthorization.armed = True
        with self.assertRaisesRegex(ProbeError, "authorization_missing"):
            confirmation.run_controlled_probe(auth, repo_root=self.repo)
        self.assertEqual(StatefulAuthorization.reads, 0)
        self.assertNoExecution()

    def test_concurrent_reuse_of_one_confirmation_runs_at_most_once(self) -> None:
        confirmed = self.approve()
        barrier = threading.Barrier(3)
        results: list[object] = []

        def attempt() -> None:
            barrier.wait()
            try:
                results.append(run_confirmed_probe(confirmed))
            except BaseException as exc:  # noqa: BLE001 - asserted below
                results.append(exc)

        threads = [threading.Thread(target=attempt) for _ in range(2)]
        for thread in threads:
            thread.start()
        barrier.wait()
        for thread in threads:
            thread.join(30)
        self.assertEqual(len(self.argv_log()), 3)
        self.assertEqual(sum(isinstance(item, dict) for item in results), 1)
        self.assertEqual(sum(isinstance(item, ProbeError) for item in results), 1)

    def test_two_processes_using_the_controlled_entry_point_have_one_winner(self) -> None:
        auth = self.authorization()
        argv = [sys.executable, "-c", CONTROLLED_CHILD, str(ROOT), "run",
                json.dumps(asdict(auth)), str(self.repo)]
        processes = [subprocess.Popen(argv, cwd=ROOT, text=True, stdout=subprocess.PIPE,
                                      stderr=subprocess.PIPE) for _ in range(2)]
        results = [process.communicate(timeout=20) for process in processes]
        self.assertEqual([process.returncode for process in processes], [0, 0], results)
        self.assertEqual(sorted(stdout.strip() for stdout, _ in results), ["authorization_reused", "ran"])
        self.assertEqual(self.argv_log(), [list(args) for args in ALLOWED_COMMANDS])

    def test_crash_after_consumption_leaves_the_authorization_spent(self) -> None:
        auth = self.authorization()
        crashed = self.controlled_child(auth, "crash_after_consumption")
        self.assertEqual(crashed.returncode, 91, crashed.stderr)
        refused = self.controlled_child(auth)
        self.assertEqual(refused.returncode, 0, refused.stderr)
        self.assertEqual(refused.stdout.strip(), "authorization_reused")
        self.assertNoExecution()

    def test_atomic_commit_failure_never_creates_scratch_or_launches(self) -> None:
        def approve(text: str) -> str:
            return phrase_in(text).decode().strip()

        with patch.object(confirmation, "_ask", approve), \
                patch.object(ledger, "_commit", side_effect=sqlite3.OperationalError("synthetic")):
            with self.assertRaisesRegex(ProbeError, "authorization_ledger_unavailable"):
                run_controlled_probe(self.authorization(), repo_root=self.repo)
        self.assertNoExecution()

    def test_missing_or_unsafe_state_root_never_prompts_or_launches(self) -> None:
        missing = self.authorization(state_root=str(self.root / "missing-state"))
        with self.assertRaisesRegex(ProbeError, "state_root"):
            run_controlled_probe(missing, repo_root=self.repo)
        self.state.chmod(0o755)
        with self.assertRaisesRegex(ProbeError, "state_root_not_private"):
            run_controlled_probe(self.authorization(), repo_root=self.repo)
        self.assertEqual(self.opened, 0)
        self.assertNoExecution()


class NoBypassTests(ConfirmationCase):
    def test_raw_runner_is_not_a_supported_public_api(self) -> None:
        self.assertFalse(hasattr(runner, "run_codex_probe"))
        self.assertEqual(runner.__all__, ["ProbeAuthorization", "ProbeError"])
        self.assertEqual(confirmation.__all__, ["run_controlled_probe"])

    def test_public_cli_does_not_expose_the_controlled_probe(self) -> None:
        source = (ROOT / "runtime" / "cli.py").read_text(encoding="utf-8")
        self.assertNotIn("run_controlled_probe", source)
        self.assertNotIn("probe_confirmation", source)
        self.assertNotIn("probe_authorization_ledger", source)

    def test_public_api_has_no_parameter_that_can_carry_consent(self) -> None:
        self.assertEqual(list(inspect.signature(run_controlled_probe).parameters),
                         ["authorization", "repo_root", "cancel"])
        self.assertEqual(list(inspect.signature(confirm_probe).parameters), ["authorization", "repo_root"])
        self.assertEqual(list(inspect.signature(run_confirmed_probe).parameters), ["confirmed", "cancel"])

    def test_consent_is_never_taken_from_stdin_environment_or_files(self) -> None:
        source = (ROOT / "runtime" / "probe_confirmation.py").read_text()
        for forbidden in ("sys.stdin", "input(", "os.environ", "getenv", "write_text", "pickle",
                          "shell=True", "subprocess", "--model", "login"):
            with self.subTest(forbidden=forbidden):
                self.assertNotIn(forbidden, source)

    def test_module_never_opens_files_other_than_the_terminal(self) -> None:
        source = (ROOT / "runtime" / "probe_confirmation.py").read_text()
        self.assertEqual(re.findall(r"os\.open\([^)]*\)", source), ['os.open("/dev/tty", os.O_RDWR | os.O_NOCTTY)'])
        self.assertIsNone(re.search(r"(?<![.\w])open\(", source))

    def test_approval_is_not_persisted_or_exported(self) -> None:
        before_env = dict(os.environ)
        watched = (self.repo, self.vault, self.parent, self.bin, Path.cwd())
        before = {path: sorted(os.listdir(path)) for path in watched}
        self.approve()
        self.assertEqual(dict(os.environ), before_env)
        self.assertEqual({path: sorted(os.listdir(path)) for path in watched}, before)
        self.assertNoExecution()

    def test_no_module_other_than_the_confirmation_module_references_it(self) -> None:
        for path in (ROOT / "runtime").rglob("*.py"):
            if path.name != "probe_confirmation.py":
                with self.subTest(path=path.name):
                    self.assertNotIn("probe_confirmation", path.read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
