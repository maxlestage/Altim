import { Section } from "./Section";

type App = { id: string; name: string; tag: string; points: string[]; icon: string };

const APPS: App[] = [
  {
    id: "iphone",
    name: "iPhone",
    tag: "App native SwiftUI · iOS 17+",
    icon: "M8 2h8a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2zM11 18h2",
    points: [
      "Notification « achat possible » dès qu'un actif de votre radar ou de vos avoirs devient achetable.",
      "Suivi en direct sur l'écran verrouillé et dans la Dynamic Island : prix et verdict d'achat.",
      "Face ID à l'ouverture, mot de passe chiffré dans le trousseau de l'iPhone.",
    ],
  },
  {
    id: "watch",
    name: "Apple Watch",
    tag: "App watchOS",
    icon: "M9 2h6l1 4H8zM9 22h6l1-4H8zM7 6h10a1 1 0 0 1 1 1v10a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1zM12 9v3l2 1",
    points: [
      "Les actifs achetables d'un coup d'œil, avec leurs raisons.",
      "Les notifications d'achat de l'iPhone arrivent au poignet.",
      "Ni mot de passe ni session sur la montre : elle affiche ce que l'iPhone a calculé.",
    ],
  },
  {
    id: "android",
    name: "Android",
    tag: "App native Kotlin · Android 11+",
    icon: "M7 9h10v8a1 1 0 0 1-1 1H8a1 1 0 0 1-1-1zM7 9a5 5 0 0 1 10 0M9 5 8 3M15 5l1-2M10 7h.01M14 7h.01",
    points: [
      "Notification « achat possible » toutes les 15 minutes, sans répétition.",
      "Empreinte, visage ou code de l'écran ; mot de passe chiffré par le Keystore Android.",
      "Mêmes écrans que sur iPhone : radar en direct, zones, sélection, avoirs.",
    ],
  },
  {
    id: "web",
    name: "Navigateur",
    tag: "Application web",
    icon: "M3 5h18v14H3zM3 9h18M7 7h.01M10 7h.01",
    points: [
      "Sur ordinateur comme sur téléphone, sans installation.",
      "Les mêmes chiffres partout : tout est calculé sur votre serveur Altim privé.",
    ],
  },
];

/** The native apps (iPhone, Apple Watch, Android) and the web app, all clients of the same private server. */
export function Apps() {
  return (
    <Section
      id="apps"
      eyebrow="Applications"
      title={
        <>
          Sur <span className="gradient">iPhone, Apple Watch et Android</span>
        </>
      }
    >
      <p className="muted apps-intro">
        Des applications natives, pas un site déguisé. Elles se connectent toutes à votre serveur Altim privé : 40 sources de prix, mêmes signaux, mêmes zones
        d'achat, mêmes chiffres qu'ici. Altim ne passe jamais d'ordre.
      </p>
      <div className="apps">
        {APPS.map((a) => (
          <article key={a.id} className="card feature app-card" id={`app-${a.id}`}>
            <div className="feature-icon">
              <svg viewBox="0 0 24 24" width="28" height="28" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                <path d={a.icon} />
              </svg>
            </div>
            <h3>{a.name}</h3>
            <p className="app-tag">{a.tag}</p>
            <ul>
              {a.points.map((p) => (
                <li key={p}>{p}</li>
              ))}
            </ul>
          </article>
        ))}
      </div>
      <p className="muted small apps-note">
        Accès privé : les apps iPhone et Apple Watch s'installent par TestFlight, l'app Android par un APK signé (sans Google Play). La marche à suivre est dans le
        guide de déploiement.
      </p>
    </Section>
  );
}
