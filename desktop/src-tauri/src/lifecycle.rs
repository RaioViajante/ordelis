// Execution lifecycle contracts (D3.2b-1, issue #112, D3.2b-2, issue #114, D3.2b-3, issue #116, and D3.2b-4, issue
// #118; design record #108; D3.2b design r5, sections 3, 4, 6, 7 and 11; see
// docs/implementation/desktop-execution-contracts.md). Pure data and pure functions only. Nothing here starts, stops,
// finds or observes a process, reads a clock, a file or the environment, persists anything, or is reachable from the
// webview: no command calls it, and config.test.ts pins that the compiled files contain no process, filesystem,
// network, environment, clock or FFI code. The codec reads and writes bytes it is given, never a file.
//
// model: the minimum stable vocabulary that the later reducer, live layer, reconciliation and journal codec share
// (runs, attempts, approvals, events, states, concerns, limits and closed rejection and fault codes).
//
// Nothing in this module but `live` is authority. A run, an attempt, an approval decision, an event or a projection is
// history. Live authority exists only in memory, in `live`, and is never built from these values.
// Parts of the vocabulary serve only later increments (reconciliation, the codec, the interface), so test builds
// would flag them as unused.
#[allow(dead_code)]
pub(crate) mod model;
// reduce: the deterministic reducer (D3.2b-2, issue #114). It records drafts and recorded frames into a projection,
// which is history, never authority.
pub(crate) mod reduce;
// live: the in-memory live authority of one supervised run (D3.2b-2). The only place a grant is made; nothing reaches
// it from the journal, and nothing in it starts a process.
pub(crate) mod live;
// reconcile: conservative crash reconciliation and cross-run start conflicts (D3.2b-3). History in, history out: it
// proposes an opening frame and inert recommendations, and never stops, starts or permits anything.
pub(crate) mod reconcile;
// codec: the canonical ORDJ1 journal codec (D3.2b-4, issue #118): framing, CRC-32C, canonical JSON through its own wire
// types, classification of damaged bytes, the audit scan and the byte-exact check of an uncertain write. Bytes and
// values only: it reads and writes no file and repairs nothing. The only lifecycle file that names serde.
pub(crate) mod codec;
// replay: full deterministic replay of a decoded journal through the reducer (D3.2b-4). History, never authority.
pub(crate) mod replay;
