import { Section } from "./Section";

const ITEMS = [
  ["Clés chiffrées", "Vos clés API restent dans le trousseau iOS de votre iPhone, jamais sur un serveur Altim."],
  ["Sans retrait", "Altim n'a besoin que de la permission de trading. Désactivez toujours le retrait."],
  ["Face ID obligatoire", "Aucun ordre ne part sans votre authentification biométrique."],
  ["Test d'abord", "Mode démo au démarrage, testnet Binance et paper trading Alpaca disponibles."],
  ["Ordre pré-validé", "Règles de lot, de prix et de montant minimum vérifiées avant envoi, puis ordre test chez le courtier."],
  ["Coupe-circuit", "Au-delà de la perte journalière choisie, les nouveaux achats sont bloqués."],
] as const;

export function Security() {
  return (
    <Section
      id="security"
      eyebrow="Sécurité"
      title={
        <>
          Votre argent, <span className="gradient">vos règles</span>
        </>
      }
    >
      <div className="security">
        {ITEMS.map(([title, text]) => (
          <div key={title} className="sec-item">
            <span className="check" aria-hidden>
              ◆
            </span>
            <div>
              <h3>{title}</h3>
              <p>{text}</p>
            </div>
          </div>
        ))}
      </div>
    </Section>
  );
}
