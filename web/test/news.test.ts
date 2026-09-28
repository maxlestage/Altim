import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { aggregate, decodeText, mentions, newsDigest, parseFeed, PER_CATEGORY, safeLink, sameStory, topStories, type RawNews } from "../src/engine/news";

// Real feeds captured in September 2026 (6 items each), see test/news-samples.
const sample = (name: string) => readFileSync(join(import.meta.dir, "news-samples", `${name}.xml`), "utf8");

describe("lecture des flux RSS réels", () => {
  test("chaque flux donne des titres, liens, dates et sources propres", () => {
    for (const [name, source] of [["bfm", "BFM Économie"], ["cointelegraph", "Cointelegraph"], ["google-fr", "Google Actualités"], ["yahoo-aapl", "Yahoo Finance"], ["decrypt", "Decrypt"]] as const) {
      const items = parseFeed(sample(name), source);
      expect(items.length).toBe(6);
      for (const it of items) {
        expect(it.title.length).toBeGreaterThan(5);
        expect(it.title).not.toMatch(/<|&amp;|&#\d+;|CDATA/);
        expect(it.link).toMatch(/^https?:\/\//);
        expect(Number.isFinite(it.time)).toBe(true);
        expect(it.source.length).toBeGreaterThan(1);
        if (it.summary) expect(it.summary).not.toMatch(/<[a-z/]/i);
      }
    }
  });

  test("Google Actualités : la source est retirée du titre et le résumé (qui répète le titre) supprimé", () => {
    const items = parseFeed(sample("google-fr"), "Google Actualités");
    for (const it of items) {
      expect(it.title.endsWith(` - ${it.source}`)).toBe(false);
      expect(it.summary).toBeUndefined();
      expect(it.source).not.toBe("Google Actualités");
    }
  });

  test("entités, balises encodées, liens dangereux", () => {
    expect(decodeText("<![CDATA[L&#8217;or &amp; le <b>dollar</b>]]>")).toBe("L’or & le dollar");
    expect(decodeText("&lt;a href=&quot;x&quot;&gt;Titre&lt;/a&gt;&nbsp;Source")).toBe("Titre Source");
    expect(safeLink("javascript:alert(1)")).toBeNull();
    expect(safeLink("data:text/html,x")).toBeNull();
    expect(safeLink("https://example.com/a?b=1")).toBe("https://example.com/a?b=1");
    const feed = `<rss><channel><item><title>Titre piégé</title><link>javascript:alert(1)</link><pubDate>Mon, 28 Sep 2026 06:00:00 GMT</pubDate></item></channel></rss>`;
    expect(parseFeed(feed, "X")).toEqual([]);
  });

  test("Atom", () => {
    const atom = `<feed><entry><title>Fed holds rates</title><link href="https://ex.com/1"/><updated>2026-09-28T06:00:00Z</updated></entry></feed>`;
    expect(parseFeed(atom, "Atom")).toEqual([{ title: "Fed holds rates", link: "https://ex.com/1", time: Date.parse("2026-09-28T06:00:00Z"), source: "Atom", summary: undefined }]);
  });
});

const now = Date.parse("2026-09-28T08:00:00Z");
const item = (title: string, source: string, hoursAgo = 1, link = `https://ex.com/${encodeURIComponent(title)}/${source}`): RawNews => ({ title, link, source, time: now - hoursAgo * 3_600_000 });

describe("regroupement et classement", () => {
  test("la même histoire racontée par trois sources n'apparaît qu'une fois, avec les autres sources", () => {
    const a = item("Fed holds interest rates steady as inflation cools", "Reuters", 3);
    const b = item("Fed holds interest rates steady while inflation cools", "CNBC", 2);
    const c = item("Fed holds interest rates steady, inflation cools", "MarketWatch", 1);
    const d = item("Bitcoin ETF inflows reach record high", "CoinDesk", 1);
    expect(sameStory(a, b)).toBe(true);
    expect(sameStory(a, d)).toBe(false);
    const out = aggregate([{ category: "monde", items: [a, b, c] }, { category: "crypto", items: [d] }], [], now);
    expect(out.length).toBe(2);
    const fed = out.find((i) => i.title.startsWith("Fed"))!;
    expect(fed.source).toBe("Reuters"); // the first to tell it
    expect(fed.alsoIn.sort()).toEqual(["CNBC", "MarketWatch"]);
    expect(fed.themes).toContain("monetary");
    expect(topStories(out)[0]!.id).toBe(fed.id);
  });

  test("actifs de l'utilisateur : nom ou symbole, sans faux positifs", () => {
    const btc = { id: "crypto:BTC", symbol: "BTC", name: "Bitcoin" };
    const apple = { id: "stock:AAPL", symbol: "AAPL", name: "Apple Inc." };
    const sol = { id: "crypto:SOL", symbol: "SOL", name: "Solana" };
    expect(mentions("Bitcoin tops $90,000", btc)).toBe(true);
    expect(mentions("BTC dominance rises", btc)).toBe(true);
    expect(mentions("Apple unveils new iPhone", apple)).toBe(true);
    expect(mentions("Pineapple prices rise", apple)).toBe(false);
    expect(mentions("Le sol est gelé", sol)).toBe(false);
    expect(mentions("$SOL rallies", sol)).toBe(true);
  });

  test("une recherche pour un actif qui ramène un article sans rapport le range dans son marché", () => {
    const out = aggregate(
      [{ category: "actifs", fallback: "crypto", items: [item("Bitcoin miners expand in Texas", "Google News"), item("Crypto market wobbles on macro fears", "Google News")] }],
      [{ id: "crypto:BTC", symbol: "BTC", name: "Bitcoin" }],
      now,
    );
    expect(out.find((i) => i.title.startsWith("Bitcoin"))).toMatchObject({ category: "actifs", assets: ["crypto:BTC"] });
    expect(out.find((i) => i.title.startsWith("Crypto"))).toMatchObject({ category: "crypto", assets: [] });
  });

  test("ton, langue, escalade, fraîcheur et plafond par catégorie", () => {
    const out = aggregate(
      [
        {
          category: "monde",
          items: [
            item("Stocks plunge as bank collapse sparks contagion fears", "A"),
            item("La Bourse de Paris grimpe après la décision de la BCE", "B"),
            item("Russia declares war on neighbour, markets slide", "C"),
            item("Old story from last week", "D", 24 * 7),
          ],
        },
      ],
      [],
      now,
    );
    expect(out.length).toBe(3);
    const crash = out.find((i) => i.source === "A")!;
    expect(crash.tone).toBe("negative");
    expect(crash.themes).toContain("stress");
    expect(crash.alert).toBe(true); // bank collapse
    const paris = out.find((i) => i.source === "B")!;
    expect(paris).toMatchObject({ lang: "fr", tone: "positive" });
    expect(paris.themes).toContain("monetary");
    expect(out.find((i) => i.source === "C")!.alert).toBe(true);
    // An escalation told by one source only stays off the front page; hedged titles are not escalations.
    expect(topStories(out).length).toBe(0);
    const hedged = aggregate([{ category: "monde", items: [item("Les agents IA pourraient déclencher un bank run, selon cet économiste", "X"), item("AI agents could trigger a bank run", "Y")] }], [], now);
    expect(hedged.every((i) => !i.alert)).toBe(true);
    const war = aggregate([{ category: "monde", items: [item("Russia declares war on neighbour, markets slide", "A"), item("Russia declares war on its neighbour as markets slide", "B"), item("Fed holds interest rates steady as inflation cools", "C"), item("Fed holds interest rates steady while inflation cools", "D")] }], [], now);
    expect(topStories(war)[0]!.alert).toBe(true);
    const w = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot", "golf", "hotel", "india", "juliet"];
    const distinct = (i: number) => `${w[i % 10]} ${w[Math.floor(i / 10)]}token ${i}x${w[(i * 3) % 10]}chain news`;
    const many = aggregate([{ category: "crypto", items: Array.from({ length: 100 }, (_, i) => item(distinct(i), `S${i}`, i / 10)) }], [], now);
    expect(many.length).toBe(PER_CATEGORY.crypto);
    expect(newsDigest(out, now).tone.negative).toBe(2);
  });
});
