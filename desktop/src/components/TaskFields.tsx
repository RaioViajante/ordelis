// Title and description fields for Home's composer, New task and Edit (issues #96 and #98). Limits are Rust's, counted
// in UTF-8 bytes, and the text is never cut: a problem is explained and the user decides what to change. The composer
// variant sits inside Home's composer card, so the description has no card of its own.
import { useId } from "react";
import { problemText } from "../app/TaskState";
import { bytes, MAX_TEXT_BYTES, MAX_TITLE_BYTES, type TextProblem } from "../app/tasks";

const fmt = (n: number) => n.toLocaleString("en-US");
const TITLE_PROBLEMS: TextProblem[] = ["title_empty", "title_too_long", "title_spaces", "title_control"];

export function TaskFields({
  title,
  description,
  onTitle,
  onDescription,
  problem,
  readOnly,
  variant = "page",
}: {
  title: string;
  description: string;
  onTitle: (value: string) => void;
  onDescription: (value: string) => void;
  /** Shown once the user has tried to save. */
  problem: TextProblem | null;
  readOnly?: boolean;
  variant?: "page" | "composer";
}) {
  const id = useId();
  const titleBytes = bytes(title.trim());
  // Rust's rule is title + 2 + description <= 4096; the two separating newlines are not the user's text.
  const total = titleBytes + bytes(description);
  const room = MAX_TEXT_BYTES - 2;
  const titleProblem = problem && TITLE_PROBLEMS.includes(problem) ? problem : null;
  const textProblem = problem && !TITLE_PROBLEMS.includes(problem) ? problem : null;
  const composer = variant === "composer";
  const textarea = (
    <textarea
      id={`${id}-description`}
      className={composer ? "ask__description" : "tk-description"}
      dir="auto"
      rows={composer ? 3 : 6}
      value={description}
      readOnly={readOnly}
      onChange={(e) => onDescription(e.target.value)}
      placeholder={composer ? "Describe the outcome and any constraints…" : undefined}
      aria-invalid={textProblem ? true : undefined}
      aria-describedby={textProblem ? `${id}-description-problem` : composer ? undefined : `${id}-description-help`}
      spellCheck
    />
  );

  return (
    <>
      <div className={composer ? "tk-field ask__field" : "tk-field"}>
        <label className="lb" htmlFor={`${id}-title`}>
          Title
          <span className={`cap${titleBytes > MAX_TITLE_BYTES ? " cap--warn" : ""}`}>
            {fmt(titleBytes)} / {fmt(MAX_TITLE_BYTES)} bytes
          </span>
        </label>
        <input
          id={`${id}-title`}
          className={composer ? "ask__title" : "field tk-title"}
          dir="auto"
          value={title}
          placeholder={composer ? "What should be done?" : undefined}
          readOnly={readOnly}
          onChange={(e) => onTitle(e.target.value)}
          aria-invalid={titleProblem ? true : undefined}
          aria-describedby={titleProblem ? `${id}-title-problem` : undefined}
          autoComplete="off"
          spellCheck
        />
        {titleProblem && (
          <p id={`${id}-title-problem`} className="tk-problem" role="alert">
            {problemText(titleProblem)}
          </p>
        )}
      </div>
      <div className={composer ? "tk-field ask__field" : "tk-field"}>
        <label className="lb" htmlFor={`${id}-description`}>
          Description
          <span className={`cap${total > room ? " cap--warn" : ""}`}>
            {fmt(total)} / {fmt(room)} bytes with the title
          </span>
        </label>
        {composer ? textarea : <div className="card tk-card">{textarea}</div>}
        {textProblem ? (
          <p id={`${id}-description-problem`} className="tk-problem" role="alert">
            {problemText(textProblem)}
          </p>
        ) : composer ? null : (
          <p id={`${id}-description-help`} className="cap">
            Describe the outcome and any constraints. It is saved as you wrote it, line breaks included.
          </p>
        )}
      </div>
    </>
  );
}
