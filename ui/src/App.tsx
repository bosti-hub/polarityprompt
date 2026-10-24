import { useEffect } from "react";
import { windowRole } from "./ipc";
import { startSnapshotStore, useSnapshot } from "./store/snapshot";
import OperatorWindow from "./windows/OperatorWindow";
import TalentWindow from "./windows/TalentWindow";

export default function App() {
  const snapshot = useSnapshot();
  useEffect(() => startSnapshotStore(), []);

  if (!snapshot) return <div className="loading">Connecting…</div>;
  return windowRole() === "talent" ? <TalentWindow snapshot={snapshot} /> : <OperatorWindow snapshot={snapshot} />;
}
