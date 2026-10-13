import { useState } from "react";
import { Agent, Button, Evidence, PageTitle, SecHead, Vision, type AgentId } from "../components/ui";

// Which agent CLIs the table lists, and whether an MCO adapter exists for each,
// follows docs/implementation/synapse-mvp-plan.md. No CLI is executed or located
// by this build, so every capability stays "unknown".
const AGENTS: { id: AgentId; name: string; adapter: string | null }[] = [
  { id: "cc", name: "claude-code", adapter: "claude" },
  { id: "cx", name: "codex", adapter: "codex" },
  { id: "oc", name: "opencode", adapter: "opencode" },
  { id: "km", name: "kimi", adapter: null },
];

const CAPABILITIES = [
  "Edit files",
  "Read-only mode",
  "Isolated worktree",
  "Mid-run guidance",
  "Resume session",
  "Clean cancel",
  "Network off",
  "Token usage",
  "Cost",
];

const SETUP = ["Installed", "Authentication", "Default model", "Effort levels", "Last check"];

const NOT_CHECKED = "Checks run the agent CLIs, and this build doesn't connect to the runtime.";

export function Agents() {
  const [picked, setPicked] = useState(3);
  const cur = AGENTS[picked];

  // Setup facts (installed, signed in, ...) are unknown for every agent. Only capabilities
  // are reported as unsupported for an agent that has no adapter to drive them.
  const unknown = <Evidence level="unk">unknown</Evidence>;
  const capability = (a: (typeof AGENTS)[number]) =>
    a.adapter ? unknown : <Evidence level="bad">no adapter</Evidence>;

  return (
    <div className="col col--1300">
      <div className="flex flex-wrap items-end gap-5">
        <PageTitle lede="What each CLI says it can do, and what Synapse has actually checked on this machine.">Agents</PageTitle>
        <span className="sp" />
        <div className="legend2 meta" aria-label="Evidence levels">
          <Evidence level="ver">verified</Evidence>
          <Evidence level="adv">advertised</Evidence>
          <Evidence level="unk">unknown</Evidence>
          <Evidence level="bad">not supported</Evidence>
        </div>
      </div>

      <div className="notice notice--quiet" role="note">
        <span className="mono ink2">◇</span>
        <p className="hint">
          <b className="font-medium text-ink-100">Nothing has been checked.</b> This build never runs an agent CLI, so no row
          below is verified. Unknown is the honest state.
        </p>
      </div>

      <div className="split">
        <div className="flex min-w-0 flex-col gap-9">
          <section>
            <SecHead title="Setup" meta="via MCO adapters" />
            <div className="tw">
              <table className="tbl">
                <thead>
                  <tr>
                    <th />
                    {AGENTS.map((a, i) => (
                      <th key={a.id} className={i === picked ? "sel" : ""}>
                        <button type="button" aria-pressed={i === picked} onClick={() => setPicked(i)}>
                          <Agent id={a.id}>{a.name}</Agent>
                        </button>
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  <tr>
                    <td>Adapter</td>
                    {AGENTS.map((a, i) => (
                      <td key={a.id} className={`m ${i === picked ? "sel" : ""}`}>
                        {a.adapter ?? "none"}
                      </td>
                    ))}
                  </tr>
                  {SETUP.map((row) => (
                    <tr key={row}>
                      <td>{row}</td>
                      {AGENTS.map((a, i) => (
                        <td key={a.id} className={i === picked ? "sel" : ""}>
                          {unknown}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>

          <section>
            <SecHead title="Capabilities" meta="a flag in --help counts as advertised until a check runs" />
            <div className="tw">
              <table className="tbl">
                <thead>
                  <tr>
                    <th />
                    {AGENTS.map((a) => (
                      <th key={a.id}>{a.name}</th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {CAPABILITIES.map((row) => (
                    <tr key={row}>
                      <td>{row}</td>
                      {AGENTS.map((a, i) => (
                        <td key={a.id} className={i === picked ? "sel" : ""}>
                          {capability(a)}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        </div>

        <aside className="aside" aria-label={`${cur.name} details`}>
          <section>
            <Agent id={cur.id}>{cur.name}</Agent>
            <h2 className="h2-lg !text-[26px]">{cur.name === "claude-code" ? "Claude Code" : cur.name[0].toUpperCase() + cur.name.slice(1)}</h2>
            <p className="hint mt-3">
              {cur.adapter
                ? "Not located and never checked. Nothing about this agent is verified."
                : "No MCO adapter exists for this agent yet. Two integration options are documented, and neither is selected."}
            </p>
            <dl className="kv2 mt-6">
              <dt>Binary</dt>
              <dd>not located</dd>
              <dt>Verified</dt>
              <dd>0 of {CAPABILITIES.length}</dd>
              <dt>Advertised</dt>
              <dd>0</dd>
              <dt>Unknown</dt>
              <dd>{CAPABILITIES.length}</dd>
            </dl>
          </section>
          <section>
            <p className="t-label mb-2 flex items-center gap-2.5">
              What “check” does <Vision>planned</Vision>
            </p>
            <p className="hint ink2">
              Runs the CLI in a throwaway folder with each flag Synapse relies on, then records what actually happened. No project
              files are touched.
            </p>
            <div className="actions mt-3">
              <Button reason={NOT_CHECKED}>Check now</Button>
              <Button variant="ghost" reason={NOT_CHECKED}>
                Check log
              </Button>
            </div>
          </section>
        </aside>
      </div>
    </div>
  );
}
