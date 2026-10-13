import { useState } from "react";
import { countText, EXAMPLE_BUDGET_EST_TOKENS, EXCERPT_LIMIT_CHARS, excerptOf, fmt, sumCounts } from "../app/contextBudget";
import { type Location } from "../app/routes";
import { useSampleData } from "../app/SampleData";
import { useWorkspace } from "../app/Workspace";
import { entriesFor, isSelectable, MOCK_PROJECTS, SOURCE_LABEL, type ContextEntry, type ContextSource } from "../fixtures/mock";
import { Button, Empty, PageTitle, SecHead, Select, Vision } from "../components/ui";

const LEDE =
  "Where task context would come from. Everything on this page is a synthetic sample: no vault is connected, no history exists and nothing is read from your machine.";

const SOURCES: { id: ContextSource; heading: string; blurb: string; prov: string }[] = [
  { id: "notes", heading: "Sample notes", blurb: "Selectable. Invented to stand in for optional Obsidian notes.", prov: "prov--you" },
  { id: "history", heading: "Sample private history", blurb: "Selectable while past decisions are offered. Invented.", prov: "prov--hist" },
  { id: "suggestion", heading: "Sample pending suggestions", blurb: "Read-only drafts. They cannot be selected as context.", prov: "prov--sug" },
];

const SUGGESTION_REASON = "A draft is not context until it is accepted, and accepting isn't built.";

function matches(e: ContextEntry, query: string) {
  const hay = `${e.title} ${e.where} ${e.text}`.toLowerCase();
  return query
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
    .every((term) => hay.includes(term));
}

export function Context({ go }: { go: (page: Location["page"], param?: string) => void }) {
  const { on, setOn } = useSampleData();
  const {
    project, selectProject, draft, selectable, contextEntries, toggleContext, clearContext, historyOffered, setHistoryOffered,
  } = useWorkspace();
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState<string[]>([]);

  const entries = on ? entriesFor(project) : [];
  const shown = entries.filter((e) => (historyOffered || e.source !== "history") && matches(e, query));
  const total = sumCounts(contextEntries.map((e) => countText(excerptOf(e.text).shown)));
  const count = (s: ContextSource) => entries.filter((e) => e.source === s).length;
  const blocker = !on
    ? "Sample data is hidden."
    : !project
      ? "Choose a project first."
      : !draft.trim()
        ? "Describe the task on Home first."
        : undefined;

  return (
    <div className="col col--1300">
      <PageTitle lede={LEDE}>Context</PageTitle>

      <section>
        <SecHead title="Where context comes from" meta="kept separate, always labelled" />
        <div className="src3">
          <div>
            <span className="prov prov--you">your notes</span>
            <span className="text-[15px]">Sample notes</span>
            <span className="hint !text-[13.5px]">
              No Obsidian vault is connected. These notes are invented and never read from disk.
            </span>
            <span className="mono meta">{on ? `${count("notes")} sample ${count("notes") === 1 ? "note" : "notes"} · ${project ?? "no project"}` : "0 notes · sample data hidden"}</span>
          </div>
          <div>
            <span className="prov prov--hist">private history</span>
            <span className="text-[15px]">Sample past decisions</span>
            <span className="hint !text-[13.5px]">
              No task has recorded anything. These decisions are invented for this UI.
            </span>
            {on ? (
              <label className="flex cursor-pointer items-center gap-3">
                <input
                  className="switch"
                  type="checkbox"
                  checked={historyOffered}
                  onChange={(e) => setHistoryOffered(e.target.checked)}
                />
                <span className="text-[13.5px]">Offer past decisions as context</span>
              </label>
            ) : (
              <span className="mono meta">no history · sample data hidden</span>
            )}
          </div>
          <div>
            <span className="prov prov--sug">pending suggestion</span>
            <span className="text-[15px]">{on ? `${count("suggestion")} sample drafts` : "No drafts"}</span>
            <span className="hint !text-[13.5px]">
              Drafts only, written by hand for this sample. Nothing reaches a vault, and drafts can’t be selected as context.
            </span>
            <Button small reason="Accepting suggestions isn't built. Sample drafts are read-only.">
              Review suggestions
            </Button>
          </div>
        </div>
      </section>

      {!on ? (
        <Empty
          title="Sample context is hidden"
          why="sample entries: 0 · selection: none"
          actions={<Button onClick={() => setOn(true)}>Show sample data</Button>}
        >
          There is nothing to search or select while sample data is hidden. No vault or history is connected in this build.
        </Empty>
      ) : (
        <div className="split">
          <section className="min-w-0">
            <SecHead title="Search sample context" meta={`${contextEntries.length} selected`} />
            <div className="ctx-proj">
              <label className="t-label" htmlFor="ctx-project">
                Project
              </label>
              <Select
                id="ctx-project"
                className="field"
                value={project ?? ""}
                onChange={(e) => selectProject(e.target.value || null)}
              >
                <option value="">Choose a project…</option>
                {MOCK_PROJECTS.map((p) => (
                  <option key={p.name}>{p.name}</option>
                ))}
              </Select>
            </div>
            <div className="search">
              <label className="sr" htmlFor="q">
                Search sample context
              </label>
              <input
                id="q"
                type="search"
                value={query}
                placeholder="Search titles and text, e.g. refresh token"
                onChange={(e) => setQuery(e.target.value)}
              />
            </div>
            <p className="mono meta mb-2" aria-live="polite">
              {project ? `${shown.length} of ${entries.length} sample entries · ${project}` : "no project selected"}
            </p>

            {!project ? (
              <Empty small title="No project selected" why="sample context is scoped to one project">
                Choose a project to search its sample context.
              </Empty>
            ) : entries.length === 0 ? (
              <Empty small title={`No sample context for ${project}`} why="sample entries: 0 for this project">
                The sample data has no notes, history or drafts for this project.
              </Empty>
            ) : shown.length === 0 ? (
              <Empty small title="No results" why={`“${query.trim()}” matched 0 of ${entries.length} sample entries`}>
                {historyOffered ? "Try fewer words." : "Try fewer words, or offer past decisions as context to include them."}
              </Empty>
            ) : (
              SOURCES.map((src) => {
                const group = shown.filter((e) => e.source === src.id);
                if (!group.length) return null;
                return (
                  <section key={src.id} aria-labelledby={`grp-${src.id}`} className="ctx-group">
                    <h3 id={`grp-${src.id}`} className="ctx-group__h">
                      <span className={`prov ${src.prov}`}>{SOURCE_LABEL[src.id]}</span>
                      <span>{src.heading}</span>
                      <span className="meta">{group.length}</span>
                    </h3>
                    <p className="hint !text-[13px]">{src.blurb}</p>
                    <ul className="ctx-list">
                      {group.map((e) => {
                        const ex = excerptOf(e.text);
                        const isOpen = open.includes(e.id);
                        const picked = contextEntries.some((c) => c.id === e.id);
                        return (
                          <li key={e.id} className="ctx-row">
                            <div className="ctx-row__head">
                              {isSelectable(e) ? (
                                <input
                                  type="checkbox"
                                  aria-label={`Select ${e.title}`}
                                  checked={picked}
                                  onChange={() => toggleContext(e.id)}
                                  disabled={!selectable.some((s) => s.id === e.id)}
                                />
                              ) : (
                                <span className="ctx-row__none" aria-hidden="true">
                                  ○
                                </span>
                              )}
                              <span className="ctx-row__title">
                                {e.title}
                                <span className="ink2"> · {e.where}</span>
                              </span>
                              {e.unavailable ? (
                                <span className="mono meta">unavailable</span>
                              ) : (
                                <>
                                  <span className="mono meta">~{fmt(countText(ex.shown).estTokens)} est. tok</span>
                                  <Button
                                    small
                                    variant="ghost"
                                    aria-expanded={isOpen}
                                    aria-controls={`ex-${e.id}`}
                                    aria-label={`Excerpt of ${e.title}`}
                                    onClick={() => setOpen(isOpen ? open.filter((o) => o !== e.id) : [...open, e.id])}
                                  >
                                    {isOpen ? "Close" : "Excerpt"}
                                  </Button>
                                </>
                              )}
                            </div>
                            <p className="hint !text-[13px] ctx-row__why">
                              {e.unavailable
                                ? `Not selectable: ${e.unavailable}.`
                                : e.source === "suggestion"
                                  ? `Not selectable: ${SUGGESTION_REASON}`
                                  : picked
                                    ? "Selected as sample context"
                                    : "Not selected"}
                            </p>
                            {isOpen && !e.unavailable && (
                              <div id={`ex-${e.id}`} role="region" aria-label={`Excerpt of ${e.title}, sample`} className="ctx-ex">
                                <p className="preview-text">{ex.shown}</p>
                                {ex.omittedChars > 0 && (
                                  <p className="mono meta">
                                    — cut here · {fmt(ex.omittedChars)} chars not shown (example limit {EXCERPT_LIMIT_CHARS}) —
                                  </p>
                                )}
                                <dl className="kv2 kv2--90">
                                  <dt>Provenance</dt>
                                  <dd className="!font-sans">{e.provenance}</dd>
                                  <dt>Counts</dt>
                                  <dd>
                                    {fmt(countText(ex.shown).chars)} chars · {fmt(countText(ex.shown).bytes)} bytes · ~
                                    {fmt(countText(ex.shown).estTokens)} est. tok
                                  </dd>
                                </dl>
                              </div>
                            )}
                          </li>
                        );
                      })}
                    </ul>
                  </section>
                );
              })
            )}
          </section>

          <aside className="aside" aria-label="Selection and sample source">
            <section aria-labelledby="sel-h">
              <SecHead title={<span id="sel-h">Selection</span>} meta="sample · preview only" />
              <p className="text-[15px]">
                {contextEntries.length
                  ? `${contextEntries.length} sample ${contextEntries.length === 1 ? "entry" : "entries"} selected`
                  : "No sample context selected"}
              </p>
              <progress
                className="budget"
                max={EXAMPLE_BUDGET_EST_TOKENS}
                value={Math.min(total.estTokens, EXAMPLE_BUDGET_EST_TOKENS)}
                aria-label={`Estimated tokens against the ${fmt(EXAMPLE_BUDGET_EST_TOKENS)} token example budget`}
              />
              <dl className="kv2 kv2--90 mt-2">
                <dt>Characters</dt>
                <dd>{fmt(total.chars)}</dd>
                <dt>Bytes</dt>
                <dd>{fmt(total.bytes)} UTF-8</dd>
                <dt>Est. tokens</dt>
                <dd>
                  ~{fmt(total.estTokens)} of {fmt(EXAMPLE_BUDGET_EST_TOKENS)} example
                </dd>
              </dl>
              <p className="hint mt-2.5 !text-[13px]">
                Tokens are estimated as characters ÷ 4, rounded up. Nothing was measured by a provider, and the budget is an
                illustration, not an enforced limit.
              </p>
              <div className="actions mt-3">
                <Button
                  small
                  onClick={clearContext}
                  reason={contextEntries.length ? undefined : "Nothing is selected."}
                >
                  Clear selection
                </Button>
                <Button small variant="go" reason={blocker} onClick={() => go("preview")}>
                  Preview task
                </Button>
              </div>
            </section>
            <section>
              <SecHead title="Sample source" />
              <dl className="kv2 kv2--90">
                <dt>Source</dt>
                <dd className="!font-sans">synthetic fixture</dd>
                <dt>Vault</dt>
                <dd className="!font-sans">none connected</dd>
                <dt>Access</dt>
                <dd className="!font-sans">none in this build</dd>
                <dt>Write</dt>
                <dd className="!font-sans">
                  <Vision>not in this version</Vision>
                </dd>
              </dl>
              <p className="hint mt-2.5 !text-[13px]">
                Excerpts are cut at an example limit of {EXCERPT_LIMIT_CHARS} characters, and a cut always shows where it happened.
              </p>
            </section>
          </aside>
        </div>
      )}
    </div>
  );
}
