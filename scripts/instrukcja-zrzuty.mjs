// Screenshots for the user guide (`instrukcja/index.html`): the website demo with made-up data,
// opened in Chrome by Playwright, with numbered markers drawn over the parts the guide talks about.
// Run after interface changes: `npm run instrukcja` (needs Google Chrome; another browser:
// CHROME_PATH=...). Writes `instrukcja/zrzuty/*.png`.

import { mkdirSync } from "node:fs";
import { createServer } from "vite";
import { chromium } from "playwright-core";

const OUT = "instrukcja/zrzuty";
const CHROME = process.env.CHROME_PATH ?? "C:/Program Files/Google/Chrome/Application/chrome.exe";
const PHONE_AGENT =
  "Mozilla/5.0 (Linux; Android 15; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Mobile Safari/537.36";
const only = process.argv[2];

/** Numbered markers over the given parts: a dashed frame and a number in a black circle. */
async function mark(page, items) {
  const boxes = [];
  for (const [locator, n] of items) {
    const box = await locator.first().boundingBox();
    if (!box) throw new Error(`marker ${n}: the part is not on screen`);
    boxes.push({ ...box, n });
  }
  await page.evaluate((list) => {
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    for (const b of list) {
      const frame = document.createElement("div");
      Object.assign(frame.style, {
        position: "fixed",
        left: `${b.x - 3}px`,
        top: `${b.y - 3}px`,
        width: `${b.width + 6}px`,
        height: `${b.height + 6}px`,
        border: "2px dashed #c0392b",
        borderRadius: "4px",
        zIndex: 2147483646,
        pointerEvents: "none",
      });
      const badge = document.createElement("div");
      badge.textContent = String(b.n);
      Object.assign(badge.style, {
        position: "fixed",
        left: `${Math.min(Math.max(b.x - 14, 2), vw - 30)}px`,
        top: `${Math.min(Math.max(b.y - 14, 2), vh - 30)}px`,
        width: "28px",
        height: "28px",
        display: "grid",
        placeItems: "center",
        borderRadius: "50%",
        background: "#c0392b",
        color: "#fff",
        border: "2px solid #fff",
        boxShadow: "0 1px 4px rgb(0 0 0 / 0.5)",
        font: "700 15px/1 Arial, sans-serif",
        zIndex: 2147483647,
        pointerEvents: "none",
      });
      // An open window is in the top layer - the markers go inside it, else they would be under it.
      const windows = document.querySelectorAll("dialog[open]");
      (windows[windows.length - 1] ?? document.body).append(frame, badge);
    }
  }, boxes);
}

const wait = (page, ms = 350) => page.waitForTimeout(ms);
const tab = (page, name) => page.locator(".topbar .tab", { hasText: name });
const dialog = (page) => page.locator("dialog.modal[open]");
const settings = async (page, section) => {
  await page.getByRole("button", { name: "Ustawienia", exact: true }).click();
  await wait(page);
  await dialog(page).getByRole("button", { name: section, exact: true }).click();
  await wait(page);
};
const card = (page) => page.locator("aside.pass");

/** Each screenshot: on a fresh demo (the guide in bubbles closed unless `tour`). */
const SHOTS = [
  {
    name: "pc-ekran-startowy",
    run: async (p) =>
      mark(p, [
        [p.locator(".topbar .tabs"), 1],
        [p.getByRole("button", { name: "Pobierz oferty" }), 2],
        [p.getByRole("button", { name: "Dodaj ofertę" }), 3],
        [p.getByRole("button", { name: "Ustawienia", exact: true }), 4],
        [p.locator(".list-filters"), 5],
        [p.getByRole("button", { name: "Eksportuj tę listę" }), 6],
        [p.locator("table.board thead"), 7],
        [card(p).locator(".pass-stub"), 8],
      ]),
  },
  { name: "pc-przewodnik", tour: true, run: async () => {} },
  {
    name: "pc-dodaj-oferte",
    run: async (p) => {
      await p.getByRole("button", { name: "Dodaj ofertę" }).click();
      await wait(p);
      const d = dialog(p);
      await mark(p, [
        [d.getByRole("textbox", { name: "Link do oferty" }).first(), 1],
        [d.getByRole("button", { name: "Pobierz treść ze strony" }), 2],
        [d.getByRole("textbox", { name: "Albo wklej treść oferty (skopiowaną ze strony)" }), 3],
        [d.getByRole("button", { name: "Uzupełnij pola (AI)" }), 4],
        [d.getByRole("textbox", { name: "Stanowisko (wymagane)" }), 5],
        [d.getByRole("textbox", { name: "Firma", exact: true }), 6],
        [d.getByRole("button", { name: "Zapisz ofertę" }), 7],
      ]);
    },
  },
  {
    name: "pc-karta",
    run: async (p) => {
      const c = card(p);
      await mark(p, [
        [c.locator(".stamp, .pass-score").first(), 1],
        [c.getByRole("button", { name: "Sprawdź duplikaty" }), 2],
        [c.getByRole("region", { name: "Ocena AI" }), 3],
        [c.getByRole("group", { name: "Zainteresowanie" }), 4],
        [c.getByRole("button", { name: "Dodaj do ulubionych" }), 5],
        [c.getByRole("button", { name: "Wygasła" }), 6],
        [c.getByRole("button", { name: "Otwórz ofertę" }), 7],
      ]);
    },
  },
  {
    name: "pc-karta-etap",
    run: async (p) => {
      const c = card(p);
      await c.getByRole("combobox", { name: "Etap rekrutacji" }).scrollIntoViewIfNeeded();
      await c.getByRole("combobox", { name: "Etap rekrutacji" }).evaluate((el) => el.scrollIntoView({ block: "start" }));
      await wait(p);
      await mark(p, [
        [c.getByRole("combobox", { name: "Etap rekrutacji" }), 1],
        [c.getByRole("checkbox", { name: "Własna oferta (zakładka „Własne”)" }), 2],
        [c.getByRole("button", { name: "Dodaj termin" }), 3],
        [c.getByRole("textbox", { name: /^Notatki/ }), 4],
        [c.getByRole("heading", { name: "Filtr" }), 5],
      ]);
    },
  },
  {
    name: "pc-termin",
    run: async (p) => {
      const c = card(p);
      await c.getByRole("button", { name: "Dodaj termin" }).click();
      await wait(p);
      await c.getByRole("combobox", { name: "Rodzaj" }).evaluate((el) => el.scrollIntoView({ block: "center" }));
      await wait(p);
      await mark(p, [
        [c.getByRole("combobox", { name: "Rodzaj" }), 1],
        [c.getByRole("textbox", { name: "Kiedy" }), 2],
        [c.getByRole("spinbutton", { name: "Ile minut" }), 3],
        [c.getByRole("button", { name: "Zapisz", exact: true }), 4],
      ]);
    },
  },
  {
    name: "pc-ogloszenia",
    run: async (p) => {
      await p.getByRole("button", { name: "+2 kolejne" }).click();
      await wait(p);
      await mark(p, [
        [p.getByRole("button", { name: "+2 kolejne" }), 1],
        [p.getByText(/kolejne ogłoszenie: /).first(), 2],
        [p.getByText(/wysłano przez:/).first(), 3],
      ]);
    },
  },
  {
    name: "pc-zebrane",
    run: async (p) => {
      await tab(p, "Zebrane").click();
      await wait(p, 600);
      const c = card(p);
      await mark(p, [
        [p.locator(".collected-switch"), 1],
        [p.locator("table.board tbody tr").first().getByText(/^AI: /), 2],
        [c.locator(".stamp, .pass-score").first(), 3],
        [c.locator(".pass-dock").getByRole("button", { name: "Do własnych" }), 4],
      ]);
    },
  },
  {
    name: "pc-kosz",
    run: async (p) => {
      await tab(p, "Kosz").click();
      await wait(p, 600);
      const c = card(p);
      await mark(p, [
        [p.locator(".collected-switch"), 1],
        [p.locator(".archive-search input"), 2],
        [p.getByRole("button", { name: "Opróżnij kosz" }), 3],
        [c.getByRole("button", { name: "Przywróć" }), 4],
        [c.getByRole("button", { name: "Usuń na zawsze" }), 5],
      ]);
    },
  },
  {
    name: "pc-profil",
    run: async (p) => {
      await settings(p, "Profil");
      const d = dialog(p);
      await mark(p, [
        [d.getByRole("textbox", { name: "Czego szukam (wymagane)" }), 1],
        [d.getByRole("button", { name: "Zaproponuj filtry (AI)" }), 2],
        [d.getByRole("textbox", { name: /^Doświadczenie zawodowe/ }), 3],
        [d.getByRole("group", { name: "Jak użyć: Gdzie i jak" }), 4],
      ]);
    },
  },
  {
    name: "pc-profil-filtry",
    run: async (p) => {
      await settings(p, "Profil");
      const d = dialog(p);
      await d.getByRole("heading", { name: /Oczekiwania finansowe/ }).evaluate((el) => el.scrollIntoView({ block: "start" }));
      await wait(p);
      await mark(p, [
        [d.getByRole("group", { name: "Oczekiwania finansowe", exact: true }), 1],
        [d.getByRole("textbox", { name: "Słowa kluczowe / technologie" }), 2],
        [d.getByRole("textbox", { name: "Słowa wykluczające" }), 3],
        [d.getByRole("button", { name: "Zapisz profil" }), 4],
      ]);
    },
  },
  {
    name: "pc-kilka-profili",
    run: async (p) => {
      await settings(p, "Wygląd");
      await dialog(p).getByRole("checkbox", { name: "Kilka profili" }).check();
      await dialog(p).getByRole("button", { name: "Zamknij" }).click();
      await wait(p, 500);
      await mark(p, [
        [p.locator(".profile-tabs, [aria-label='Profile']").first(), 1],
      ]);
    },
  },
  {
    name: "pc-zrodla",
    run: async (p) => {
      await settings(p, "Źródła ofert");
      const d = dialog(p);
      await mark(p, [
        [d.getByRole("checkbox", { name: /^Zbieraj oferty automatycznie/ }), 1],
        [d.getByRole("checkbox", { name: /^Pobieraj nowe oferty automatycznie/ }), 2],
        [d.getByRole("checkbox", { name: "Pobieraj też przy zamkniętej aplikacji" }), 3],
        [d.getByRole("checkbox", { name: /^Przypomnienia/ }), 4],
      ]);
    },
  },
  {
    name: "pc-zrodla-lista",
    run: async (p) => {
      await settings(p, "Źródła ofert");
      const d = dialog(p);
      await d.getByRole("region", { name: /^Adzuna/ }).evaluate((el) => el.scrollIntoView({ block: "center" }));
      await wait(p);
      await mark(p, [
        [d.getByRole("button", { name: "Adzuna Pokaż ustawienia" }), 1],
        [d.getByRole("checkbox", { name: "Remotive: Włączone" }), 2],
      ]);
    },
  },
  {
    name: "pc-alerty",
    run: async (p) => {
      await settings(p, "Źródła ofert");
      const d = dialog(p);
      await d.getByRole("button", { name: "Alerty e-mail z portali Pokaż ustawienia" }).click();
      await wait(p, 500);
      await d.getByRole("region", { name: /^Alerty e-mail z portali/ }).evaluate((el) => el.scrollIntoView({ block: "start" }));
      await p.mouse.wheel(0, -60);
      await wait(p);
      await mark(p, [
        [d.getByRole("textbox", { name: /^Adres e-mail/ }), 1],
        [d.getByText("Hasło aplikacji Google", { exact: true }), 2],
        [d.getByRole("button", { name: "Sprawdź połączenie" }), 3],
        [d.getByRole("textbox", { name: /^Słowa do alertów/ }), 4],
        [d.getByText("Jak założyć alerty na portalach"), 5],
      ]);
    },
  },
  {
    name: "pc-ai",
    run: async (p) => {
      await settings(p, "AI");
      const d = dialog(p);
      await mark(p, [
        [d.getByRole("group", { name: "Jakie AI" }), 1],
        [d.getByRole("region", { name: /^Claude \(Anthropic\)/ }), 2],
        [d.getByRole("checkbox", { name: "Ucz się z moich ocen" }), 3],
        [d.getByRole("textbox", { name: /^Dzienny limit wydatków/ }), 4],
      ]);
    },
  },
  {
    name: "pc-wyglad",
    run: async (p) => {
      await settings(p, "Wygląd");
      const d = dialog(p);
      await mark(p, [
        [d.getByRole("group", { name: "Motyw" }), 1],
        [d.getByRole("group", { name: "Wielkość tekstu" }), 2],
        [d.getByRole("group", { name: "Profile" }), 3],
        [d.getByRole("group", { name: "Lista ofert" }), 4],
      ]);
    },
  },
  {
    name: "pc-synchronizacja",
    run: async (p) => {
      await settings(p, "Synchronizacja");
      const d = dialog(p);
      await d.getByRole("checkbox", { name: "Synchronizuj z Google Dyskiem" }).check();
      await wait(p, 500);
      await mark(p, [
        [d.getByRole("checkbox", { name: "Synchronizuj z Google Dyskiem" }), 1],
        [d.getByRole("button", { name: "Połącz z Google Dyskiem" }), 2],
        [d.getByRole("button", { name: "Pokaż kod QR dla telefonu" }), 3],
      ]);
    },
  },
  {
    name: "pc-dane",
    run: async (p) => {
      await settings(p, "Dane");
      const d = dialog(p);
      await mark(p, [
        [d.getByRole("form", { name: "Zapisz kopię jako…" }), 1],
        [d.getByRole("form", { name: "Wczytaj kopię…" }), 2],
        [d.getByRole("region", { name: "Eksport do Excela (CSV)" }), 3],
      ]);
    },
  },
  {
    name: "tel-ekran-startowy",
    phone: true,
    run: async (p) =>
      mark(p, [
        [p.locator(".topbar .tabs"), 1],
        [p.getByRole("button", { name: "Dodaj ofertę" }), 2],
        [p.getByRole("button", { name: "Ustawienia", exact: true }), 3],
        [p.locator("table.board tbody tr").first(), 4],
      ]),
  },
  {
    name: "tel-karta",
    phone: true,
    run: async (p) => {
      await p.locator("table.board tbody tr").first().click();
      await wait(p, 600);
      const c = card(p);
      await mark(p, [
        [c.getByRole("button", { name: "Wróć do listy" }), 1],
        [c.locator(".pass-back-bar").getByRole("button", { name: "Wyjaśnienie" }), 2],
        [c.getByRole("group", { name: "Zainteresowanie" }), 3],
        [c.getByRole("button", { name: "Otwórz ofertę" }), 4],
      ]);
    },
  },
  {
    name: "tel-dodaj-oferte",
    phone: true,
    run: async (p) => {
      await p.getByRole("button", { name: "Dodaj ofertę" }).click();
      await wait(p);
      const d = dialog(p);
      await mark(p, [
        [d.getByRole("textbox", { name: /^Wklej treść oferty/ }), 1],
        [d.getByRole("button", { name: /^Uzupełnij pola/ }), 2],
      ]);
    },
  },
  {
    name: "tel-synchronizacja",
    phone: true,
    run: async (p) => {
      await settings(p, "Synchronizacja");
      const d = dialog(p);
      await d.getByRole("checkbox", { name: "Synchronizuj z Google Dyskiem" }).check();
      await wait(p, 500);
      await mark(p, [
        [d.getByRole("checkbox", { name: "Synchronizuj z Google Dyskiem" }), 1],
        [d.getByRole("button", { name: "Zeskanuj kod QR" }), 2],
      ]);
    },
  },
];

mkdirSync(OUT, { recursive: true });
const server = await createServer({ mode: "demo", logLevel: "error", server: { port: 5199, strictPort: false, open: false } });
await server.listen();
const url = server.resolvedUrls.local[0];
const browser = await chromium.launch({ executablePath: CHROME });
let failed = 0;
try {
  for (const shot of SHOTS) {
    if (only && !shot.name.includes(only)) continue;
    const context = await browser.newContext(
      shot.phone
        ? { viewport: { width: 400, height: 860 }, userAgent: PHONE_AGENT, hasTouch: true, isMobile: true, colorScheme: "light", reducedMotion: "reduce" }
        : { viewport: { width: 1280, height: 900 }, colorScheme: "light", reducedMotion: "reduce" },
    );
    const page = await context.newPage();
    try {
      await page.goto(url);
      await page.locator("dialog.tour[open]").waitFor();
      await wait(page, 400);
      if (!shot.tour) {
        await page.keyboard.press("Escape");
        await wait(page);
      }
      await shot.run(page);
      await page.screenshot({ path: `${OUT}/${shot.name}.png` });
      console.log("ok", shot.name);
    } catch (e) {
      failed++;
      console.error("FAILED", shot.name, e.message.split("\n")[0]);
    } finally {
      await context.close();
    }
  }
} finally {
  await browser.close();
  await server.close();
}
process.exit(failed ? 1 : 0);
