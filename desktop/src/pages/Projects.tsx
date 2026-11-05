import { useEffect, useRef, useState } from "react";
import { ConfirmDialog } from "../components/ConfirmDialog";
import { describeRemove, errorText, useAddFolder, useRegistry } from "../app/Registry";
import type { Project, ProjectStatus } from "../app/projects";
import { useSampleData } from "../app/SampleData";
import { taskErrorText, useTasks } from "../app/TaskState";
import { hrefFor, type Location } from "../app/routes";
import { belongsTo } from "../app/tasks";
import { useWorkspace } from "../app/Workspace";
import { findProject, MOCK_PROJECTS, type MockProject } from "../fixtures/mock";
import { byUpdated, draftsText, TaskRow, useTaskCounts } from "../components/TaskBits";
import { Button, Empty, Evidence, SecHead } from "../components/ui";

type Go = (page: Location["page"], param?: string) => void;

export const STATUS_LABEL: Record<ProjectStatus, { tone: "done" | "fail" | "you"; text: string }> = {
  available: { tone: "done", text: "Available" },
  missing: { tone: "fail", text: "Folder not found" },
  inaccessible: { tone: "fail", text: "Can't access folder" },
  changed: { tone: "you", text: "Different folder at this path" },
};

const STATUS_HELP: Record<ProjectStatus, string> = {
  available: "",
  missing: "Nothing was found at the saved location. The folder may have been moved, renamed or deleted. Remove this entry, then add the folder again from its new place.",
  inaccessible: "Ordelis cannot access this location right now. Check that the folder and its drive are available.",
  changed: "A different folder is now at the saved location, so Ordelis will not treat it as this project. Remove this entry, then add the folder again if it is the one you want.",
};

export const registeredDate = (ms: number) => new Date(ms).toLocaleDateString(undefined, { dateStyle: "medium" });

export function StatusText({ status }: { status: ProjectStatus }) {
  const s = STATUS_LABEL[status];
  return <span className={`st st--${s.tone}`}>{s.text}</span>;
}

export function Projects({ param, go }: { param?: string; go: Go }) {
  const { on } = useSampleData();
  const { project, selectProject } = useWorkspace();
  const reg = useRegistry();
  const { message, setMessage, reason: addReason, add, adding, id: addId } = useAddFolder((p) => go("projects", p.id));
  const [confirming, setConfirming] = useState<Project | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const removeRef = useRef<HTMLButtonElement>(null);

  // A link such as #/projects/p3 asks for a registered folder, and #/projects/atlas-api for a sample project.
  // Registered folders are identified by their registry id, never by name.
  const registered = param === undefined ? undefined : reg.projects.find((p) => p.id === param);
  const requested = param === undefined || !on ? undefined : findProject(param);
  const curReg = registered ?? (param === undefined ? reg.projects[0] : undefined);
  const curSample = curReg || !on ? undefined : findProject(requested?.name ?? project);
  // The page was opened on a registered id before the list arrived.
  const waiting = param !== undefined && /^p[1-9][0-9]*$/.test(param) && reg.loading;

  useEffect(() => {
    if (on && requested) selectProject(requested.name);
  }, [on, requested, selectProject]);

  async function confirmRemove(p: Project) {
    const result = await reg.remove(p.id);
    setMessage(describeRemove(result, p.name));
    setConfirming(null);
    if (result.kind === "removed" || result.kind === "not_found") {
      go("projects");
      // The shell moves focus to the page after a route change; the list is where the removed entry was.
      requestAnimationFrame(() => listRef.current?.focus());
    } else {
      removeRef.current?.focus();
    }
  }

  const nothing = !reg.loading && reg.projects.length === 0 && !on && !reg.listError;

  return (
    <div className="col col--1300">
      <div className="flex items-end gap-5">
        <h1 className="h1">Projects</h1>
        <span className="sp" />
        <Button id={addId} variant="go" reason={addReason} onClick={add}>
          {adding ? "Choose a folder…" : "Add a folder…"}
        </Button>
      </div>
      <p className="hint max-w-[760px]">
        Adding a folder saves its location on this Mac, nothing more. Ordelis does not read its files, run agents, use Git or
        open a terminal there.
      </p>

      {message && (
        <div className={`notice ${message.tone === "ok" ? "notice--quiet" : ""}`} role={message.tone === "error" ? "alert" : "note"}>
          <p>{message.text}</p>
        </div>
      )}

      {reg.listError && (
        <div className="notice" role="alert">
          <p>{errorText(reg.listError)}</p>
          <span className="sp" />
          <Button small onClick={() => void reg.reload()}>
            Try again
          </Button>
        </div>
      )}

      {nothing ? (
        <Empty
          title="No projects yet"
          why={reg.desktop ? "projects: 0 · nothing has been scanned" : "browser preview · no native access"}
        >
          Projects are folders you add. {reg.desktop ? "Choose one with Add a folder." : "Adding folders needs the desktop app."}
        </Empty>
      ) : waiting ? (
        <p className="meta" role="status">
          Loading your folders…
        </p>
      ) : param !== undefined && !registered && !requested ? (
        <Empty
          title="Project not found"
          why={`no project matches “${param}”`}
          actions={
            <a className="btn" href={hrefFor("projects")}>
              Back to projects
            </a>
          }
        >
          This link points at a project that isn’t in your folders{on ? " or the sample data" : ""}. Pick one from the list instead.
        </Empty>
      ) : (
        <div className="lay">
          <div className="flex flex-col gap-0.5">
            <div ref={listRef} tabIndex={-1} className="flex flex-col gap-0.5" role="group" aria-label="Your folders">
              <div className="pj-grp">Your folders</div>
              {reg.loading && (
                <p className="meta px-3 py-2" role="status">
                  Loading your folders…
                </p>
              )}
              {!reg.loading && reg.projects.length === 0 && (
                <p className="meta px-3 py-2">{reg.desktop ? "None yet. Add a folder to see it here." : "Adding folders needs the desktop app."}</p>
              )}
              {reg.projects.map((p) => (
                <button
                  type="button"
                  key={p.id}
                  className={`pj${p.id === curReg?.id ? " on" : ""}`}
                  aria-pressed={p.id === curReg?.id}
                  onClick={() => go("projects", p.id)}
                >
                  <span className="bidi font-medium text-[15.5px] leading-[22px] text-ink-100" dir="auto">
                    {p.name}
                  </span>
                  <span className="bidi mono !text-[13px] text-ink-300" dir="auto">
                    {p.path_display}
                  </span>
                  <StatusText status={p.status} />
                </button>
              ))}
            </div>
            {on && (
              <div className="flex flex-col gap-0.5" role="group" aria-label="Sample projects">
                <div className="pj-grp mt-4">
                  Sample projects <span className="meta">· sample data</span>
                </div>
                {MOCK_PROJECTS.map((p) => (
                  <button
                    type="button"
                    key={p.name}
                    className={`pj${p.name === curSample?.name ? " on" : ""}`}
                    aria-pressed={p.name === curSample?.name}
                    onClick={() => {
                      selectProject(p.name);
                      go("projects", p.name);
                    }}
                  >
                    <span className="font-medium text-[15.5px] leading-[22px] text-ink-100">{p.name}</span>
                    <span className="mono !text-[13px] text-ink-300">{p.path}</span>
                    <span className="meta">{p.line}</span>
                  </button>
                ))}
              </div>
            )}
          </div>

          {curReg ? (
            <RegisteredDetail
              p={curReg}
              removing={reg.removing === curReg.id}
              blocked={reg.adding || reg.removing !== null || reg.listError !== null}
              removeRef={removeRef}
              onRemove={() => setConfirming(curReg)}
              go={go}
            />
          ) : curSample ? (
            <SampleDetail cur={curSample} />
          ) : (
            <Empty title="No project selected" why="choose one from the list to see its details">
              Pick a folder or a sample project on the left.
            </Empty>
          )}
        </div>
      )}

      {confirming && (
        <ConfirmDialog
          title={`Remove ${confirming.name} from Ordelis?`}
          confirmLabel="Remove"
          busy={reg.removing === confirming.id}
          onCancel={() => {
            setConfirming(null);
            removeRef.current?.focus();
          }}
          onConfirm={() => void confirmRemove(confirming)}
        >
          <p>
            Ordelis forgets this folder. The folder and its files stay exactly where they are; nothing is deleted or changed.
            You can add it again later.
          </p>
          <p className="bidi mono meta" dir="auto">
            {confirming.path_display}
          </p>
        </ConfirmDialog>
      )}
    </div>
  );
}

function RegisteredDetail({
  p,
  removing,
  blocked,
  removeRef,
  onRemove,
  go,
}: {
  p: Project;
  removing: boolean;
  blocked: boolean;
  removeRef: React.RefObject<HTMLButtonElement | null>;
  onRemove: () => void;
  go: Go;
}) {
  const tasks = useTasks();
  const { announce } = useWorkspace();
  // Preselects this registration in the shared draft; the text already written is kept, and New task checks the
  // project again before saving.
  function newTaskHere() {
    tasks.setDraft({ ...tasks.draft, project: { id: p.id, registered_at_ms: p.registered_at_ms } });
    announce(`New task in ${p.name}.`);
    go("tasks", "new");
  }
  return (
    <section className="flex min-w-0 flex-col gap-8" aria-label={`${p.name} details`}>
      <div className="flex flex-wrap items-baseline gap-4">
        <h2 className="h2-lg bidi" dir="auto">
          {p.name}
        </h2>
        <span className="sp" />
        <Button small variant="go" onClick={newTaskHere} reason={tasks.saving ? "Saving…" : undefined}>
          New task in this project
        </Button>
        <button
          type="button"
          ref={removeRef}
          className="btn btn--sm"
          aria-disabled={blocked || undefined}
          onClick={() => {
            if (!blocked) onRemove();
          }}
        >
          {removing ? "Removing…" : "Remove from Ordelis…"}
        </button>
      </div>
      <dl className="kv2">
        <dt>Location</dt>
        <dd className="bidi" dir="auto">
          {p.path_display}
        </dd>
        <dt>Status</dt>
        <dd className="!font-sans">
          <StatusText status={p.status} />
        </dd>
        <dt>Added</dt>
        <dd className="!font-sans">{registeredDate(p.registered_at_ms)}</dd>
      </dl>
      {p.status !== "available" && <p className="notice">{STATUS_HELP[p.status]}</p>}
      <ProjectTasks p={p} />
      <p className="hint max-w-[640px]">
        Registered only. Ordelis has not read this folder, and Git, terminals and agents are not connected to it. Removing the
        entry never deletes or changes anything inside the folder.
      </p>
    </section>
  );
}

// The tasks saved for this registration: same id and registration time, and linked when Rust listed them. Tasks saved
// for a removed or earlier registration with this id are not shown or counted here (they stay under Tasks).
function ProjectTasks({ p }: { p: Project }) {
  const { tasks, listError, loading, reload } = useTasks();
  const counts = useTaskCounts();
  const mine = tasks.filter((t) => belongsTo(t, p)).sort(byUpdated);
  const drafts = mine.filter((t) => t.status === "draft");
  const archived = mine.filter((t) => t.status === "archived");
  const c = counts?.(p);
  return (
    <section aria-labelledby={`${p.id}-tasks`} className="flex flex-col gap-3">
      <SecHead
        title={<span id={`${p.id}-tasks`}>Tasks</span>}
        meta={c ? `${draftsText(c.drafts)}${c.archived ? ` · ${c.archived} archived` : ""}` : undefined}
      />
      {listError ? (
        <div className="notice" role="alert">
          <p>{taskErrorText(listError)}</p>
          <span className="sp" />
          <Button small onClick={() => void reload()}>
            Try again
          </Button>
        </div>
      ) : loading ? (
        <p className="meta" role="status">
          Loading tasks…
        </p>
      ) : mine.length === 0 ? (
        <p className="meta">No tasks saved for this project yet.</p>
      ) : (
        <>
          {drafts.length > 0 && (
            <ul className="tlist card" aria-label={`Drafts in ${p.name}`}>
              {drafts.map((t) => (
                <TaskRow key={t.id} task={t} inProject />
              ))}
            </ul>
          )}
          {archived.length > 0 && (
            <ul className="tlist card" aria-label={`Archived tasks in ${p.name}`}>
              {archived.map((t) => (
                <TaskRow key={t.id} task={t} inProject />
              ))}
            </ul>
          )}
        </>
      )}
    </section>
  );
}

function SampleDetail({ cur }: { cur: MockProject }) {
  return (
    <section className="flex min-w-0 flex-col gap-8">
      <div className="flex flex-wrap items-baseline gap-4">
        <h2 className="h2-lg">{cur.name}</h2>
        <span className="mono meta">{cur.path}</span>
        <span className="sp" />
        <Button small variant="go" reason="Saved tasks belong to your own folders. Add a folder to write a task for it.">
          New task in this project
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
          <SecHead title="Workspace" meta="sample" />
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
  );
}
