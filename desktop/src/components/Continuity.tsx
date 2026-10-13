import { findProject } from "../fixtures/mock";
import type { MockSession, SessionState } from "../fixtures/sessions";
import { Button, Evidence, SecHead } from "./ui";

const NOT_CONNECTED = "Sessions come from the runtime, which this build isn't connected to. Nothing here can start, resume or repeat a run.";

const STATE_NOTE: Record<SessionState, string> = {
  running: "This sample is shown as running, so there is nothing to pick up yet. The columns show how the options would differ once it ends.",
  paused: "A paused session is the case where \"continue\" would apply, if the agent supports it.",
  stopped: "A stopped session keeps its worktree and files. Whether its conversation can be resumed is a separate question.",
  finished: "A finished session has a result to build on. A new run starts from its files, not from its conversation.",
  blocked: "This session never began work, so there is no conversation, worktree or files to carry over.",
};

/** Informational comparison of the three ways to pick up a session. Every action is disabled. */
export function Continuity({ session: s }: { session: MockSession }) {
  const head = findProject(s.project)?.head ?? "—";
  const wt = s.worktree;
  const started = s.state !== "blocked";
  return (
    <section aria-labelledby={`pick-${s.id}`}>
      <SecHead
        title={
          <span id={`pick-${s.id}`}>
            Pick up from{" "}
            <span className="mono !text-base">
              {s.agentName} · {wt ?? "no worktree"}
            </span>
          </span>
        }
        meta="three different things — not interchangeable"
      />
      <p className="hint mb-3 !text-[13.5px]">
        Informational only, about a sample session. {STATE_NOTE[s.state]}
      </p>
      <div className="tw">
        <table className="mx">
          <caption className="sr">How continuing, starting a new run and repeating with new settings differ for sample session {s.id}</caption>
          <thead>
            <tr>
              <th scope="col"><span className="sr">Aspect</span></th>
              <th scope="col">
                Continue this session<small>Same process or CLI conversation, same worktree.</small>
              </th>
              <th scope="col">
                New run with this context<small>Fresh CLI session; Synapse would hand over a summary of this one.</small>
              </th>
              <th scope="col">
                Repeat with new settings<small>Run the original step again from the base, configured differently.</small>
              </th>
            </tr>
          </thead>
          <tbody>
            <tr><th scope="row">CLI conversation</th><td className={started ? "keep" : "no"}>{started ? "kept" : "none to keep"}</td><td className="no">new</td><td className="no">new</td></tr>
            <tr><th scope="row">Worktree</th><td className={wt ? "keep" : "no"}>{wt ? `same · ${wt}` : "none exists"}</td><td className="new">new{wt ? `, branched from ${wt} HEAD` : ", from the base"}</td><td className="new">new, from {head}</td></tr>
            <tr><th scope="row">Files from this run</th><td className={s.result.availability === "none" ? "no" : "keep"}>{s.result.availability === "none" ? "none produced" : "all, as they are"}</td><td className={s.result.availability === "none" ? "no" : "keep"}>{s.result.availability === "none" ? "none produced" : "all, as a starting point"}</td><td className="no">not carried over · kept for comparison</td></tr>
            <tr><th scope="row">Context passed</th><td className="no">nothing new</td><td className="new">summary (~900 est. tok) + diff + notes</td><td className="new">original notes, re-selected</td></tr>
            <tr><th scope="row">Agent / model</th><td className="no">fixed</td><td className="new">can change</td><td className="new">can change</td></tr>
            <tr><th scope="row">Authorization</th><td className="keep">original, if still in scope</td><td className="new">new authorization</td><td className="new">new authorization</td></tr>
            <tr>
              <th scope="row">Requires</th>
              <td>
                <Evidence level="unk">
                  {s.resume === "advertised" ? `resume is advertised for ${s.agentName}, not verified` : `resume support for ${s.agentName} is unknown`}
                </Evidence>
              </td>
              <td><Evidence level="unk">not checked</Evidence></td>
              <td><Evidence level="unk">not checked</Evidence></td>
            </tr>
            <tr>
              <th scope="row"><span className="sr">Actions</span></th>
              <td><Button small reason={NOT_CONNECTED}>Try to resume</Button></td>
              <td><Button small reason={NOT_CONNECTED}>Plan a new run</Button></td>
              <td><Button small reason={NOT_CONNECTED}>Change settings</Button></td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>
  );
}
