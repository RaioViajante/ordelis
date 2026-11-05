// Agent integration foundations (D3.2a, issue #109; design record #108; see docs/implementation/desktop-agents.md).
// Pure data and pure functions only. Nothing here finds, opens, hashes or runs an executable, reads the environment,
// a clock or a file, uses the network, or is reachable from the webview: no command calls it, and config.test.ts pins
// that the compiled files contain no process, filesystem, network, environment, clock or FFI code.
//
// catalog: which agent CLIs Ordelis targets and what their vendors' documentation claims, with references and dates.
// evidence: the vocabulary that keeps such claims apart from observations, mirrored from runtime/runtime_validation.py.
//
// Cataloguing a provider authorizes nothing: not locating it, not running it, not checking its authentication.
pub(crate) mod catalog;
pub(crate) mod evidence;
