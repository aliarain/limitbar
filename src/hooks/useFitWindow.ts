import { currentMonitor, getCurrentWindow, LogicalPosition, LogicalSize } from "@tauri-apps/api/window";
import { useLayoutEffect, useRef } from "react";

const MARGIN = 8;

/**
 * Keeps the OS window exactly the size of its content, so the chip is a chip.
 * When the content widens (detail panel opens) and there is no room to the
 * right, the window is shifted left so the chip stays on screen; the original
 * x is restored when it narrows again.
 */
export function useFitWindow(ref: React.RefObject<HTMLElement | null>, deps: unknown[]) {
  const anchorX = useRef<number | null>(null);
  const lastW = useRef(0);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    let cancelled = false;

    const apply = async () => {
      const rect = el.getBoundingClientRect();
      const w = Math.ceil(rect.width);
      const h = Math.ceil(rect.height);
      if (w === 0 || h === 0) return;
      const win = getCurrentWindow();
      try {
        const widening = w > lastW.current && lastW.current > 0;
        const narrowing = w < lastW.current;
        lastW.current = w;
        if (widening) {
          const scale = await win.scaleFactor();
          const pos = (await win.outerPosition()).toLogical(scale);
          const mon = await currentMonitor();
          if (mon) {
            const right = mon.position.toLogical(scale).x + mon.size.toLogical(scale).width - MARGIN;
            if (pos.x + w > right) {
              anchorX.current = pos.x;
              await win.setPosition(new LogicalPosition(Math.max(MARGIN, right - w), pos.y));
            }
          }
        }
        if (cancelled) return;
        await win.setSize(new LogicalSize(w, h));
        if (narrowing && anchorX.current !== null) {
          const scale = await win.scaleFactor();
          const pos = (await win.outerPosition()).toLogical(scale);
          await win.setPosition(new LogicalPosition(anchorX.current, pos.y));
          anchorX.current = null;
        }
      } catch {
        /* not running under Tauri (tests) */
      }
    };

    void apply();
    const ro = new ResizeObserver(() => void apply());
    ro.observe(el);
    return () => { cancelled = true; ro.disconnect(); };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
}
