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
          Sur iPhone et Apple Watch, sur Android, ou tout de suite dans votre navigateur : les mêmes conseils partout, et une notification quand vous pouvez acheter.
        </p>
        <div className="download-actions">
          <a className="btn" href="/#apps">
            Voir les applications
          </a>
          <a className="btn btn-ghost" href="/app">
            Ouvrir l'app web
          </a>
        </div>
      </div>
    </section>
  );
}
