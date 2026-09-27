import { Section } from "./Section";

function Icon({ d }: { d: string }) {
  return (
    <svg viewBox="0 0 24 24" width="28" height="28" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d={d} />
    </svg>
  );
}

const FEATURES = [
  {
    icon: "M3 12h4l3-8 4 16 3-8h4",
    title: "Signaux par confluence",
    text: "Tendance (EMA 50/200 + ADX), MACD, RSI, Stochastique, Bollinger et volume (OBV) votent ensemble. Un achat n'est proposé que s'ils s'accordent.",
  },
  {
    icon: "M4 20V10M10 20V4M16 20v-7M22 20H2",
    title: "Multi-unités de temps",
    text: "Un signal 1 h est confirmé par la tendance 4 h. Acheter contre la tendance de fond ? Altim rétrograde automatiquement le signal.",
  },
  {
    icon: "M12 3v18M5 8l7-5 7 5M5 16l7 5 7-5",
    title: "Des conseils concrets",
    text: "Chaque conseil d'achat arrive avec un plan : zone d'entrée, stop à 2 ATR, objectif à 2× le risque, et le montant prudent à y consacrer selon votre patrimoine.",
  },
  {
    icon: "M12 2a10 10 0 1 0 10 10M12 6v6l4 2",
    title: "Backtest intégré",
    text: "Voyez comment les signaux se sont comportés sur chaque actif : rendement, taux de réussite, pire baisse — comparés à l'achat-conservation.",
  },
  {
    icon: "M12 2 3 7l9 5 9-5-9-5zM3 12l9 5 9-5M3 17l9 5 9-5",
    title: "Vos avoirs analysés",
    text: "Renseignez ce que vous possédez déjà : patrimoine, plus-values, répartition, risque d'une mauvaise journée et, ligne par ligne, quoi faire (conserver, alléger, protéger, renforcer).",
  },
  {
    icon: "M12 2 3 6v6c0 5 4 9 9 10 5-1 9-5 9-10V6z",
    title: "Prudence intégrée",
    text: "Montants conseillés pour ne risquer que 1 % de votre patrimoine par idée, plafond par ligne, et aucun conseil quand les données ne sont pas fiables.",
  },
  {
    icon: "M7 11V7a5 5 0 0 1 10 0v4M5 11h14v10H5z",
    title: "Conseil, jamais d'ordre",
    text: "Altim ne passe aucun ordre et n'accède à aucun compte. Vos avoirs restent sur votre appareil, dans votre navigateur.",
  },
];

export function Features() {
  return (
    <Section
      id="features"
      eyebrow="Fonctionnalités"
      title={
        <>
          Tout ce qu'il faut pour <span className="gradient">décider vite et bien</span>
        </>
      }
    >
      <div className="features">
        {FEATURES.map((f) => (
          <article key={f.title} className="card feature">
            <div className="feature-icon">
              <Icon d={f.icon} />
            </div>
            <h3>{f.title}</h3>
            <p>{f.text}</p>
          </article>
        ))}
      </div>
    </Section>
  );
}
