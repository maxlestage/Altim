import type { Tick } from "../market";
import { PhoneMockup } from "./PhoneMockup";

export function Hero({ ticks }: { ticks: Tick[] }) {
  return (
    <section className="hero">
      <div className="hero-text">
        <p className="chip">
          <span className="dot" /> iOS · Crypto & Actions · Swift natif
        </p>
        <h1>
          Le marché,
          <br />
          <span className="gradient glitch" data-text="décodé.">
            décodé.
          </span>
        </h1>
        <p className="lead">
          Altim scanne en continu vos cryptos et vos actions, croise <strong>7 familles d'indicateurs</strong> sur
          plusieurs unités de temps et vous dit clairement quand <em className="buy">acheter</em>, quand{" "}
          <em className="sell">vendre</em> — et surtout quand <em className="hold">attendre</em>.
        </p>
        <div className="cta">
          <a className="btn" href="#live">
            Voir le signal en direct
          </a>
          <a className="btn btn-ghost" href="#features">
            Découvrir l'app
          </a>
        </div>
        <dl className="stats">
          <div>
            <dt>7</dt>
            <dd>indicateurs croisés</dd>
          </div>
          <div>
            <dt>4</dt>
            <dd>unités de temps</dd>
          </div>
          <div>
            <dt>0</dt>
            <dd>ordre sans Face ID</dd>
          </div>
        </dl>
      </div>
      <PhoneMockup ticks={ticks} />
    </section>
  );
}
