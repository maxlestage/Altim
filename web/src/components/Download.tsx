import { useReveal } from "../hooks";

export function Download() {
  const ref = useReveal<HTMLElement>();
  return (
    <section className="section download reveal" id="download" ref={ref}>
      <div className="card download-card">
        <img src="/logo.svg" alt="" width={96} height={96} />
        <h2>
          Prêt à voir le <span className="gradient">signal</span> ?
        </h2>
        <p className="muted">Altim arrive sur iPhone (iOS 17 et plus). Rejoignez la bêta TestFlight.</p>
        <a className="btn" href="https://testflight.apple.com/" target="_blank" rel="noreferrer">
          Rejoindre la bêta TestFlight
        </a>
      </div>
    </section>
  );
}
