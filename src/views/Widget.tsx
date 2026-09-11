import { useEffect, useRef, useState } from "react";
import { Chip } from "../components/Chip";
import { DetailPanel } from "../components/DetailPanel";
import { useFitWindow } from "../hooks/useFitWindow";
import { useNow } from "../hooks/useNow";
import { useUsage } from "../hooks/useUsage";

const COLLAPSE_DELAY_MS = 220;

export function Widget() {
  const { views, refresh, refreshing } = useUsage();
  const now = useNow(15_000);
  const [expanded, setExpanded] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const timer = useRef<number | null>(null);
  const enabled = views.filter((v) => v.enabled);
  const anyRefreshing = refreshing || views.some((v) => v.freshness === "REFRESHING");

  useFitWindow(root, [enabled, expanded, now]);

  const open = () => { if (timer.current) { window.clearTimeout(timer.current); timer.current = null; } setExpanded(true); };
  const close = () => { timer.current = window.setTimeout(() => setExpanded(false), COLLAPSE_DELAY_MS); };
  useEffect(() => () => { if (timer.current) window.clearTimeout(timer.current); }, []);

  return (
    <div ref={root} className={`widget${expanded ? " widget--expanded" : ""}`} onMouseEnter={open} onMouseLeave={close}>
      {expanded
        ? <DetailPanel views={enabled} now={now} refreshing={anyRefreshing} onRefresh={refresh} />
        : <Chip views={enabled} />}
    </div>
  );
}
