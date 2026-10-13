import { useState } from "react";
import { useSampleData } from "../app/SampleData";
import { MOCK_SESSIONS } from "../fixtures/mock";
import { Agent, Button, Empty, Evidence, PageTitle } from "../components/ui";

const NOT_CONNECTED = "Sessions come from the runtime, which this build isn't connected to.";

export function Sessions() {
  const { on } = useSampleData();
  const [picked, setPicked] = useState(1);
  const cur = MOCK_SESSIONS[picked];

  return (
    <div className="col col--1300">
      <div className="flex flex-wrap items-end gap-5">
        <PageTitle lede="Every agent run, across projects. Terminals for a single task live inside that task.">Sessions</PageTitle>
        <span className="sp" />
        <span className="meta">{on ? "sample · 1 running · 1 paused · 1 stopped · 1 finished" : "0 sessions"}</span>
      </div>

      {!on ? (
        <Empty title="No sessions yet" why="sessions: 0 · no agent has been started by this app">
          A session is one agent CLI working in one worktree. None can start from this build: it doesn’t run agents or contact
          providers.
        </Empty>
      ) : (
        <>
          <section>
            <div className="hdr t-label">
              <span>Agent</span>
              <span>Task</span>
              <span>Project</span>
              <span>State</span>
              <span>Worktree</span>
            </div>
            {MOCK_SESSIONS.map((s, i) => (
              <button
                type="button"
                key={s.id}
                className={`srow${i === picked ? " on" : ""}`}
                aria-pressed={i === picked}
                onClick={() => setPicked(i)}
              >
                <Agent id={s.agent}>{s.agentName}</Agent>
                <span className="min-w-0">{s.task}</span>
                <span className="mono meta">{s.project}</span>
                <span className={`st st--${s.tone}`}>{s.state}</span>
                <span className="mono meta">{s.wt}</span>
              </button>
            ))}
          </section>

          <section>
            <div className="sec-h">
              <h2>
                Pick up from{" "}
                <span className="mono !text-base">
                  {cur.agentName} · {cur.wt}
                </span>
              </h2>
              <span className="meta">three different things — not interchangeable</span>
            </div>
            <div className="tw">
              <table className="mx">
                <thead>
                  <tr>
                    <th />
                    <th>
                      Continue this session<small>Same process or CLI conversation, same worktree.</small>
                    </th>
                    <th>
                      New run with this context<small>Fresh CLI session; Synapse hands over a summary of this one.</small>
                    </th>
                    <th>
                      Repeat with new settings<small>Run the original step again from the base, configured differently.</small>
                    </th>
                  </tr>
                </thead>
                <tbody>
                  <tr><td>CLI conversation</td><td className="keep">kept</td><td className="no">new</td><td className="no">new</td></tr>
                  <tr><td>Worktree</td><td className="keep">same · {cur.wt}</td><td className="new">new, branched from {cur.wt} HEAD</td><td className="new">new, from main @ 3f2a91c</td></tr>
                  <tr><td>Files from this run</td><td className="keep">all, as they are</td><td className="keep">all, as a starting point</td><td className="no">not carried over · kept for comparison</td></tr>
                  <tr><td>Context passed</td><td className="no">nothing new</td><td className="new">summary ~900 tok + diff + notes</td><td className="new">original notes, re-selected</td></tr>
                  <tr><td>Agent / model</td><td className="no">fixed</td><td className="new">can change</td><td className="new">can change</td></tr>
                  <tr><td>Authorization</td><td className="keep">original, if still in scope</td><td className="new">new authorization</td><td className="new">new authorization</td></tr>
                  <tr>
                    <td>Requires</td>
                    <td><Evidence level="unk">resume is not checked</Evidence></td>
                    <td><Evidence level="unk">not checked</Evidence></td>
                    <td><Evidence level="unk">not checked</Evidence></td>
                  </tr>
                  <tr>
                    <td />
                    <td><Button small reason={NOT_CONNECTED}>Try to resume</Button></td>
                    <td><Button small reason={NOT_CONNECTED}>Plan a new run</Button></td>
                    <td><Button small reason={NOT_CONNECTED}>Change settings</Button></td>
                  </tr>
                </tbody>
              </table>
            </div>
          </section>
        </>
      )}
    </div>
  );
}

