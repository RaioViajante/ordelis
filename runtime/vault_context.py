"""Read bounded, indexed Markdown context without dispatching an agent."""

from __future__ import annotations

import json
import os
import re
import stat
from pathlib import Path, PurePosixPath

MAX_INDEX_BYTES = 65536
MAX_NOTE_BYTES = 65536
MAX_SELECTED_NOTES = 32
MAX_EXCERPT_BYTES = 2048
MAX_TOTAL_BYTES = 8192
MAX_OUTPUT_BYTES = 32768


class VaultContextError(ValueError):
    pass


def _unique_object(pairs: list[tuple[str, object]]) -> dict[str, object]:
    result: dict[str, object] = {}
    for key, value in pairs:
        if key in result:
            raise VaultContextError(f"duplicate index key: {key}")
        result[key] = value
    return result


def _read_bounded(path: Path, limit: int) -> bytes:
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(descriptor, "rb") as source:
        if not stat.S_ISREG(os.fstat(source.fileno()).st_mode):
            raise VaultContextError("index must be a regular file")
        data = source.read(limit + 1)
    if len(data) > limit:
        raise VaultContextError(f"file exceeds {limit} bytes: {path}")
    return data


def _text(value: object, label: str, maximum: int) -> str:
    if not isinstance(value, str) or not value or len(value.encode("utf-8")) > maximum or any(ord(c) < 32 for c in value):
        raise VaultContextError(f"invalid {label}")
    return value


def _relative_note_path(value: object) -> str:
    path = _text(value, "note path", 512)
    pure = PurePosixPath(path)
    if (pure.is_absolute() or "\\" in path or ":" in path or
            any(part in ("", ".", "..") for part in path.split("/")) or pure.suffix.lower() != ".md"):
        raise VaultContextError(f"invalid note path: {path}")
    return path


def _load_index(path: Path) -> dict[str, dict[str, list[dict[str, str]]]]:
    if path.is_symlink() or not path.is_file():
        raise VaultContextError("index must be a regular file")
    try:
        data = json.loads(_read_bounded(path, MAX_INDEX_BYTES).decode("utf-8"), object_pairs_hook=_unique_object)
    except (UnicodeError, json.JSONDecodeError) as exc:
        raise VaultContextError("index must be valid UTF-8 JSON") from exc
    if not isinstance(data, dict) or set(data) != {"projects", "topics"}:
        raise VaultContextError("index must contain projects and topics maps")
    count = 0
    for kind in ("projects", "topics"):
        group = data[kind]
        if not isinstance(group, dict):
            raise VaultContextError(f"{kind} must be a map")
        for identifier, entries in group.items():
            _text(identifier, f"{kind} ID", 128)
            if not isinstance(entries, list) or not entries:
                raise VaultContextError(f"{kind}.{identifier} must be a nonempty list")
            count += len(entries)
            if count > 512:
                raise VaultContextError("index contains too many entries")
            for entry in entries:
                if not isinstance(entry, dict) or set(entry) not in ({"path"}, {"path", "heading"}):
                    raise VaultContextError("each index entry needs path and optional heading")
                _relative_note_path(entry["path"])
                if "heading" in entry:
                    _text(entry["heading"], "heading", 128)
    return data


def _read_note(vault: Path, relative: str) -> str:
    # Open every component relative to its parent without following links.
    flags = os.O_RDONLY | os.O_NOFOLLOW
    directory = os.open(vault, flags | os.O_DIRECTORY)
    try:
        parts = relative.split("/")
        for part in parts[:-1]:
            next_directory = os.open(part, flags | os.O_DIRECTORY, dir_fd=directory)
            os.close(directory)
            directory = next_directory
        note = os.open(parts[-1], flags | os.O_NONBLOCK, dir_fd=directory)
        try:
            if not stat.S_ISREG(os.fstat(note).st_mode):
                raise VaultContextError(f"not a regular note: {relative}")
            with os.fdopen(note, "rb", closefd=False) as source:
                data = source.read(MAX_NOTE_BYTES + 1)
        finally:
            os.close(note)
    except OSError as exc:
        raise VaultContextError(f"cannot read indexed note: {relative}") from exc
    finally:
        os.close(directory)
    if len(data) > MAX_NOTE_BYTES:
        raise VaultContextError(f"note exceeds {MAX_NOTE_BYTES} bytes: {relative}")
    try:
        return data.decode("utf-8")
    except UnicodeError as exc:
        raise VaultContextError(f"invalid UTF-8 note: {relative}") from exc


def _section(note: str, heading: str) -> tuple[str, bool]:
    lines = note.splitlines(keepends=True)
    start = None
    level = 0
    for index, line in enumerate(lines):
        match = re.match(r"^ {0,3}(#{1,6})[ \t]+(.+?)[ \t]*#*[ \t]*(?:\r?\n)?$", line)
        if not match:
            continue
        if start is None and match.group(2) == heading:
            start, level = index + 1, len(match.group(1))
        elif start is not None and len(match.group(1)) <= level:
            return "".join(lines[start:index]), False
    if start is None:
        return "", True
    return "".join(lines[start:]), False


def _limit_excerpt(value: str, limit: int) -> tuple[str, bool]:
    encoded = value.encode("utf-8")
    if len(encoded) <= limit:
        return value, False
    return encoded[:limit].decode("utf-8", "ignore"), True


def preview(vault_path: Path, index_path: Path, project: str | None, topic: str | None,
            agent: str, model: str, effort: str, per_note_bytes: int = MAX_EXCERPT_BYTES,
            total_bytes: int = MAX_TOTAL_BYTES) -> dict[str, object]:
    if not project and not topic:
        raise VaultContextError("project or topic ID is required")
    for name, value in (("project", project), ("topic", topic)):
        if value is not None:
            _text(value, name, 128)
    for name, value in (("agent", agent), ("model", model), ("effort", effort)):
        _text(value, name, 128)
    if type(per_note_bytes) is not int or not 1 <= per_note_bytes <= MAX_EXCERPT_BYTES:
        raise VaultContextError("invalid per-note byte limit")
    if type(total_bytes) is not int or not 1 <= total_bytes <= MAX_TOTAL_BYTES:
        raise VaultContextError("invalid total byte limit")
    if vault_path.is_symlink() or not vault_path.is_dir():
        raise VaultContextError("vault must be an existing directory, not a symlink")
    index = _load_index(index_path)
    selected: set[tuple[str, str | None]] = set()
    for kind, identifier in (("projects", project), ("topics", topic)):
        if identifier is not None:
            if identifier not in index[kind]:
                raise VaultContextError(f"unknown {kind[:-1]} ID: {identifier}")
            selected.update((entry["path"], entry.get("heading")) for entry in index[kind][identifier])
    if len(selected) > MAX_SELECTED_NOTES:
        raise VaultContextError("too many selected notes")
    remaining = total_bytes
    excerpts = []
    for path, heading in sorted(selected, key=lambda item: (item[0], item[1] or "")):
        note = _read_note(vault_path, path)
        source, missing = _section(note, heading) if heading else (note, False)
        excerpt, truncated = _limit_excerpt(source, min(per_note_bytes, remaining))
        remaining -= len(excerpt.encode("utf-8"))
        excerpts.append({"path": path, "heading": heading, "excerpt": excerpt,
                         "missing_heading": missing, "truncated": truncated})
    result = {"schema_version": 1, "selection": {"project": project, "topic": topic},
              "context": {"kind": "reference_data", "excerpts": excerpts,
                          "total_excerpt_bytes": total_bytes - remaining},
              "execution": {"agent": agent, "model": model, "effort": effort,
                            "policy": {"dispatch": False, "vault_access": "read_only"},
                            "validated_provider_settings": False}}
    if len(json.dumps(result, ensure_ascii=True, sort_keys=True, separators=(",", ":")).encode("utf-8")) > MAX_OUTPUT_BYTES:
        raise VaultContextError(f"preview exceeds {MAX_OUTPUT_BYTES} output bytes")
    return result
