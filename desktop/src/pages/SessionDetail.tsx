import { useEffect } from "react";
import { hrefFor, type Location } from "../app/routes";
import { useSampleData } from "../app/SampleData";
import { useWorkspace } from "../app/Workspace";
import { findProject, findTask } from "../fixtures/mock";
import {
  findSession, offsetLabel, orderedEvents, sessionsForTask, STATE_LABEL, STATE_TONE,
  type Basis, type HandoffStatus, type ResultAvailability,
} from "../fixtures/sessions";
import { Continuity } from "../components/Continuity";
import { Agent, Empty, Evidence, PageTitle, SecHead, type EvidenceLevel } from "../components/ui";

const BASIS_LEVEL: Record<Basis, EvidenceLevel> = { measured: "ver", estimated: "adv", reported: "adv", unknown: "unk" };
const BASIS_HELP: Record<Basis, string> = {
  measured: "counted by this app",
  estimated: "a rule of thumb, not counted",
  reported: "stated by the agent CLI, not checked",
  unknown: "no value is available",
};
const RESULT_LABEL: Record<ResultAvailability, string> = {
  available: "available", partial: "partial", pending: "not yet — session still running", none: "none produced",
};
const HANDOFF_LABEL: Record<HandoffStatus, string> = {
  ready: "could be prepared", partial: "could be partly prepared", "not-prepared": "nothing to prepare",
};

export function SessionDetail({ id, go }: { id: string; go: (page: Location["page"], param?: string) => void }) {
  const { on } = useSampleData();
  const { selectSession } = useWorkspace();
  const s = on ? findSession(id) : undefined;

  useEffect(() => {
    if (s) selectSession(s.id);
  }, [s, selectSession]);

  if (!s) {
    return (
      <div className="col col--1300">
        <h1 className="h1">Session</h1>
        <Empty
          title="Session not found"
          why={on ? `no sample session has the id “${id}”` : "sessions: 0 · sample data is hidden"}
          actions={<button type="button" className="btn" onClick={() => go("sessions")}>Back to sessions</button>}
        >
          {on ? "This link points at a session that isn’t in the sample data." : "There are no sessions to show while sample data is hidden."}
        </Empty>
      </div>
    );
  }

  const task = findTask(s.taskId);
  const siblings = sessionsForTask(s.taskId).filter((o) => o.id !== s.id);
  const events = orderedEvents(s.events);

  return (
    <div className="col col--1300">
      <a className="link" href={hrefFor("sessions")}>← All sessions</a>
      <PageTitle lede="A sample session record. It was not recorded from a real process, and nothing is running or can be controlled from here.">
        {s.task}
      </PageTitle>
      <div className="flex flex-wrap items-center gap-4">
        <Agent id={s.agent}>{s.agentName}</Agent>
        <span className={`st st--${STATE_TONE[s.state]}`}>{STATE_LABEL[s.state].toLowerCase()}</span>
        <span className="mono meta">session {s.id} · synthetic</span>
      </div>

      <div className="split">
        <div className="flex min-w-0 flex-col gap-8">
          <section aria-labelledby="tl-h">
            <SecHead title={<span id="tl-h">Recorded timeline</span>} meta="synthetic example · not live, not streaming" />
            <ol className="tl" aria-label={`Recorded synthetic events for session ${s.id}, oldest first`}>
              {events.map((e, i) => (
                <li key={i} className={`tl__i tl__i--${e.kind}`}>
                  <span className="mono meta">{offsetLabel(e.t)}</span>
                  <span className="mono tl__k">{e.kind}</span>
                  <span className={e.text ? "mono" : "meta"}>{e.text ?? "no detail recorded"}</span>
                </li>
              ))}
            </ol>
            <p className="hint mt-2.5 !text-[13px]">
              Offsets count from the example’s own start. This is a recorded sample, not output from a running terminal.
            </p>
          </section>

          <section aria-labelledby="res-h">
            <SecHead title={<span id="res-h">Result</span>} meta={RESULT_LABEL[s.result.availability]} />
            <p className="preview-text">{s.result.summary}</p>
            <p className="hint mt-2 !text-[13.5px]">
              <b className="font-medium text-ink-100">Limits.</b> {s.result.limits}
            </p>
          </section>
        </div>

        <aside className="aside" aria-label="Session facts">
          <section aria-labelledby="facts-h">
            <SecHead title={<span id="facts-h">Session</span>} meta="sample" />
            <dl className="kv2 kv2--90">
              <dt>State</dt>
              <dd className="!font-sans">{STATE_LABEL[s.state]} · {s.reason}</dd>
              <dt>Started</dt>
              <dd className="!font-sans">{s.started}</dd>
              <dt>Worktree</dt>
              <dd>{s.worktree ?? "none"} <span className="ink2 !font-sans">· {s.worktreeNote} · not a real path</span></dd>
              <dt>Handoff</dt>
              <dd className="!font-sans">{HANDOFF_LABEL[s.handoff.status]} — {s.handoff.note}</dd>
              <dt>Source</dt>
              <dd className="!font-sans">{s.provenance}</dd>
            </dl>
          </section>

          <section aria-labelledby="use-h">
            <SecHead title={<span id="use-h">Usage</span>} meta="sample figures" />
            <table className="utbl">
              <caption className="sr">Usage for sample session {s.id}, with how each figure was obtained</caption>
              <thead>
                <tr><th scope="col">Figure</th><th scope="col">Value</th><th scope="col">Basis</th></tr>
              </thead>
              <tbody>
                {s.usage.map((u) => (
                  <tr key={u.label}>
                    <th scope="row">{u.label}</th>
                    <td className="mono">{u.value ?? "unknown"}</td>
                    <td><Evidence level={BASIS_LEVEL[u.basis]}>{u.basis}</Evidence></td>
                  </tr>
                ))}
              </tbody>
            </table>
            <p className="hint mt-2.5 !text-[13px]">
              measured: {BASIS_HELP.measured} · estimated: {BASIS_HELP.estimated} · reported: {BASIS_HELP.reported} · unknown:{" "}
              {BASIS_HELP.unknown}. Every figure here was invented, including the measured ones.
            </p>
          </section>

          <section aria-labelledby="rel-h">
            <SecHead title={<span id="rel-h">Related</span>} />
            <dl className="kv2 kv2--90">
              <dt>Task</dt>
              <dd className="!font-sans">
                {task ? <a className="link" href={hrefFor("task", task.id)}>Preview task</a> : "unavailable"}
              </dd>
              <dt>Project</dt>
              <dd className="!font-sans">
                <a className="link" href={hrefFor("projects", s.project)}>{s.project}</a>
                {" · "}
                <span className="mono">{findProject(s.project)?.path}</span>
              </dd>
            </dl>
            {siblings.length > 0 && (
              <>
                <p className="t-label mt-4 mb-1">Other sessions on this task</p>
                <ul className="rows">
                  {siblings.map((o) => (
                    <li key={o.id} className="r">
                      <a className="link flex-1" href={hrefFor("sessions", o.id)}>{o.agentName} · {o.id}</a>
                      <span className={`st st--${STATE_TONE[o.state]}`}>{STATE_LABEL[o.state].toLowerCase()}</span>
                    </li>
                  ))}
                </ul>
              </>
            )}
          </section>
        </aside>
      </div>

      <Continuity session={s} />
    </div>
  );
}
