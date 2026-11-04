// Title and description fields for New task and Edit (issue #96). Limits are Rust's, counted in UTF-8 bytes, and the
// text is never cut: a problem is explained and the user decides what to change.
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
}: {
  title: string;
  description: string;
  onTitle: (value: string) => void;
  onDescription: (value: string) => void;
  /** Shown once the user has tried to save. */
  problem: TextProblem | null;
  readOnly?: boolean;
}) {
  const id = useId();
  const titleBytes = bytes(title.trim());
  // Rust's rule is title + 2 + description <= 4096; the two separating newlines are not the user's text.
  const total = titleBytes + bytes(description);
  const room = MAX_TEXT_BYTES - 2;
  const titleProblem = problem && TITLE_PROBLEMS.includes(problem) ? problem : null;
  const textProblem = problem && !TITLE_PROBLEMS.includes(problem) ? problem : null;

  return (
    <>
      <div className="tk-field">
        <label className="lb" htmlFor={`${id}-title`}>
          Title
          <span className={`cap${titleBytes > MAX_TITLE_BYTES ? " cap--warn" : ""}`}>
            {fmt(titleBytes)} / {fmt(MAX_TITLE_BYTES)} bytes
          </span>
        </label>
        <input
          id={`${id}-title`}
          className="field tk-title"
          dir="auto"
          value={title}
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
      <div className="tk-field">
        <label className="lb" htmlFor={`${id}-description`}>
          Description
          <span className={`cap${total > room ? " cap--warn" : ""}`}>
            {fmt(total)} / {fmt(room)} bytes with the title
          </span>
        </label>
        <div className="card tk-card">
          <textarea
            id={`${id}-description`}
            className="tk-description"
            dir="auto"
            rows={6}
            value={description}
            readOnly={readOnly}
            onChange={(e) => onDescription(e.target.value)}
            aria-invalid={textProblem ? true : undefined}
            aria-describedby={textProblem ? `${id}-description-problem` : `${id}-description-help`}
            spellCheck
          />
        </div>
        {textProblem ? (
          <p id={`${id}-description-problem`} className="tk-problem" role="alert">
            {problemText(textProblem)}
          </p>
        ) : (
          <p id={`${id}-description-help`} className="cap">
            Describe the outcome and any constraints. It is saved as you wrote it, line breaks included.
          </p>
        )}
      </div>
    </>
  );
}
