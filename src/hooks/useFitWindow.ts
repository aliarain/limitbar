import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { useLayoutEffect } from "react";

const WIDTH = 300;
const MIN_HEIGHT = 96;
const MAX_HEIGHT = 560;

/** Resizes the popup to its content so it reads as a chip, not a window. */
export function useFitWindow(ref: React.RefObject<HTMLElement | null>, deps: unknown[]) {
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    let last = -1;
    const apply = () => {
      const h = Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, Math.ceil(el.getBoundingClientRect().height)));
      if (h === last) return;
      last = h;
      getCurrentWindow().setSize(new LogicalSize(WIDTH, h)).catch(() => {/* not running under Tauri (tests) */});
    };
    apply();
    const ro = new ResizeObserver(apply);
    ro.observe(el);
    return () => ro.disconnect();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}
