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
          Utilisez Altim tout de suite dans votre navigateur, sur téléphone comme sur ordinateur. Sur iPhone : Partager → « Sur l'écran d'accueil » pour l'ouvrir comme une app.
        </p>
        <div className="download-actions">
          <a className="btn" href="/app">
            Ouvrir l'app web
          </a>
        </div>
      </div>
    </section>
  );
}
