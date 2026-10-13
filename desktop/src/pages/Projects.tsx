import { useState } from "react";
import { useSampleData } from "../app/SampleData";
import { MOCK_PROJECTS } from "../fixtures/mock";
import { Button, Empty, Evidence, SecHead } from "../components/ui";

const NO_FOLDERS = "Folder access isn't built yet. This build never reads your filesystem.";

export function Projects() {
  const { on } = useSampleData();
  const [picked, setPicked] = useState(0);
  const cur = MOCK_PROJECTS[picked];

  return (
    <div className="col col--1300">
      <div className="flex items-end gap-5">
        <h1 className="h1">Projects</h1>
        <span className="sp" />
        <Button reason={NO_FOLDERS}>Add a folder…</Button>
      </div>

      {!on ? (
        <Empty title="No projects yet" why="projects: 0 · nothing has been scanned">
          Projects are folders you add, each with its own workspace and optional vault. Adding one will be possible once
          folder access is built.
        </Empty>
      ) : (
        <div className="lay">
          <div className="flex flex-col gap-0.5" role="group" aria-label="Projects">
            {MOCK_PROJECTS.map((p, i) => (
              <button
                type="button"
                key={p.name}
                className={`pj${i === picked ? " on" : ""}`}
                aria-pressed={i === picked}
                onClick={() => setPicked(i)}
              >
                <span className="font-medium text-[15.5px] leading-[22px] text-ink-100">{p.name}</span>
                <span className="mono !text-[13px] text-ink-300">{p.path}</span>
                <span className="meta">{p.line}</span>
              </button>
            ))}
          </div>

          <section className="flex min-w-0 flex-col gap-8">
            <div className="flex flex-wrap items-baseline gap-4">
              <h2 className="h2-lg">{cur.name}</h2>
              <span className="mono meta">{cur.path}</span>
              <span className="sp" />
              <Button small variant="go" reason="Task planning isn't connected to the runtime yet.">
                New task here
              </Button>
            </div>

            <div className="stages">
              {cur.stages.map((s) => (
                <div key={s.label} className={`stage-i ${s.tone}`}>
                  <b>{s.label}</b>
                  <span>{s.text}</span>
                </div>
              ))}
            </div>

            <div className="grid2">
              <section className="min-w-0">
                <SecHead title="Workspace" />
                <dl className="kv2">
                  <dt>Repository</dt>
                  <dd>{cur.repo}</dd>
                  <dt>Main branch</dt>
                  <dd>{cur.head}</dd>
                  <dt>Uncommitted</dt>
                  <dd>{cur.dirty}</dd>
                  <dt>Worktrees</dt>
                  <dd>{cur.worktrees}</dd>
                </dl>
                {cur.wts.length > 0 && (
                  <div className="mt-[18px] border-t border-line-100">
                    {cur.wts.map((w) => (
                      <div className="wt" key={w.id}>
                        <span className="mono">{w.id}</span>
                        <span className="min-w-0">
                          {w.task} <span className="meta block">{w.agent}</span>
                        </span>
                        <span className={`st st--${w.tone}`}>{w.state}</span>
                      </div>
                    ))}
                  </div>
                )}
              </section>

              <section className="min-w-0">
                <SecHead title="Context" />
                {cur.vault ? (
                  <dl className="kv2">
                    <dt>Vault</dt>
                    <dd>{cur.vault.name}</dd>
                    <dt>Linked notes</dt>
                    <dd>{cur.vault.notes}</dd>
                    <dt>Access</dt>
                    <dd className="!font-sans">
                      <Evidence level="adv">read-only · by design, not checked</Evidence>
                    </dd>
                  </dl>
                ) : (
                  <Empty small title="No vault for this project">
                    That’s fine. Tasks here use the repository, files you attach and what you write in the request. A vault is
                    optional.
                  </Empty>
                )}
                <div className="mt-[26px]">
                  <SecHead title="Recent tasks" />
                </div>
                <div className="rows">
                  {cur.tasks.length === 0 && <div className="r meta">No tasks yet.</div>}
                  {cur.tasks.map((t) => (
                    <div className="r" key={t.name}>
                      <span className="min-w-0 flex-1 text-[15px]">{t.name}</span>
                      <span className={`st st--${t.tone}`}>{t.state}</span>
                    </div>
                  ))}
                </div>
              </section>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}
