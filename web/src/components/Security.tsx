import { Section } from "./Section";

const ITEMS = [
  ["Aucun accès à vos comptes", "Altim conseille uniquement : aucune clé, aucun identifiant de courtier, aucun ordre."],
  ["Vos avoirs restent chez vous", "Sur votre appareil (navigateur ou application, dans un stockage chiffré et exclu des sauvegardes), jamais sur un serveur."],
  ["Aucun traceur", "Pas de cookie, pas de mesure d'audience, pas de publicité."],
  ["Export à tout moment", "Vos avoirs s'exportent en un fichier, à réimporter sur un autre appareil."],
  ["Données vérifiées", "Chaque cours est recoupé sur plusieurs sources ; en cas de doute, aucun conseil."],
  ["Transparent", "Chaque conseil affiche ses raisons, les indicateurs et les sources utilisés."],
] as const;

export function Security() {
  return (
    <Section
      id="security"
      eyebrow="Confidentialité"
      title={
        <>
          Vos données, <span className="gradient">chez vous</span>
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
