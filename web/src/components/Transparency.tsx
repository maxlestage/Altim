import { Section } from "./Section";

export function Transparency() {
  return (
    <Section
      id="transparency"
      eyebrow="Honnêteté"
      title={
        <>
          Ce qu'Altim <span className="gradient">ne vous promettra jamais</span>
        </>
      }
    >
      <div className="card transparency">
        <p>
          Aucun algorithme ne prédit le marché à coup sûr. Nos backtests le montrent : en marché fortement haussier,
          conserver simplement l'actif rapporte souvent plus que suivre des signaux. En revanche, en marché baissier, le
          moteur limite nettement les pertes — sur BTC en journalier, <b>−3,8 %</b> pour la stratégie contre{" "}
          <b>−29,6 %</b> pour l'achat-conservation sur la même période.
        </p>
        <p>
          C'est pourquoi l'app affiche, pour chaque actif, la performance de la stratégie <em>et</em> celle de
          l'achat-conservation, frais inclus. Vous décidez en connaissance de cause.
        </p>
      </div>
    </Section>
  );
}
