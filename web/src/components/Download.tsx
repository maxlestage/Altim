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
        <p className="muted">
          Utilisez Altim tout de suite dans votre navigateur, ou sur iPhone (iOS 17 et plus) via la bêta TestFlight.
        </p>
        <div className="download-actions">
          <a className="btn" href="/app">
            Ouvrir l'app web
          </a>
          <a className="btn btn-ghost" href="https://testflight.apple.com/" target="_blank" rel="noreferrer">
            Bêta iPhone (TestFlight)
          </a>
        </div>
      </div>
    </section>
  );
}
