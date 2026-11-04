# Polityka prywatności - Tablica Ofert

[English version below](#privacy-policy---tablica-ofert)

Stan na 4.11.2026. Tablica Ofert to aplikacja na komputer i telefon z Androidem, bez kont i bez własnego serwera. Autor aplikacji nie dostaje żadnych Twoich danych.

## Jakie dane i gdzie

- **Oferty, profile „czego szukam”, doświadczenie, notatki, terminy, kontakty** - tylko na Twoim komputerze, w zaszyfrowanej bazie (SQLCipher) w folderze `%APPDATA%\pl.prywatne.tablicaofert\`. Klucz bazy leży w Menedżerze poświadczeń Windows; druga kopia klucza (plik `tablica.key` obok bazy) jest zaszyfrowana przez Windows (DPAPI) i da się ją odczytać tylko na Twoim koncie Windows - na wypadek, gdyby klucz zniknął z Menedżera.
- **Klucze API, hasło do skrzynki z alertami** - tylko w Menedżerze poświadczeń Windows. Nie trafiają do bazy, kopii, eksportu CSV, logów ani do AI. Na ekranie widać tylko 4 ostatnie znaki, a cały klucz - dopiero po kliknięciu „Pokaż”.
- **Kopie zapasowe** - 5 ostatnich kopii bazy w podfolderze `kopie` (zaszyfrowane tym samym kluczem). „Zapisz kopię jako…” tworzy plik zabezpieczony hasłem, które wybierasz.
- **Dziennik zdarzeń** (`zdarzenia.log` obok bazy) - przy każdym starcie godzina, wersja i to, czy klucz bazy był na miejscu. Bez kluczy, ofert i danych osobowych; nie wychodzi z komputera.

## Z czym aplikacja się łączy

Tylko z tym, co włączysz w ustawieniach:

- **Adzuna** (agregator ofert) - wysyłane są słowa kluczowe i miasto z profilu oraz Twój klucz Adzuna.
- **Remotive** (praca zdalna) - wysyłane są słowa kluczowe z profilu.
- **We Work Remotely, Remote OK** (praca zdalna) - aplikacja pobiera publiczną listę ofert i nic o Tobie nie wysyła (słowa kluczowe sprawdza u siebie).
- **Jooble** (agregator ofert) - wysyłane są słowo kluczowe i miasto z profilu oraz Twój klucz Jooble.
- **Himalayas** (praca zdalna) - wysyłane są słowa kluczowe z profilu i kraj („PL”). Raz dziennie aplikacja pyta Himalayas o każdą pobraną od nich ofertę, czy jest jeszcze otwarta (to warunek Himalayas: zamknięte oferty trzeba usuwać).
- **Twoja skrzynka z alertami** (IMAP) - aplikacja tylko czyta maile; niczego nie usuwa, nie oznacza jako przeczytane i nie wysyła.
- **Włączeni dostawcy AI** (Gemini, Groq, OpenRouter, Claude, OpenAI) - AI pyta ich po kolei (po dziennym limicie jednego - następnego), więc dane jednej oferty mogą trafić do każdego z włączonych; wysyłana jest treść oferty, opisy profili, oczekiwania finansowe i doświadczenie. Nigdy klucze, hasła ani notatki. Przy Ollamie AI działa na Twoim komputerze i nic nie wychodzi na zewnątrz. Dane w usłudze AI podlegają polityce prywatności tej usługi.
- **Strony ofert otwarte przez Ciebie** w oknie podglądu.
- **Google Dysk** (gdy włączysz synchronizację z telefonem) - aplikacja widzi tylko swój ukryty folder na Twoim Dysku i zapisuje tam jeden plik z danymi, zaszyfrowany kluczem synchronizacji, którego Google nie zna. Token dostępu do Dysku leży w sejfie systemu.
- **Kalendarz Google** (gdy klikniesz „Dodaj do kalendarza”) - w Twojej przeglądarce otwiera się Kalendarz z wypełnionym terminem: stanowisko, firma, notatka terminu i link do oferty.

Aplikacja nie ma analityki, reklam ani żadnych „telemetrycznych” połączeń.

## Twoje prawa i usuwanie danych

Wszystkie dane są u Ciebie. Ustawienia → Dane → „Usuń wszystkie dane” kasuje bazę, kopie, kopię klucza bazy, dziennik zdarzeń, klucze z Menedżera poświadczeń i zadanie w Harmonogramie zadań. Odinstalowanie usuwa zadanie w Harmonogramie zadań.

---

# Privacy policy - Tablica Ofert

As of 4 November 2026. Tablica Ofert is a desktop and Android app with no accounts and no server of its own. The author of the app receives none of your data.

## What data and where

- **Offers, "what I'm looking for" profiles, experience, notes, dates, contacts** - only on your computer, in an encrypted database (SQLCipher) in `%APPDATA%\pl.prywatne.tablicaofert\`. The database key is kept in Windows Credential Manager; a second copy of the key (the `tablica.key` file next to the database) is encrypted by Windows (DPAPI) and can only be read under your Windows account - in case the key ever disappears from Credential Manager.
- **API keys, the alert mailbox password** - only in Windows Credential Manager. They never go to the database, copies, CSV export, logs or AI. Only the last 4 characters are shown; the whole key only after clicking "Show".
- **Backups** - the 5 newest copies of the database in the `kopie` subfolder (encrypted with the same key). "Save a copy as…" creates a file protected with a password you choose.
- **Event log** (`zdarzenia.log` next to the database) - on every start the time, the version and whether the database key was in place. No keys, offers or personal data; it never leaves the computer.

## What the app connects to

Only what you turn on in the settings:

- **Adzuna** (job aggregator) - profile keywords and city, and your Adzuna key.
- **Remotive** (remote jobs) - profile keywords.
- **We Work Remotely, Remote OK** (remote jobs) - the app downloads their public list of offers and sends nothing about you (keywords are matched on your computer).
- **Jooble** (job aggregator) - a profile keyword and city, and your Jooble key.
- **Himalayas** (remote jobs) - profile keywords and the country ("PL"). Once a day the app asks Himalayas whether each offer it got from them is still open (a Himalayas condition: closed jobs must be removed).
- **Your alert mailbox** (IMAP) - read only; nothing is deleted, marked as read or sent.
- **The AI providers you turn on** (Gemini, Groq, OpenRouter, Claude, OpenAI) - AI asks them in turn (after the daily limit of one - the next), so the data of one offer may reach any of them; sent are the offer text, profile descriptions, salary expectations and experience. Never keys, passwords or notes. With Ollama, AI runs on your computer and nothing leaves it. Data at an AI service is covered by that service's privacy policy.
- **Offer pages you open yourself** in the preview window.
- **Google Drive** (when you turn on sync with your phone) - the app sees only its own hidden folder in your Drive and keeps one data file there, encrypted with the sync key, which Google does not know. The Drive access token is kept in the system vault.
- **Google Calendar** (when you click "Add to calendar") - Calendar opens in your browser with the event filled in: job title, company, the event note and the offer link.

The app has no analytics, no ads and no telemetry.

## Your rights and deleting data

All data is with you. Settings → Data → "Delete all data" removes the database, the backups, the database key copy, the event log, the keys in Credential Manager and the Task Scheduler task. Uninstalling removes the Task Scheduler task.
