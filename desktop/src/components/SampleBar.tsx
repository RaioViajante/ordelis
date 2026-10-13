import { useSampleData } from "../app/SampleData";
import { Button } from "./ui";

export function SampleBar() {
  const { setOn } = useSampleData();
  return (
    <div className="sample-bar notice notice--quiet" role="note" aria-label="Sample data notice">
      <span className="mono ink2">◇</span>
      <p className="hint">
        <b className="font-medium text-ink-100">Sample data.</b> Invented for layout review. Nothing here was read from your
        machine, and nothing is running.
      </p>
      <span className="sp" />
      <Button small variant="ghost" onClick={() => setOn(false)}>
        Hide sample data
      </Button>
    </div>
  );
}
