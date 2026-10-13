import { useEffect } from "react";
import { useSampleData } from "../app/SampleData";
import { hrefFor, type Location } from "../app/routes";
import { useWorkspace } from "../app/Workspace";
import { findProject, MOCK_PROJECTS } from "../fixtures/mock";
import { Button, Empty, Evidence, SecHead } from "../components/ui";

const NO_FOLDERS = "Folder access isn't built yet. This build never reads your filesystem.";

export function Projects({ param, go }: { param?: string; go: (page: Location["page"], param?: string) => void }) {
  const { on } = useSampleData();
  const { project, selectProject } = useWorkspace();
  // A link such as #/projects/atlas-api asks for that project; the shared selection stays the single source of truth.
  const requested = param === undefined ? undefined : findProject(param);
  const cur = findProject(requested?.name ?? project);

  useEffect(() => {
    if (on && requested) selectProject(requested.name);
  }, [on, requested, selectProject]);

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
      ) : param !== undefined && !requested ? (
        <Empty
          title="Project not found"
          why={`no sample project is named “${param}”`}
          actions={
            <a className="btn" href={hrefFor("projects")}>
              Back to projects
            </a>
          }
        >
          This link points at a project that isn’t in the sample data. Pick one from the list instead.
        </Empty>
      ) : (
        <div className="lay">
          <div className="flex flex-col gap-0.5" role="group" aria-label="Projects">
            {MOCK_PROJECTS.map((p) => (
              <button
                type="button"
                key={p.name}
                className={`pj${p.name === cur?.name ? " on" : ""}`}
                aria-pressed={p.name === cur?.name}
                onClick={() => {
                  selectProject(p.name);
                  if (param !== undefined) go("projects");
                }}
              >
                <span className="font-medium text-[15.5px] leading-[22px] text-ink-100">{p.name}</span>
                <span className="mono !text-[13px] text-ink-300">{p.path}</span>
                <span className="meta">{p.line}</span>
              </button>
            ))}
          </div>

          {!cur ? (
            <Empty title="No project selected" why="choose one from the list to see its details">
              Sample projects are listed on the left. The one you choose here is also the one Home will use for your draft.
            </Empty>
          ) : (
          <section className="flex min-w-0 flex-col gap-8">
            <div className="flex flex-wrap items-baseline gap-4">
              <h2 className="h2-lg">{cur.name}</h2>
              <span className="mono meta">{cur.path}</span>
              <span className="sp" />
              <Button small variant="go" onClick={() => go("home")}>
                Draft task here
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
                        <a className="mono link" href={hrefFor("sessions", w.id)} aria-label={`Sample session ${w.id}`}>
                          {w.id}
                        </a>
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
                <SecHead title="Context" meta={cur.vault ? "sample" : undefined} />
                {cur.vault ? (
                  <>
                    <dl className="kv2">
                      <dt>Vault</dt>
                      <dd>
                        {cur.vault.name} <span className="ink2 !font-sans">· sample, not connected</span>
                      </dd>
                      <dt>Sample notes</dt>
                      <dd>{cur.vault.notes}</dd>
                      <dt>Access</dt>
                      <dd className="!font-sans">
                        <Evidence level="unk">none · no vault is connected</Evidence>
                      </dd>
                    </dl>
                    <p className="hint mt-2.5 !text-[13px]">
                      Invented for layout review. No Obsidian vault exists or was indexed; see Context for the sample entries.
                    </p>
                  </>
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
                    <a className="r" key={t.id} href={hrefFor("task", t.id)}>
                      <span className="min-w-0 flex-1 text-[15px]">{t.name}</span>
                      <span className={`st st--${t.tone}`}>{t.state}</span>
                    </a>
                  ))}
                </div>
              </section>
            </div>
          </section>
          )}
        </div>
      )}
    </div>
  );
}
