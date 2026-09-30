import { Background } from "./components/Background";
import { Nav } from "./components/Nav";
import { Hero } from "./components/Hero";
import { Ticker } from "./components/Ticker";
import { LiveSignal } from "./components/LiveSignal";
import { Features } from "./components/Features";
import { Sources } from "./components/Sources";
import { HowItWorks } from "./components/HowItWorks";
import { Security } from "./components/Security";
import { Transparency } from "./components/Transparency";
import { BotSection } from "./components/BotSection";
import { FAQ } from "./components/FAQ";
import { Download } from "./components/Download";
import { Apps } from "./components/Apps";
import { Footer } from "./components/Footer";
import { MobileCTA } from "./components/MobileCTA";
import { LEGAL_PAGES, LegalPage } from "./components/Legal";
import { useTicks } from "./hooks";
import { WebApp } from "./webapp/WebApp";
import { setMoneyDisplay } from "./money";
import { startFx, useFx } from "./webapp/fx";
import { useAppState } from "./webapp/store";
import { useEffect } from "react";

export function App() {
  const path = window.location.pathname.replace(/\/$/, "") || "/";
  if (path === "/app" || path.startsWith("/app/")) return <WebApp />;
  if (LEGAL_PAGES[path]) {
    document.title = `${LEGAL_PAGES[path]!.title} — Altim`;
    return (
      <>
        <Background />
        <Nav />
        <LegalPage path={path} />
        <Footer />
      </>
    );
  }
  return <Home />;
}

function Home() {
  const ticks = useTicks();
  // The presentation site shows amounts in the app's display currency (euros by default), at the verified rate.
  const { currency } = useAppState();
  const { fx } = useFx();
  setMoneyDisplay(currency, fx);
  useEffect(() => startFx(), []);
  return (
    <>
      <Background />
      <Nav />
      <main>
        <Hero ticks={ticks} />
        <Ticker ticks={ticks} />
        <LiveSignal />
        <Features />
        <Apps />
        <Sources />
        <HowItWorks />
        <BotSection />
        <Transparency />
        <Security />
        <FAQ />
        <Download />
      </main>
      <Footer />
      <MobileCTA />
    </>
  );
}
