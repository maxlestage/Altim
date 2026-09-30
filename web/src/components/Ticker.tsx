import { useEffect, useState } from "react";
import { formatPercent, type Tick } from "../market";
import { moneyPrice } from "../money";
import { api } from "../webapp/api";
import { ACTION_UI, type BotViews } from "../webapp/model-bot";

type View = BotViews["views"][number];
const BOT_EVERY = 5 * 60_000;

/** The bot's current view of each shown asset (/api/bot/views), refreshed every 5 minutes; empty when unavailable. */
function useBotViews(ticks: Tick[]): Record<string, View> {
  const [views, setViews] = useState<Record<string, View>>({});
  const key = ticks.map((t) => `${t.kind}:${t.symbol}`).join(",");
  useEffect(() => {
    if (!ticks.length) return;
    let alive = true;
    const load = () =>
      api
        .botViews(ticks.map((t) => ({ symbol: t.symbol, kind: t.kind })))
        .then((r) => alive && setViews(Object.fromEntries((r.views ?? []).map((v) => [`${v.kind}:${v.symbol}`, v]))))
        .catch(() => {});
    void load();
    const timer = setInterval(load, BOT_EVERY);
    return () => {
      alive = false;
      clearInterval(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
  return views;
}

/** Markets panel: prices validated by consensus, in the display currency, with the bot's live view (fixed grid, no horizontal scrolling). */
export function Ticker({ ticks }: { ticks: Tick[] }) {
  const bot = useBotViews(ticks);
  const anyBot = Object.values(bot).some((v) => v.available && v.action);
  return (
    <section className="markets" aria-labelledby="markets-title">
      <div className="markets-head">
        <h2 id="markets-title">Marchés en direct</h2>
        <small className="muted">Chaque cours = médiane des sources concordantes{anyBot ? " · avis du Bot Altim en direct" : ""}</small>
      </div>
      {ticks.length ? (
        <ul className="markets-grid">
          {ticks.map((t) => {
            const v = bot[`${t.kind}:${t.symbol}`];
            const ui = v?.available && v.action ? ACTION_UI[v.action] : null;
            return (
              <li key={t.symbol} className="market">
                <div className="market-top">
                  <b>{t.symbol}</b>
                  <span className={t.agreeing >= 2 ? "agree ok" : "agree weak"} title={`${t.agreeing} source(s) concordante(s) sur ${t.total}`}>
                    {t.agreeing >= 2 ? "✔" : "!"} {t.agreeing}/{t.total}
                  </span>
                </div>
                <span className="market-price">{moneyPrice(t.price, " ")}</span>
                {t.change !== null && <span className={t.change >= 0 ? "up" : "down"}>{formatPercent(t.change)}</span>}
                {ui && (
                  <a className="market-bot" href="/app/bot" title={v!.counts ? "Compte dans la décision" : "Pour information : ne compte pas dans la décision"}>
                    <span className={`bot-chip ${ui.tone}`}>Bot : {ui.label}</span>
                    {!v!.counts && <small className="muted">pour info</small>}
                  </a>
                )}
              </li>
            );
          })}
        </ul>
      ) : (
        <p className="muted markets-loading">Interrogation des sources…</p>
      )}
      {anyBot && (
        <small className="muted markets-note">
          Avis du bot = modèle appris, jugé hors échantillon ; tant qu'il n'a pas prouvé d'avantage, il ne pèse pas dans les décisions (« pour info »). <a href="/app/bot">Voir ses résultats</a>
        </small>
      )}
    </section>
  );
}
