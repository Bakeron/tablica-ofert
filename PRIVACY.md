# Polityka prywatności - Tablica Ofert

[English version below](#privacy-policy---tablica-ofert)

Stan na 6.11.2026. Tablica Ofert to aplikacja na komputer i telefon z Androidem, bez kont i bez własnego serwera. Autor aplikacji nie dostaje żadnych Twoich danych.

## Jakie dane i gdzie

- **Oferty, profile „czego szukam”, doświadczenie, notatki, terminy, kontakty** - tylko na Twoich urządzeniach (komputer, telefon) i, gdy włączysz synchronizację, w zaszyfrowanym pliku na Twoim Google Dysku. Na komputerze są w zaszyfrowanej bazie (SQLCipher) w folderze `%APPDATA%\pl.prywatne.tablicaofert\`. Klucz bazy leży w Menedżerze poświadczeń Windows; druga kopia klucza (plik `tablica.key` obok bazy) jest zaszyfrowana przez Windows (DPAPI) i da się ją odczytać tylko na Twoim koncie Windows - na wypadek, gdyby klucz zniknął z Menedżera.
- **Klucze API, hasło do skrzynki z alertami** - tylko w Menedżerze poświadczeń Windows. Nie trafiają do bazy, kopii, eksportu CSV, logów ani do AI. Na ekranie widać tylko 4 ostatnie znaki, a cały klucz - dopiero po kliknięciu „Pokaż”.
- **Kopie zapasowe** - 5 ostatnich kopii bazy w podfolderze `kopie` (zaszyfrowane tym samym kluczem). „Zapisz kopię jako…” tworzy plik zabezpieczony hasłem, które wybierasz.
- **Dziennik zdarzeń** (`zdarzenia.log` obok bazy) - przy każdym starcie godzina, wersja i to, czy klucz bazy był na miejscu. Bez kluczy, ofert i danych osobowych; nie wychodzi z urządzenia.

## Z czym aplikacja się łączy

Tylko z tym, co włączysz w ustawieniach:

- **Adzuna** (agregator ofert) - wysyłane są słowa kluczowe z profilu (albo „Słowa do alertów”, gdy je wpiszesz), miasto z profilu oraz Twój klucz Adzuna.
- **Remotive** (praca zdalna) - wysyłane są słowa kluczowe z profilu.
- **We Work Remotely, Remote OK** (praca zdalna) - aplikacja pobiera publiczną listę ofert i nic o Tobie nie wysyła (słowa kluczowe sprawdza u siebie).
- **Jooble** (agregator ofert) - wysyłane są słowo kluczowe z profilu (albo ze „Słów do alertów”, gdy je wpiszesz), miasto z profilu oraz Twój klucz Jooble.
- **Himalayas** (praca zdalna) - wysyłane są słowa kluczowe z profilu i kraj wybrany w „Kraj ofert”. Raz dziennie aplikacja pyta Himalayas o każdą pobraną od nich ofertę, czy jest jeszcze otwarta (to warunek Himalayas: zamknięte oferty trzeba usuwać).
- **Twoja skrzynka z alertami** (IMAP) - aplikacja tylko czyta maile; niczego nie usuwa, nie oznacza jako przeczytane i nie wysyła.
- **Włączeni dostawcy AI** (Gemini, Groq, OpenRouter, Claude, OpenAI) - AI pyta ich po kolei (po dziennym limicie jednego - następnego), więc dane jednej oferty mogą trafić do każdego z włączonych; wysyłana jest treść oferty, opisy profili, oczekiwania finansowe i doświadczenie. Nigdy klucze, hasła ani notatki. Przy Ollamie AI działa na Twoim komputerze i nic nie wychodzi na zewnątrz. Dane w usłudze AI podlegają polityce prywatności tej usługi.
- **Strony ofert otwarte przez Ciebie** w oknie podglądu.
- **ollama.com i GitHub** (gdy klikniesz „Zainstaluj Ollamę za mnie”) - aplikacja pobiera oficjalny instalator Ollamy i nic o Tobie nie wysyła.
- **Google Dysk** (gdy włączysz synchronizację z telefonem) - aplikacja widzi tylko pliki, które sama utworzyła, i zapisuje w „Moim dysku” jeden plik z danymi (`tablica-ofert.sync`), zaszyfrowany kluczem synchronizacji, którego Google nie zna. Token dostępu do Dysku leży w sejfie systemu.
- **Kalendarz Google** (gdy klikniesz „Kalendarz Google” przy terminie) - w Twojej przeglądarce otwiera się Kalendarz z wypełnionym terminem: stanowisko, firma, notatka terminu i link do oferty.

Aplikacja nie ma reklam ani własnej analityki. Jedyny wyjątek to ML Kit w skanerze kodu QR na telefonie (opis niżej).

## Na telefonie (Android)

Telefon nie zbiera ofert i nie pyta AI - robi to komputer, a telefon dostaje wyniki przez synchronizację. Łączy się z Google Dyskiem (gdy włączysz synchronizację) i otwiera w przeglądarce strony, które sam wybierzesz, a poza tym:

- **Dane** - zaszyfrowana baza w prywatnym folderze aplikacji, obok niej 5 ostatnich kopii bazy i dziennik zdarzeń. Inne aplikacje nie mają tam dostępu, a kopia zapasowa Androida jest dla tej aplikacji wyłączona.
- **Klucze** - klucz bazy, klucz synchronizacji, logowanie do Dysku i dane klienta Google są w sejfie Androida (Android Keystore).
- **Aparat** - tylko do skanowania kodu QR z komputera, gdy sam je włączysz. Aplikacja nie zapisuje obrazu z aparatu. Kod QR niesie klucz synchronizacji i dane klienta Google (przy własnym projekcie Google także jego sekret), więc nie pokazuj go innym.
- **Powiadomienia** - przypomnienia o terminach planuje sam telefon, bez żadnego serwera. Android pyta o zgodę, a odmowa wyłącza tylko przypomnienia.
- **„Udostępnij”** - tekst lub link wysłany z innej aplikacji otwiera okno dodawania oferty. Zapisana oferta jest traktowana jak każda inna.
- **ML Kit (Google)** - skaner kodu QR korzysta z ML Kit z usług Google Play. Według Google (stan na 14.05.2025) ML Kit wysyła do Google przez HTTPS informacje o urządzeniu, nazwę i wersję aplikacji, dane o wydajności, konfigurację API, rozmiar danych wejściowych i wyjściowych, rodzaje zdarzeń, kody błędów i identyfikatory urządzenia - do diagnostyki i analizy użycia, bez przekazywania firmom trzecim. Obrazu z aparatu ani treści kodu Google w tym wykazie nie wymienia. Wyłączyć się tego nie da, a dane mogą iść także bez skanowania, bo ML Kit startuje razem z aplikacją. Autor aplikacji tych danych nie dostaje. Opis Google: [ML Kit - ujawnianie danych](https://developers.google.com/ml-kit/android-data-disclosure).

## Twoje prawa i usuwanie danych

Wszystkie dane są u Ciebie. Ustawienia → Dane → „Usuń wszystkie dane” kasuje bazę, kopie, kopię klucza bazy, dziennik zdarzeń, klucze z Menedżera poświadczeń i zadanie w Harmonogramie zadań. Odinstalowanie usuwa zadanie w Harmonogramie zadań.

Na telefonie dane usuwa odinstalowanie aplikacji albo „Wyczyść dane” w ustawieniach Androida. Plik na Google Dysku usuwa „Usuń z Dysku” w Ustawienia → Synchronizacja.

---

# Privacy policy - Tablica Ofert

As of 6 November 2026. Tablica Ofert is a desktop and Android app with no accounts and no server of its own. The author of the app receives none of your data.

## What data and where

- **Offers, “what I'm looking for” profiles, experience, notes, dates, contacts** - only on your devices (computer, phone) and, when you turn on sync, in an encrypted file on your Google Drive. On the computer they are in an encrypted database (SQLCipher) in `%APPDATA%\pl.prywatne.tablicaofert\`. The database key is kept in Windows Credential Manager; a second copy of the key (the `tablica.key` file next to the database) is encrypted by Windows (DPAPI) and can only be read under your Windows account - in case the key ever disappears from Credential Manager.
- **API keys, the alert mailbox password** - only in Windows Credential Manager. They never go to the database, copies, CSV export, logs or AI. Only the last 4 characters are shown; the whole key only after clicking “Show”.
- **Backups** - the 5 newest copies of the database in the `kopie` subfolder (encrypted with the same key). “Save a copy as…” creates a file protected with a password you choose.
- **Event log** (`zdarzenia.log` next to the database) - on every start the time, the version and whether the database key was in place. No keys, offers or personal data; it never leaves the device.

## What the app connects to

Only what you turn on in the settings:

- **Adzuna** (job aggregator) - profile keywords (or “Words for alerts”, when you fill them in), the profile city and your Adzuna key.
- **Remotive** (remote jobs) - profile keywords.
- **We Work Remotely, Remote OK** (remote jobs) - the app downloads their public list of offers and sends nothing about you (keywords are matched on your computer).
- **Jooble** (job aggregator) - a profile keyword (or one of the “Words for alerts”, when you fill them in), the profile city and your Jooble key.
- **Himalayas** (remote jobs) - profile keywords and the country chosen in “Country of offers”. Once a day the app asks Himalayas whether each offer it got from them is still open (a Himalayas condition: closed jobs must be removed).
- **Your alert mailbox** (IMAP) - read only; nothing is deleted, marked as read or sent.
- **The AI providers you turn on** (Gemini, Groq, OpenRouter, Claude, OpenAI) - AI asks them in turn (after the daily limit of one - the next), so the data of one offer may reach any of them; sent are the offer text, profile descriptions, salary expectations and experience. Never keys, passwords or notes. With Ollama, AI runs on your computer and nothing leaves it. Data at an AI service is covered by that service's privacy policy.
- **Offer pages you open yourself** in the preview window.
- **ollama.com and GitHub** (when you click “Install Ollama for me”) - the app downloads the official Ollama installer and sends nothing about you.
- **Google Drive** (when you turn on sync with your phone) - the app sees only the files it made and keeps one data file (`tablica-ofert.sync`) in “My Drive”, encrypted with the sync key, which Google does not know. The Drive access token is kept in the system vault.
- **Google Calendar** (when you click “Google Calendar” next to a date) - Calendar opens in your browser with the event filled in: job title, company, the event note and the offer link.

The app has no ads and no analytics of its own. The only exception is ML Kit in the QR code scanner on the phone (described below).

## On the phone (Android)

The phone does not collect offers and does not ask AI - the computer does that, and the phone gets the results through sync. It connects to Google Drive (when you turn on sync) and opens pages you choose in the browser, and beyond that:

- **Data** - an encrypted database in the app's private folder, next to it the 5 newest copies of the database and the event log. Other apps have no access there, and Android backup is turned off for this app.
- **Keys** - the database key, the sync key, the Drive login and the Google client details are in the Android vault (Android Keystore).
- **Camera** - only to scan the QR code from the computer, when you start it yourself. The app does not save the camera image. The QR code carries the sync key and the Google client details (with your own Google project also its secret), so do not show it to others.
- **Notifications** - reminders about dates are planned by the phone itself, with no server. Android asks for consent, and refusing only turns off the reminders.
- **“Share”** - text or a link sent from another app opens the add offer window. A saved offer is treated like any other.
- **ML Kit (Google)** - the QR code scanner uses ML Kit from Google Play services. According to Google (as of 14 May 2025) ML Kit sends Google, over HTTPS, device information, the app name and version, performance data, API configuration, input and output size, event types, error codes and device identifiers - for diagnostics and usage analytics, without sharing with third parties. Google does not list the camera image or the content of the code. This cannot be turned off, and the data may go out even without scanning, because ML Kit starts with the app. The author of the app does not receive this data. Google's description: [ML Kit data disclosure](https://developers.google.com/ml-kit/android-data-disclosure).

## Your rights and deleting data

All data is with you. Settings → Data → “Delete all data” removes the database, the backups, the database key copy, the event log, the keys in Credential Manager and the Task Scheduler task. Uninstalling removes the Task Scheduler task.

On the phone, uninstalling the app or “Clear data” in the Android settings removes the data. “Delete from Drive” in Settings → Sync removes the file on Google Drive.
