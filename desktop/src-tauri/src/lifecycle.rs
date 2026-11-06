// Execution lifecycle contracts (D3.2b-1, issue #112, and D3.2b-2, issue #114; design record #108; D3.2b design r5,
// sections 3, 6 and 11; see docs/implementation/desktop-execution-contracts.md). Pure data and pure functions only.
// Nothing here starts, stops, finds or observes a process, reads a clock, a file or the environment, persists
// anything, or is reachable from the webview: no command calls it, and config.test.ts pins that the compiled files
// contain no process, filesystem, network, environment, clock or FFI code.
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
