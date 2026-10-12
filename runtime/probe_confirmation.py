"""Interactive terminal confirmation for controlled Codex probes.

Library only: no CLI command, no persistence, no dispatch. `confirm_probe` shows the exact operation on the
controlling terminal (`/dev/tty`) and returns a `ConfirmedProbe` only if the user types a phrase bound to
that exact specification. Consent is never read from stdin, files, environment variables or arguments.

This is a process-local, same-user control. It cannot resist a malicious process running as the same user
(which can drive the terminal, replace this module or call the runner directly), and it does not
authenticate the evidence the runner returns.
"""

from __future__ import annotations

import hashlib
import json
import os
import secrets
import threading
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path

from .codex_probe_runner import (
    ALLOWED_COMMANDS, ProbeAuthorization, ProbeError, _check_window, _preflight, _run_probe)

MAX_ANSWER_BYTES = 256
PHRASE_DIGITS = 12
MAX_PENDING = 64
_DECLINED = {"n", "no", "deny", "cancel"}
# Process-local registry of confirmations this module issued: nonce -> (spec digest, checkout, expiry).
_ISSUED: dict[str, tuple[str, str, float]] = {}
_ISSUED_LOCK = threading.Lock()


@dataclass(frozen=True)
class ConfirmedProbe:
    """Proof that `confirm_probe` returned this authorization after a terminal approval, in this process.

    A hand-built or copied instance has no registry entry and is refused. Anything running in this process
    can still bypass the check, so this is a guard against mistakes and replay, not against hostile code.
    """

    authorization: ProbeAuthorization
    nonce: str = field(repr=False)


def _digest(authorization: ProbeAuthorization, repo: Path, protected: tuple[Path, ...]) -> str:
    spec = asdict(authorization)
    spec.update({"protected_roots": [str(root) for root in protected], "checkout": str(repo),
                 "commands": [list(args) for args in ALLOWED_COMMANDS]})
    return hashlib.sha256(json.dumps(spec, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def _phrase(digest: str) -> str:
    return f"authorize {digest[:PHRASE_DIGITS]}"


def _q(value: object) -> str:
    """Quote a value so control characters and look-alike text cannot alter the prompt."""
    return json.dumps(value, ensure_ascii=True)


def _render(authorization: ProbeAuthorization, repo: Path, protected: tuple[Path, ...],
            identity: dict, digest: str) -> str:
    remaining = int(authorization.expires_at - time.time())
    commands = "  |  ".join(" ".join(args) for args in ALLOWED_COMMANDS)
    return "\n".join([
        "", "=== Synapse: authorize ONE controlled Codex probe ===",
        "Nothing runs unless you type the exact phrase at the bottom.", "",
        f"Executable   : {_q(identity['path'])}", f"SHA-256      : {identity['sha256']}",
        f"Commands     : {commands}   (in this order; nothing else)",
        f"Scratch      : new private subdirectory under {_q(authorization.scratch_parent)}",
        f"Protected    : {', '.join(_q(str(root)) for root in protected)}",
        f"Checkout     : {_q(str(repo))} (excluded)",
        f"Limits       : {authorization.command_timeout_seconds}s per command, "
        f"{authorization.total_timeout_seconds}s total, {authorization.max_output_bytes} output bytes per command",
        f"Probe id     : {_q(authorization.probe_id)}   Valid for: {remaining}s   Mode: read_only (declared)",
        "Model calls  : 0 authorized. No authentication and no task is sent.",
        f"Spec digest  : {digest}", "",
        "NOT guaranteed (unknown or unenforced):",
        "  - Filesystem: the program runs as your user and can read or write whatever you can.",
        "  - Network: not restricted or observed; zero model calls is declared, not enforced.",
        "  - The hash covers this launcher file only, not any native binary it starts.",
        "  - The file is run by pathname, so it could be swapped between check and run.",
        "  - Descendants outside the process group are not tracked; CPU and memory are not limited.",
        "  - This prompt cannot detect a malicious process running as your user.",
        "This approval is for this one run in this process only and is not saved.", "",
        f"To authorize this exact run, type:  {_phrase(digest)}", "Anything else cancels.", "> "])


def _open_terminal() -> int:
    """Open the controlling terminal. Default deny when there is none or we are not in the foreground."""
    try:
        fd = os.open("/dev/tty", os.O_RDWR | os.O_NOCTTY)
    except OSError as exc:
        raise ProbeError("no_controlling_terminal") from exc
    try:
        if not os.isatty(fd):
            raise ProbeError("no_controlling_terminal")
        if os.tcgetpgrp(fd) != os.getpgrp():
            raise ProbeError("terminal_not_foreground")
    except OSError as exc:
        os.close(fd)
        raise ProbeError("no_controlling_terminal") from exc
    except BaseException:
        os.close(fd)
        raise
    return fd


def _write_all(fd: int, text: str) -> None:
    data = text.encode("utf-8")
    while data:
        data = data[os.write(fd, data):]


def _read_line(fd: int) -> str:
    buffer = bytearray()
    while b"\n" not in buffer:
        if len(buffer) > MAX_ANSWER_BYTES:
            raise ProbeError("not_confirmed")
        chunk = os.read(fd, MAX_ANSWER_BYTES + 1)
        if not chunk:
            raise ProbeError("eof" if not buffer else "not_confirmed")
        buffer += chunk
    line = bytes(buffer)
    if line.index(b"\n") != len(line) - 1 or len(line) > MAX_ANSWER_BYTES:
        raise ProbeError("not_confirmed")
    try:
        return line.removesuffix(b"\n").removesuffix(b"\r").decode("utf-8")
    except UnicodeError as exc:
        raise ProbeError("not_confirmed") from exc


def _ask(text: str) -> str:
    import termios

    fd = _open_terminal()
    try:
        termios.tcflush(fd, termios.TCIFLUSH)  # type-ahead typed before the prompt must not count
        _write_all(fd, text)
        try:
            return _read_line(fd)
        finally:
            termios.tcflush(fd, termios.TCIFLUSH)  # leftover typed lines must not reach the shell
    except KeyboardInterrupt as exc:
        raise ProbeError("interrupted") from exc
    except (OSError, termios.error) as exc:
        raise ProbeError("terminal_io_failed") from exc
    finally:
        os.close(fd)


def confirm_probe(authorization: ProbeAuthorization, *, repo_root: Path) -> ConfirmedProbe:
    """Show the exact operation on the controlling terminal and return a ConfirmedProbe only on approval.

    Raises ProbeError with a closed reason code on any refusal. Nothing is executed, consumed or stored on
    disk. The authorization is validated before the prompt and again after the user answers.
    """
    if os.name != "posix":
        raise ProbeError("unsupported_platform")
    if not isinstance(authorization, ProbeAuthorization):
        raise ProbeError("authorization_missing")
    _check_window(authorization)
    repo, protected, _, _, identity, _, _ = _preflight(authorization, repo_root)
    digest = _digest(authorization, repo, protected)
    answer = _ask(_render(authorization, repo, protected, identity, digest))
    if answer != _phrase(digest):
        raise ProbeError("declined" if answer.strip().lower() in _DECLINED else "not_confirmed")
    _check_window(authorization)
    repo, protected, *_ = _preflight(authorization, repo_root)
    if _digest(authorization, repo, protected) != digest:
        raise ProbeError("specification_changed_during_confirmation")
    nonce = secrets.token_hex(16)
    with _ISSUED_LOCK:
        for key in [key for key, (_, _, expiry) in _ISSUED.items() if expiry <= time.time()]:
            del _ISSUED[key]
        if len(_ISSUED) >= MAX_PENDING:
            raise ProbeError("confirmation_registry_full")
        _ISSUED[nonce] = (digest, str(repo), authorization.expires_at)
    return ConfirmedProbe(authorization, nonce)


def run_confirmed_probe(confirmed: ConfirmedProbe, *, cancel: threading.Event | None = None) -> dict[str, object]:
    """Run the probe the user approved, exactly once. Any mismatch is refused before a process exists."""
    if os.name != "posix":
        raise ProbeError("unsupported_platform")
    if not isinstance(confirmed, ConfirmedProbe) or not isinstance(confirmed.nonce, str):
        raise ProbeError("confirmation_missing")
    with _ISSUED_LOCK:
        entry = _ISSUED.pop(confirmed.nonce, None)
    if entry is None:
        raise ProbeError("confirmation_invalid_or_reused")
    digest, checkout, _ = entry
    authorization = confirmed.authorization
    if not isinstance(authorization, ProbeAuthorization):
        raise ProbeError("authorization_missing")
    _check_window(authorization)
    try:
        repo = Path(checkout).resolve(strict=True)
        protected = tuple(Path(root).resolve(strict=True) for root in authorization.protected_roots)
        current = _digest(authorization, repo, protected)
    except (OSError, TypeError, ValueError) as exc:
        raise ProbeError("specification_changed_after_confirmation") from exc
    if current != digest:
        raise ProbeError("specification_changed_after_confirmation")
    return _run_probe(authorization, repo, cancel, "terminal_confirmed_process_local")
