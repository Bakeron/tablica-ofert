import type {
  AddOutcome,
  SameOffer,
  DupCandidate,
  DupVerdict,
  AiCheck,
  AiProvider,
  AiStatus,
  UsdRate,
  FreeModels,
  AppStatus,
  AssistKind,
  Contact,
  EventKind,
  LogKind,
  Captured,
  Posting,
  Draft,
  Criteria,
  CareerIdea,
  Machine,
  RateReport,
  TaskStatus,
  DeviceCode,
  SyncOutcome,
  SyncStatus,
  Upcoming,
  FetchReport,
  WorkProgress,
  SecretHints,
  SecretName,
  SourceStatus,
  Interest,
  OfferDetail,
  OfferInput,
  OfferRow,
  ExportRow,
  Profile,
  ProfileInput,
  Settings,
  Stage,
  View,
  ViewCounts,
  BinPlace,
  ExpiredBy,
} from "./types";

/** The calls the UI makes. In the app they go to Rust; in a plain browser (design preview) to a mock. */
export interface Api {
  appStatus(): Promise<AppStatus>;
  /** `profile = null` - all profiles together (one fitting profile is enough). */
  listOffers(view: View, profile: number | null): Promise<OfferRow[]>;
  countOffers(profile: number | null): Promise<ViewCounts>;
  getOffer(id: number): Promise<OfferDetail>;
  /** TEMPORARY - the blind rating of 10 offers for the AI test (removed after it). */
  /** `reading` - `Draft.readingId` from "Add offer", when AI scored the offer while filling it.
   *  `place` - where the person said it goes: a separate offer, or a child of offer `parent`. */
  addOffer(offer: OfferInput, reading?: string | null, place?: { separate: true } | { parent: number }): Promise<AddOutcome>;
  /** Before "Add offer" saves: the same offer of the same portal already on the board? */
  checkSameOffer(offer: OfferInput): Promise<SameOffer>;
  /** "Update the existing one": the form's fields go into offer `id` (stage, notes, history stay),
   *  except the fields in `keep` (the person chose the old version). `renew = false` - "Fill in":
   *  only the fields change, the ad is not marked renewed. */
  updateSameOffer(id: number, offer: OfferInput, reading?: string | null, keep?: string[], renew?: boolean): Promise<void>;
  /** "Fill in that one": what the form would change in offer `id`. */
  compareOffer(id: number, offer: OfferInput): Promise<SameOffer>;
  /** "Check duplicates" on the card: offers that may be the same job (plain code). */
  findDuplicates(id: number): Promise<DupCandidate[]>;
  /** AI judges the candidates `ids` - against the form (`offer`) or an offer on the board (`id`). */
  judgeDuplicates(of: { offer: OfferInput } | { id: number }, ids: number[]): Promise<DupVerdict[]>;
  /** "Join as a child": one tree, the first added is the parent. Returns the parent. */
  joinOffers(id: number, other: number): Promise<number>;
  /** "The same company and title - the same ad?" on the card: joined (the parent) or not (`null`). */
  answerDuplicate(id: number, join: boolean): Promise<number | null>;
  /** "Renewed after your CV - send it again?": `resent` = sent once more. The mark RENEWED goes. */
  answerRenewed(id: number, resent: boolean): Promise<void>;
  /** "Detach" a child (`parent = null`), or put it back under its parent ("Undo"). */
  setOfferParent(id: number, parent: number | null): Promise<void>;
  setStage(id: number, stage: Stage): Promise<void>;
  setInterest(id: number, interest: Interest | null): Promise<void>;
  setFavorite(id: number, favorite: boolean): Promise<void>;
  /** "To own": the offer moves to "Own"; `false` = back to "Collected". */
  setOwn(id: number, own: boolean): Promise<void>;
  setNotes(id: number, notes: string): Promise<void>;
  trashOffer(id: number): Promise<void>;
  restoreOffer(id: number): Promise<void>;
  /** "Expired" on the card: nobody touched it - to the archive, otherwise marked. */
  setExpired(id: number): Promise<ExpiredBy>;
  /** "Undo" after "Expired". */
  unsetExpired(id: number): Promise<void>;
  /** "Restore" in the bin (any kind); the answer is what "Undo" needs. */
  bringBackOffer(id: number): Promise<BinPlace>;
  /** "Undo" after "Restore": back in the bin as it was. */
  putBackOffer(id: number, place: BinPlace): Promise<void>;
  /** Deletes for good one offer, or (`id = null`) everything the bin's variant `view` shows. */
  purgeOffers(id: number | null, view: View): Promise<number>;
  getSettings(): Promise<Settings>;
  setSetting<K extends keyof Settings>(key: K, value: Settings[K]): Promise<void>;
  listProfiles(): Promise<Profile[]>;
  /** Saves and re-checks every offer against the profile. Returns its id. */
  saveProfile(profile: ProfileInput): Promise<number>;
  deleteProfile(id: number): Promise<void>;
  getExperience(): Promise<string>;
  /** The experience before the last change, null = never changed. */
  getExperiencePrev(): Promise<string | null>;
  setExperience(text: string): Promise<void>;
  /** Which secrets are set - only their last 4 characters ever reach the UI. */
  secretHints(): Promise<SecretHints>;
  setSecret(name: SecretName, value: string): Promise<void>;
  deleteSecret(name: SecretName): Promise<void>;
  /** The saved value - only for "Show key" clicked in the settings. */
  revealSecret(name: SecretName): Promise<string | null>;
  sourceStatus(): Promise<SourceStatus[]>;
  /** Logs in to the alert mailbox (read-only) and returns the number of messages. */
  mailTest(): Promise<number>;
  /** "Check connection" next to a source: one small request, nothing saved; the old error goes. */
  sourceTest(source: string): Promise<void>;
  /** "Fetch now" - every enabled source that may run. */
  fetchOffers(): Promise<FetchReport>;
  /** Background fetch finished (the app collects at start and every few hours). Returns "stop listening". */
  onFetched(listener: (report: FetchReport) => void): Promise<() => void>;
  aiStatus(): Promise<AiStatus>;
  /** The dollar in zloty from NBP; null without a connection. */
  usdRate(): Promise<UsdRate | null>;
  /** "Stop rating": the run ends before its next offer. */
  aiStop(): Promise<void>;
  /** AI reads waiting offers now. */
  aiRateNow(): Promise<RateReport>;
  /** Rated offers wait for AI again, then AI reads them. */
  aiRateAllAgain(): Promise<RateReport>;
  /** "Rate again" for one offer; `precise` = "Rate more precisely" (Claude alone). */
  aiRateOffer(id: number, precise?: boolean): Promise<RateReport>;
  /** "Translate": title and description of an offer in another language. */
  aiTranslate(id: number): Promise<void>;
  /** One tiny question with the model from the settings (one call of the day). */
  aiCheck(provider: AiProvider): Promise<AiCheck>;
  /** Free models of the provider for the "Model" field. */
  aiFreeModels(provider: AiProvider): Promise<FreeModels>;
  /** Text written by AI on a click (message, questions, gaps) - never sent anywhere. */
  aiAssist(id: number, kind: AssistKind): Promise<string>;
  /** Filters proposed from a description - the user picks what to keep. */
  aiSuggestCriteria(description: string): Promise<Criteria>;
  aiSuggestCareers(description: string, experience: string): Promise<CareerIdea[]>;
  /** A clearer version of the user's own text. */
  aiTidy(text: string): Promise<string>;
  checkHardware(): Promise<Machine>;
  /** Models already in Ollama; rejects when Ollama does not answer. */
  ollamaModels(): Promise<string[]>;
  /** Downloads a model into Ollama (progress in "work-progress"). */
  ollamaPull(model: string): Promise<void>;
  /** Official installer of Ollama; empty folders = default places (progress in "work-progress"). */
  ollamaInstall(program: string, models: string): Promise<void>;
  /** Folder chosen in the system window; null = closed. */
  pickFolder(): Promise<string | null>;
  /** Fields from a pasted text or a page: AI when set up, plain code otherwise. */
  fillOffer(text: string, title: string | null, posting: Posting | null): Promise<Draft>;
  /** Opens the offer page in the preview window. */
  openPreview(url: string): Promise<void>;
  /** "Take the text from the page" in the preview window. */
  capturePreview(): Promise<Captured>;
  /** "Take the text from the page" in one click: opens the page, takes its text, closes it. */
  fetchPage(url: string): Promise<Captured>;
  closePreview(): Promise<void>;
  /** Is the Task Scheduler task really there? */
  backgroundStatus(): Promise<TaskStatus>;
  /** Turns collecting with the app closed on or off; returns the real state. */
  setBackground(on: boolean): Promise<TaskStatus>;
  addEvent(offerId: number, event: { kind: EventKind; at: string; minutes: number; note: string }): Promise<number>;
  deleteEvent(id: number): Promise<void>;
  /** `contact.id = 0` - a new contact. */
  saveContact(offerId: number, contact: Contact): Promise<number>;
  deleteContact(id: number): Promise<void>;
  addLog(offerId: number, entry: { at: string; kind: LogKind; note: string }): Promise<number>;
  deleteLog(id: number): Promise<void>;
  /** Opens Google Calendar with the event filled in (the person clicks "Save"). */
  calendarGoogle(eventId: number): Promise<void>;
  /** Saves an .ics file to Downloads and opens it; returns the path. */
  calendarIcs(eventId: number): Promise<string>;
  /** "Save a copy as…" with a password; returns the path, null = cancelled. */
  saveCopy(password: string): Promise<string | null>;
  /** "Load a copy…" - replaces all data; returns the number of offers, null = cancelled. */
  loadCopy(password: string): Promise<number | null>;
  /** Offers for "Export to Excel (CSV)": one tab, or every offer (the trash too) for null. */
  exportRows(view: View | null): Promise<ExportRow[]>;
  /** Saves the CSV cells (words in the app language); `name` - the list, part of the file name. Returns the path, null = cancelled. */
  saveCsv(name: string, cells: string[][], separator: string): Promise<string | null>;
  /** Deletes all data, keys and the scheduled task from this computer, then closes the app. */
  wipeAll(): Promise<void>;
  /** An interview starts in an hour (for the optional gong). */
  onReminder(listener: () => void): Promise<() => void>;
  syncStatus(): Promise<SyncStatus>;
  /** Step 1 of the Google login: a code to type at google.com/device. */
  syncLoginStart(): Promise<DeviceCode>;
  /** Step 2: true = logged in, false = not yet. */
  syncLoginPoll(deviceCode: string): Promise<boolean>;
  /** Download, merge, upload. `confirm` = several offers may be deleted. */
  syncNow(confirm: boolean): Promise<SyncOutcome>;
  /** QR code (SVG) with the sync key for the phone. */
  syncQr(): Promise<string>;
  /** New key - the old phone can no longer read the file. */
  syncNewKey(): Promise<void>;
  /** Deletes the sync file from Google Drive. */
  syncDeleteRemote(): Promise<void>;
  /** Forgets the Google login on this computer. */
  syncDisconnect(): Promise<void>;
  /** The phone saves the key scanned from the PC's QR code. */
  /** Saves what the phone scanned; the Client ID when the code carried the PC's Google project. */
  syncFromQr(text: string): Promise<string | null>;
  /** Future dates of all offers (phone reminders). */
  upcomingEvents(): Promise<Upcoming[]>;
  /** Saves a longer description from the offer page; returns whether it changed. */
  updateDescription(id: number, text: string): Promise<boolean>;
  /** AI finished reading offers in the background. */
  onRated(listener: (report: RateReport) => void): Promise<() => void>;
  /** Every step of fetching and of AI reading offers; null = the work is over. */
  onProgress(listener: (step: WorkProgress | null) => void): Promise<() => void>;
}

function insideTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function tauriApi(): Promise<Api> {
  const { invoke } = await import("@tauri-apps/api/core");
  return {
    appStatus: () => invoke("app_status"),
    listOffers: (view, profile) => invoke("list_offers", { view, profile }),
    countOffers: (profile) => invoke("count_offers", { profile }),
    getOffer: (id) => invoke("get_offer", { id }),
    addOffer: (offer, reading, place) =>
      invoke("add_offer", {
        offer,
        reading: reading ?? null,
        separate: place && "separate" in place ? true : null,
        parent: place && "parent" in place ? place.parent : null,
      }),
    checkSameOffer: (offer) => invoke("check_same_offer", { offer }),
    updateSameOffer: (id, offer, reading, keep, renew) =>
      invoke("update_same_offer", { id, offer, reading: reading ?? null, keep: keep ?? [], renew: renew ?? true }),
    compareOffer: (id, offer) => invoke("compare_offer", { id, offer }),
    findDuplicates: (id) => invoke("find_duplicates", { id }),
    judgeDuplicates: (of, ids) =>
      invoke("judge_duplicates", { offer: "offer" in of ? of.offer : null, id: "id" in of ? of.id : null, ids }),
    joinOffers: (id, other) => invoke("join_offers", { id, other }),
    answerDuplicate: (id, join) => invoke("answer_duplicate", { id, join }),
    answerRenewed: (id, resent) => invoke("answer_renewed", { id, resent }),
    setOfferParent: (id, parent) => invoke("set_offer_parent", { id, parent }),
    setStage: (id, stage) => invoke("set_stage", { id, stage }),
    setInterest: (id, interest) => invoke("set_interest", { id, interest }),
    setFavorite: (id, favorite) => invoke("set_favorite", { id, favorite }),
    setOwn: (id, own) => invoke("set_own", { id, own }),
    setNotes: (id, notes) => invoke("set_notes", { id, notes }),
    trashOffer: (id) => invoke("trash_offer", { id }),
    restoreOffer: (id) => invoke("restore_offer", { id }),
    setExpired: (id) => invoke("set_expired", { id }),
    unsetExpired: (id) => invoke("unset_expired", { id }),
    bringBackOffer: (id) => invoke("bring_back_offer", { id }),
    putBackOffer: (id, place) => invoke("put_back_offer", { id, place }),
    purgeOffers: (id, view) => invoke("purge_offers", { id, view }),
    getSettings: () => invoke("get_settings"),
    setSetting: (key, value) => invoke("set_setting", { key, value }),
    listProfiles: () => invoke("list_profiles"),
    saveProfile: (profile) => invoke("save_profile", { profile }),
    deleteProfile: (id) => invoke("delete_profile", { id }),
    getExperience: () => invoke("get_experience"),
    getExperiencePrev: () => invoke("get_experience_prev"),
    setExperience: (text) => invoke("set_experience", { text }),
    secretHints: () => invoke("secret_hints"),
    setSecret: (name, value) => invoke("set_secret", { name, value }),
    deleteSecret: (name) => invoke("delete_secret", { name }),
    revealSecret: (name) => invoke("reveal_secret", { name }),
    sourceStatus: () => invoke("source_status"),
    mailTest: () => invoke("mail_test"),
    sourceTest: (source) => invoke("source_test", { source }),
    fetchOffers: () => invoke("fetch_offers"),
    onFetched: async (listener) => {
      const { listen } = await import("@tauri-apps/api/event");
      return listen<FetchReport>("offers-fetched", (e) => listener(e.payload));
    },
    aiStatus: () => invoke("ai_status"),
    usdRate: () => invoke("usd_rate"),
    aiStop: () => invoke("ai_stop"),
    aiRateNow: () => invoke("ai_rate_now"),
    aiRateAllAgain: () => invoke("ai_rate_all_again"),
    aiRateOffer: (id, precise) => invoke("ai_rate_offer", { id, precise: precise ?? false }),
    aiTranslate: (id) => invoke("ai_translate", { id }),
    aiCheck: (provider) => invoke("ai_check", { provider }),
    aiFreeModels: (provider) => invoke("ai_free_models", { provider }),
    aiAssist: (id, kind) => invoke("ai_assist", { id, kind }),
    aiSuggestCriteria: (description) => invoke("ai_suggest_criteria", { description }),
    aiSuggestCareers: (description, experience) => invoke("ai_suggest_careers", { description, experience }),
    aiTidy: (text) => invoke("ai_tidy", { text }),
    checkHardware: () => invoke("check_hardware"),
    ollamaModels: () => invoke("ollama_models"),
    ollamaPull: (model) => invoke("ollama_pull", { model }),
    ollamaInstall: (program, models) => invoke("ollama_install", { program, models }),
    pickFolder: () => invoke("pick_folder"),
    fillOffer: (text, title, posting) => invoke("fill_offer", { text, title, posting }),
    openPreview: (url) => invoke("open_preview", { url }),
    capturePreview: () => invoke("capture_preview"),
    fetchPage: (url) => invoke("fetch_page", { url }),
    closePreview: () => invoke("close_preview"),
    backgroundStatus: () => invoke("background_status"),
    setBackground: (on) => invoke("set_background", { on }),
    addEvent: (offerId, event) => invoke("add_event", { offerId, event }),
    deleteEvent: (id) => invoke("delete_event", { id }),
    saveContact: (offerId, contact) => invoke("save_contact", { offerId, contact }),
    deleteContact: (id) => invoke("delete_contact", { id }),
    addLog: (offerId, entry) => invoke("add_log", { offerId, entry }),
    deleteLog: (id) => invoke("delete_log", { id }),
    calendarGoogle: (id) => invoke("calendar_google", { id }),
    calendarIcs: (id) => invoke("calendar_ics", { id }),
    saveCopy: (password) => invoke("save_copy", { password }),
    loadCopy: (password) => invoke("load_copy", { password }),
    exportRows: (view) => invoke("export_rows", { view }),
    saveCsv: (name, cells, separator) => invoke("save_csv", { name, cells, separator }),
    wipeAll: () => invoke("wipe_all"),
    syncStatus: () => invoke("sync_status"),
    syncLoginStart: () => invoke("sync_login_start"),
    syncLoginPoll: (deviceCode) => invoke("sync_login_poll", { deviceCode }),
    syncNow: (confirm) => invoke("sync_now", { confirm }),
    syncQr: () => invoke("sync_qr"),
    syncNewKey: () => invoke("sync_new_key"),
    syncDeleteRemote: () => invoke("sync_delete_remote"),
    syncDisconnect: () => invoke("sync_disconnect"),
    syncFromQr: (text) => invoke("sync_from_qr", { text }),
    upcomingEvents: () => invoke("upcoming_events"),
    onReminder: async (listener) => {
      const { listen } = await import("@tauri-apps/api/event");
      return listen("reminder", () => listener());
    },
    updateDescription: (id, text) => invoke("update_description", { id, text }),
    onRated: async (listener) => {
      const { listen } = await import("@tauri-apps/api/event");
      return listen<RateReport>("offers-rated", (e) => listener(e.payload));
    },
    onProgress: async (listener) => {
      const { listen } = await import("@tauri-apps/api/event");
      return listen<WorkProgress | null>("work-progress", (e) => listener(e.payload));
    },
  };
}

export async function createApi(): Promise<Api> {
  if (insideTauri()) return tauriApi();
  // Only for `npm run dev` in a normal browser - never part of the installed app's data path.
  const { createMockApi } = await import("./mock");
  return createMockApi();
}
