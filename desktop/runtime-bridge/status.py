"""Development-only runtime identity query for the desktop bridge.

Standard library only until the interpreter version is checked, so an unsupported interpreter is reported
before any runtime module can fail to parse or import. Prints one JSON object and nothing else. It starts no
provider, adapter, session, subprocess or network request and reads no vault, project or credential.
Launched as: python -I -S -B desktop/runtime-bridge/status.py
See docs/implementation/desktop-runtime-bridge.md.
"""
import os
import sys

MIN_PYTHON = (3, 10)
# Exit status that tells the Rust side "interpreter too old" without any output to parse.
EXIT_UNSUPPORTED_VERSION = 78
SCHEMA_VERSION = 1


def main(version_info=None):
    version_info = sys.version_info if version_info is None else version_info
    if tuple(version_info[:2]) < MIN_PYTHON:
        return EXIT_UNSUPPORTED_VERSION

    # -I leaves the checkout off sys.path; appending keeps the standard library first.
    sys.path.append(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
    import json

    from runtime import __version__
    from runtime.execution_safety_gate import POLICY_VERSION

    report = {
        "schema_version": SCHEMA_VERSION,
        "runtime": {"version": __version__, "python": "%d.%d.%d" % tuple(version_info[:3])},
        "gate": {"policy_version": POLICY_VERSION},
    }
    sys.stdout.write(json.dumps(report, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    sys.exit(main())
