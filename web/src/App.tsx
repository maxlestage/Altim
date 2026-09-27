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
import { FAQ } from "./components/FAQ";
import { Download } from "./components/Download";
import { Footer } from "./components/Footer";
import { MobileCTA } from "./components/MobileCTA";
import { LEGAL_PAGES, LegalPage } from "./components/Legal";
import { useTicks } from "./hooks";
import { WebApp } from "./webapp/WebApp";

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
  return (
    <>
      <Background />
      <Nav />
      <main>
        <Hero ticks={ticks} />
        <Ticker ticks={ticks} />
        <LiveSignal />
        <Features />
        <Sources />
        <HowItWorks />
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
