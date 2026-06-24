import { useEffect, useRef } from "react";

export interface ResizerProps {
  direction: "horizontal" | "vertical";
  onResize: (delta: number) => void;
  onResizeEnd?: () => void;
}

/**
 * Thin draggable gutter. `direction='horizontal'` resizes a column
 * (drag X axis, cursor col-resize). `direction='vertical'` resizes a
 * row (drag Y axis, cursor row-resize). Reports per-pointermove
 * deltas; parent clamps + applies. Pure DOM, no library.
 */
export function Resizer({ direction, onResize, onResizeEnd }: ResizerProps) {
  const ref = useRef<HTMLDivElement>(null);
  const dragging = useRef(false);
  const last = useRef<{ x: number; y: number } | null>(null);

  useEffect(() => {
    const move = (e: PointerEvent) => {
      if (!dragging.current || !last.current) return;
      if (direction === "horizontal") {
        const dx = e.clientX - last.current.x;
        last.current = { x: e.clientX, y: e.clientY };
        if (dx !== 0) onResize(dx);
      } else {
        const dy = e.clientY - last.current.y;
        last.current = { x: e.clientX, y: e.clientY };
        if (dy !== 0) onResize(dy);
      }
    };
    const up = () => {
      if (!dragging.current) return;
      dragging.current = false;
      last.current = null;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      onResizeEnd?.();
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
    };
  }, [direction, onResize, onResizeEnd]);

  const onDown = (e: React.PointerEvent) => {
    e.preventDefault();
    dragging.current = true;
    last.current = { x: e.clientX, y: e.clientY };
    document.body.style.cursor =
      direction === "horizontal" ? "col-resize" : "row-resize";
    document.body.style.userSelect = "none";
  };

  return (
    <div
      ref={ref}
      className={`resizer resizer-${direction}`}
      onPointerDown={onDown}
      role="separator"
      aria-orientation={direction === "horizontal" ? "vertical" : "horizontal"}
    />
  );
}
