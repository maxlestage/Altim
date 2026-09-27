import type { ReactNode } from "react";
import { useReveal } from "../hooks";

export function Section(props: { id: string; eyebrow: string; title: ReactNode; intro?: ReactNode; children: ReactNode }) {
  const ref = useReveal<HTMLElement>();
  return (
    <section className="section reveal" id={props.id} ref={ref}>
      <div className="section-head">
        <p className="eyebrow">{props.eyebrow}</p>
        <h2>{props.title}</h2>
        {props.intro && <p className="muted">{props.intro}</p>}
      </div>
      {props.children}
    </section>
  );
}
