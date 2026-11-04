import { useRef } from "react";
import { useAddFolder, useRegistry } from "../app/Registry";
import { useSampleData } from "../app/SampleData";
import { taskErrorText, useTasks } from "../app/TaskState";
import { hrefFor, type Location } from "../app/routes";
import { useComposer } from "../app/useComposer";
import { MOCK_TASKS, MOCK_WAITING } from "../fixtures/mock";
import { DiscardDraft, ProjectProblem, ProjectSelect } from "../components/Composer";
import { byUpdated, draftsText, TaskRow, useTaskCounts } from "../components/TaskBits";
import { TaskFields } from "../components/TaskFields";
import { Agent, Button, SecHead } from "../components/ui";
import { StatusText } from "./Projects";

// Draft plan and context selection exist in the design but not in this build; saving a task starts nothing.
const NO_PLAN = "Planning isn't available yet. Saving a task starts nothing: no agent, no command, no file change.";
const NO_CONTEXT = "Choosing context for a task isn't available yet.";

export function Home({ go }: { go: (page: Location["page"], param?: string) => void }) {
  const { on } = useSampleData();
  return (
    <div className="col col--home">
      <Composer go={go} />
      <Drafts />
      <RecentProjects />
      {on ? <SampleSections /> : <FirstUse />}
    </div>
  );
}

// The design's composer, saving a real task (issue #98). It edits the same draft as New task, with the same rules.
function Composer({ go }: { go: (page: Location["page"], param?: string) => void }) {
  const composer = useComposer(go);
  const { draft } = composer;
  const form = useRef<HTMLFormElement>(null);
  return (
    <section className="flex max-w-[840px] flex-col gap-4" aria-labelledby="home-h">
      <h1 className="h1--home" id="home-h">
        What should we work on?
      </h1>
      <form ref={form} className="ask" aria-label="New task" onSubmit={(e) => void composer.save(e)} noValidate>
        <TaskFields
          variant="composer"
          title={draft.title}
          description={draft.description}
          onTitle={composer.setTitle}
          onDescription={composer.setDescription}
          problem={composer.problem}
          readOnly={composer.saving}
        />
        <div className="ask__bar">
          <label className="sr" htmlFor="home-project">
            Project
          </label>
          <ProjectSelect composer={composer} id="home-project" className="field" />
          <Button reason={NO_CONTEXT}>Context</Button>
          <span className="ask__actions">
            <DiscardDraft composer={composer} onDiscarded={() => form.current?.querySelector("input")?.focus()} />
            <Button reason={NO_PLAN}>Draft plan</Button>
            {composer.blocked ? (
              <Button variant="go" reason={composer.blocked}>
                {composer.saving ? "Saving…" : "Save task"}
              </Button>
            ) : (
              <button type="submit" className="btn btn--go">
                Save task
              </button>
            )}
          </span>
        </div>
        <div className="ask__problems">
          <ProjectProblem composer={composer} id="home-project" />
        </div>
      </form>
      {composer.failure && (
        <div className="notice" role="alert">
          <p>{composer.failure} Your text is still here.</p>
        </div>
      )}
      <p className="hint">
        Save task keeps it on this Mac as a draft; nothing is planned or run. The draft is shared with New task and stays
        while you move around Ordelis, until you save or discard it. It isn’t kept when Ordelis quits.
      </p>
    </section>
  );
}

// Recent saved drafts from the task store. Nothing here is sample data, and nothing claims a task is running.
function Drafts() {
  const { desktop, loading, tasks, listError, reload } = useTasks();
  if (!desktop) return null;
  const drafts = tasks.filter((t) => t.status === "draft").sort(byUpdated);
  const recent = drafts.slice(0, 5);
  return (
    <section aria-labelledby="home-drafts" className="flex flex-col gap-3">
      <SecHead title={<span id="home-drafts">Drafts</span>} meta="saved on this Mac">
        <a className="link meta ml-auto" href={hrefFor("tasks")}>
          All tasks
        </a>
      </SecHead>
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
          Loading your tasks…
        </p>
      ) : recent.length === 0 ? (
        <p className="meta">
          {tasks.length === 0 ? "No saved tasks yet. Write one above." : "No drafts. Archived tasks are under All tasks."}
        </p>
      ) : (
        <>
          <ul className="tlist card" aria-label="Recent drafts">
            {recent.map((task) => (
              <TaskRow key={task.id} task={task} />
            ))}
          </ul>
          {drafts.length > recent.length && (
            <a className="link meta" href={hrefFor("tasks")}>
              All {drafts.length} drafts
            </a>
          )}
        </>
      )}
    </section>
  );
}

function SampleSections() {
  return (
    <>
      <section>
        <SecHead title="Waiting on you" meta="sample">
          <span className="ml-auto">
            <Button variant="link" reason="The inbox arrives with task planning; there is nothing to open yet.">
              Open inbox →
            </Button>
          </span>
        </SecHead>
        <div className="border-t border-line-100">
          {MOCK_WAITING.map((w) => (
            <div className="wait" key={w.text}>
              <span className={`kind k-${w.kind === "approval" ? "appr" : w.kind === "question" ? "q" : "blk"}`}>{w.kind}</span>
              <Agent id={w.agent}>{w.agentName}</Agent>
              <span className="flex-1">{w.mono ? <span className="mono">{w.text}</span> : w.text}</span>
              <span className="meta">{w.project}</span>
            </div>
          ))}
        </div>
      </section>

      <section>
        <SecHead title="Active tasks" meta="across projects · sample" />
        <div className="border-t border-line-100">
          {MOCK_TASKS.map((t) => (
            <a className="task" key={t.id} href={hrefFor("task", t.id)}>
              <span>{t.title}</span>
              <span className="mono meta">{t.project}</span>
              <span className="flex gap-3">
                {t.agents.map((a) => (
                  <Agent key={a.id} id={a.id}>
                    {a.name}
                  </Agent>
                ))}
              </span>
              <span className={`st st--${t.tone}`}>{t.state}</span>
            </a>
          ))}
        </div>
      </section>
    </>
  );
}

function Result({ message }: { message: ReturnType<typeof useAddFolder>["message"] }) {
  if (!message) return null;
  return (
    <div className={`notice ${message.tone === "ok" ? "notice--quiet" : ""}`} role={message.tone === "error" ? "alert" : "note"}>
      <p>{message.text}</p>
    </div>
  );
}

// Folders the user has added, newest first. Real data from the project registry; in a browser preview there is none.
function RecentProjects() {
  const { desktop, loading, projects, listError } = useRegistry();
  const { message, reason, add, adding, id } = useAddFolder();
  const counts = useTaskCounts();
  if (!desktop) return null;
  const recent = [...projects].sort((a, b) => b.registered_at_ms - a.registered_at_ms).slice(0, 5);
  return (
    <section aria-label="Recent projects">
      <SecHead title="Recent projects" meta="your folders">
        <span className="ml-auto">
          <Button id={id} small reason={reason} onClick={add}>
            {adding ? "Choose a folder…" : "Add a folder…"}
          </Button>
        </span>
      </SecHead>
      <Result message={message} />
      <div className="rows">
        {loading && <div className="r meta">Loading your folders…</div>}
        {!loading && recent.length === 0 && !listError && (
          <div className="r meta">No folders yet. Add one to see it here. Ordelis saves only its location.</div>
        )}
        {recent.map((p) => (
          <a className="r" key={p.id} href={hrefFor("projects", p.id)}>
            <span className="bidi min-w-0 flex-1 text-[15px]" dir="auto">
              {p.name}
            </span>
            <span className="bidi mono meta" dir="auto">
              {p.path_display}
            </span>
            {/* Only a count that was read: none while tasks are loading or unreadable. */}
            {counts && counts(p).drafts > 0 && <span className="meta">{draftsText(counts(p).drafts)}</span>}
            <StatusText status={p.status} />
          </a>
        ))}
      </div>
      {projects.length > recent.length && (
        <a className="link meta" href={hrefFor("projects")}>
          All {projects.length} projects
        </a>
      )}
    </section>
  );
}

function FirstUse() {
  const { projects } = useRegistry();
  const { reason, add, adding, id } = useAddFolder();
  return (
    <section className="empty max-w-[840px]">
      <h3>Set up Ordelis</h3>
      <p>Three things before the first task. Only the first two are required.</p>
      <ul className="setup-list">
        <li>
          <span>
            Add a project folder<small>{projects.length ? `${projects.length} added` : "any folder on this Mac"}</small>
          </span>
        </li>
        <li>
          <span>
            Check at least one agent<small>nothing has been checked yet</small>
          </span>
        </li>
        <li>
          <span>
            Connect an Obsidian vault<small>optional · you can skip this</small>
          </span>
        </li>
      </ul>
      <div className="actions">
        <Button id={id} variant="go" reason={reason} onClick={add}>
          {adding ? "Choose a folder…" : "Add a folder"}
        </Button>
        <Button reason="Agent checks run the CLIs, and this build doesn't connect to the runtime.">Check agents</Button>
      </div>
    </section>
  );
}
