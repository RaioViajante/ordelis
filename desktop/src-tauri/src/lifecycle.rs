// Execution lifecycle contracts (D3.2b-1, issue #112; design record #108; D3.2b design r5, section 11.1; see
// docs/implementation/desktop-execution-contracts.md). Pure data and pure functions only. Nothing here starts, stops,
// finds or observes a process, reads a clock, a file or the environment, persists anything, or is reachable from the
// webview: no command calls it, and config.test.ts pins that the compiled files contain no process, filesystem,
// network, environment, clock or FFI code.
//
// model: the minimum stable vocabulary that the later reducer, live layer, reconciliation and journal codec share
// (runs, attempts, approvals, events, states, concerns, limits and closed rejection and fault codes).
//
// Nothing in this module is authority. A run, an attempt, an approval decision or an event is history. The live
// authority that a later increment adds exists only in memory and is never built from these values.
// b-1 is vocabulary with no consumer yet (the reducer is b-2), so test builds would flag unused variants.
#[allow(dead_code)]
pub(crate) mod model;
