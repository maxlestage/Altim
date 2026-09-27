import { Background } from "./components/Background";
import { Nav } from "./components/Nav";
import { Hero } from "./components/Hero";
import { Ticker } from "./components/Ticker";
import { LiveSignal } from "./components/LiveSignal";
import { Features } from "./components/Features";
import { HowItWorks } from "./components/HowItWorks";
import { Security } from "./components/Security";
import { Transparency } from "./components/Transparency";
import { FAQ } from "./components/FAQ";
import { Download } from "./components/Download";
import { Footer } from "./components/Footer";
import { useTicks } from "./hooks";

export function App() {
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
        <HowItWorks />
        <Transparency />
        <Security />
        <FAQ />
        <Download />
      </main>
      <Footer />
    </>
  );
}
