import { useEffect, useRef } from "react";
import type { Snapshot } from "../ipc/types";
import DocumentView from "../prompter/DocumentView";
import StartScreen from "../prompter/StartScreen";
import { publishIntent } from "../store/intents";
import { startPresentationStore } from "../store/presentation";

// Text only, always mirrored per settings. The operator window reports to the
// host; this one follows the same intents. On open it jumps to where the
// operator's read line is and resumes the current motion.
export default function TalentWindow({ snapshot }: { snapshot: Snapshot }) {
  const joined = useRef(false);

  useEffect(() => startPresentationStore(), []);

  useEffect(() => {
    if (joined.current) return;
    joined.current = true;
    if (snapshot.read_line_pos) publishIntent({ type: "snap", pos: snapshot.read_line_pos });
    const last = snapshot.last_intent;
    if (last && (last.type === "cruise" || last.type === "hold" || last.type === "pause")) publishIntent(last);
  }, [snapshot]);

  return (
    <div className="talent">
      {!snapshot.document && <StartScreen settings={snapshot.settings} />}
      <DocumentView document={snapshot.document} settings={snapshot.settings} mirror />
    </div>
  );
}
