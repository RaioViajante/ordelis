import { useState } from "react";
import { useSampleData } from "../app/SampleData";
import { MOCK_CONTEXT as C } from "../fixtures/mock";
import { Button, Empty, Evidence, PageTitle, SecHead, Vision, Select } from "../components/ui";

const NO_VAULT = "Vault access isn't built yet. This build never opens your notes.";

const LEDE =
  "Where task context comes from. An Obsidian vault is optional, per project; when linked it’s read-only, only the central agent opens it, and sessions receive just the excerpts you approve.";

export function Context() {
  const { on } = useSampleData();
  const [history, setHistory] = useState(true);
  const [query, setQuery] = useState("reused refresh token");

  return (
    <div className="col col--1300">
      <PageTitle lede={LEDE}>Context</PageTitle>

      <section>
        <SecHead title="Where context comes from" meta="kept separate, always labelled" />
        <div className="src3">
          <div>
            <span className="prov prov--you">your notes</span>
            {on ? (
              <>
                <span className="text-[15px]">
                  Obsidian vault <span className="mono !text-[13px] ink2">{C.vaultName}</span>
                </span>
                <span className="hint !text-[13.5px]">Written by you. Synapse reads it and never changes it.</span>
                <span className="mono meta">{C.notes}</span>
              </>
            ) : (
              <>
                <span className="text-[15px]">No vault connected</span>
                <span className="hint !text-[13.5px]">Written by you. Synapse reads it and never changes it.</span>
                <span className="mono meta">0 notes</span>
              </>
            )}
          </div>
          <div>
            <span className="prov prov--hist">private history</span>
            <span className="text-[15px]">What happened in past tasks</span>
            <span className="hint !text-[13.5px]">
              Plans, decisions, session logs. Stored on this machine, outside the vault and the Synapse repo.
            </span>
            {on ? (
              <label className="flex cursor-pointer items-center gap-3">
                <input
                  className="switch"
                  type="checkbox"
                  checked={history}
                  onChange={(e) => setHistory(e.target.checked)}
                />
                <span className="text-[13.5px]">Offer past decisions as context</span>
              </label>
            ) : (
              <span className="mono meta">no history stored yet</span>
            )}
          </div>
          <div>
            <span className="prov prov--sug">pending suggestion</span>
            <span className="text-[15px]">{on ? "3 things Synapse thinks are worth keeping" : "Nothing pending"}</span>
            <span className="hint !text-[13.5px]">
              Drafts only. Nothing reaches your vault until you copy or accept it, note by note.
            </span>
            <Button small reason="Suggestions need task history, which doesn't exist yet.">
              Review suggestions
            </Button>
          </div>
        </div>
      </section>

      {!on ? (
        <Empty
          title="No vault connected"
          why="vault: none · history: none"
          actions={<Button reason={NO_VAULT}>Choose vault folder…</Button>}
        >
          Context comes from the repository, files you attach and your request. Linking an Obsidian vault is optional, and it
          stays read-only.
        </Empty>
      ) : (
        <div className="split">
          <section className="min-w-0">
            <SecHead title="Try a search" meta="exactly what a task would receive" />
            <div className="search">
              <label className="sr" htmlFor="q">
                Search the vault
              </label>
              <input id="q" value={query} onChange={(e) => setQuery(e.target.value)} />
              <Button reason="Search reads the vault, which this build can't open.">Search</Button>
            </div>
            <div className="border-t border-line-100">
              {C.results.map((r) => (
                <div className="res" key={r.where}>
                  <span>
                    <span className="prov prov--you">{r.src}</span> <span className="ml-2">{r.where}</span>
                    {r.sub && <span className="ink2"> · {r.sub}</span>}
                  </span>
                  <span className="mono meta">
                    {r.tok}
                    {r.trimmed && (
                      <>
                        {" · "}
                        <span className="ink2">trimmed</span>
                      </>
                    )}
                  </span>
                  <p>{r.text}</p>
                </div>
              ))}
              {history && (
                <div className="res">
                  <span>
                    <span className="prov prov--hist">private history</span> <span className="ml-2">{C.history.where}</span>
                  </span>
                  <span className="mono meta">{C.history.tok}</span>
                  <p>{C.history.text}</p>
                </div>
              )}
            </div>
          </section>

          <aside className="aside" aria-label="Vault settings">
            <section>
              <SecHead title="Connection" />
              <dl className="kv2 kv2--90">
                <dt>Path</dt>
                <dd>{C.path}</dd>
                <dt>Index</dt>
                <dd>{C.index}</dd>
                <dt>Stored</dt>
                <dd>{C.stored}</dd>
                <dt>Access</dt>
                <dd className="!font-sans">
                  <Evidence level="adv">read-only · by design</Evidence>
                </dd>
                <dt>Write</dt>
                <dd className="!font-sans">
                  <Vision>not in this version</Vision>
                </dd>
              </dl>
              <div className="actions mt-3">
                <Button small reason="Indexing reads the vault, which this build can't open.">
                  Re-index
                </Button>
              </div>
            </section>
            <section>
              <SecHead title="Limits" />
              <div className="rows">
                <div className="r">
                  <label htmlFor="l1" className="flex-1 text-sm">
                    Context per task
                  </label>
                  <Select id="l1" className="field">
                    <option>6,000 tok</option>
                    <option>12,000 tok</option>
                  </Select>
                </div>
                <div className="r">
                  <label htmlFor="l2" className="flex-1 text-sm">
                    Per note section
                  </label>
                  <Select id="l2" className="field">
                    <option>1,200 tok</option>
                    <option>2,000 tok</option>
                  </Select>
                </div>
              </div>
              <p className="hint mt-2.5 !text-[13px]">Trimmed sections always show where the cut happened.</p>
            </section>
            <div className="rows">
              <div className="r">
                <span className="flex-1">Workflows</span>
                <Vision>later</Vision>
              </div>
            </div>
          </aside>
        </div>
      )}
    </div>
  );
}
