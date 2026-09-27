import { Section } from "./Section";

const STEPS = [
  {
    n: "01",
    title: "Analyse",
    text: "Toutes les 60 s, Altim récupère les bougies clôturées (Binance, Yahoo Finance), écarte les données invalides et calcule 7 indicateurs.",
  },
  {
    n: "02",
    title: "Décision",
    text: "Chaque indicateur vote de −1 à +1. Le score pondéré (−100 à +100) et l'accord entre indicateurs donnent l'action et la confiance.",
  },
  {
    n: "03",
    title: "Exécution",
    text: "Vous validez : taille calculée par le gestionnaire de risque, vérification des règles du marché, ordre test, Face ID, puis exécution avec protection.",
  },
];

export function HowItWorks() {
  return (
    <Section
      id="how"
      eyebrow="Le moteur"
      title={
        <>
          Transparent. <span className="gradient">Explicable.</span> Vérifié.
        </>
      }
      intro="Pas de boîte noire : chaque signal affiche la contribution de chaque indicateur."
    >
      <ol className="steps">
        {STEPS.map((s) => (
          <li key={s.n} className="card step">
            <span className="step-n">{s.n}</span>
            <h3>{s.title}</h3>
            <p>{s.text}</p>
          </li>
        ))}
      </ol>
    </Section>
  );
}
