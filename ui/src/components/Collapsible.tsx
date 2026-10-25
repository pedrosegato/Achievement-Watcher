import { Children, type CSSProperties, type ReactNode, useId } from "react";
import { Icon } from "./Icon";

interface CollapsibleProps {
  card?: boolean;
  className?: string;
  open: boolean;
  onToggle: () => void;
  header: ReactNode;
  count?: ReactNode;
  peek?: ReactNode;
  children: ReactNode;
}

export function Collapsible({
  card = false,
  className,
  open,
  onToggle,
  header,
  count,
  peek,
  children,
}: CollapsibleProps) {
  const bodyId = useId();
  const rows = Children.toArray(children);
  const peeks = Children.toArray(peek);
  const state = open ? "open" : "closed";

  return (
    <section
      className={className ? `collapse-section ${className}` : "collapse-section"}
      data-state={state}
    >
      <button
        type="button"
        className="collapse-head"
        aria-expanded={open}
        aria-controls={bodyId}
        onClick={onToggle}
      >
        <Icon name="caret-down" size={16} className="collapse-caret" />
        <span className="collapse-title">{header}</span>
        <span className="collapse-rule" />
        {peeks.length > 0 && (
          <span className="collapse-peek" aria-hidden={open}>
            {peeks.map((item, index) => (
              <span
                key={keyOf(item, index)}
                style={{ "--j": peeks.length - 1 - index } as CSSProperties}
              >
                {item}
              </span>
            ))}
          </span>
        )}
        {count !== undefined && <span className="collapse-count">{count}</span>}
      </button>
      <div id={bodyId} className="collapse-body" inert={!open}>
        <div className="collapse-inner">
          <div className={card ? "collapse-card" : undefined}>
            {rows.map((row, index) => (
              <div
                key={keyOf(row, index)}
                className="collapse-row"
                style={{ "--i": index, "--n": rows.length } as CSSProperties}
              >
                {row}
              </div>
            ))}
          </div>
        </div>
      </div>
    </section>
  );
}

function keyOf(node: ReactNode, index: number): string {
  if (node && typeof node === "object" && "key" in node && node.key !== null)
    return String(node.key);
  return `item-${index}`;
}
