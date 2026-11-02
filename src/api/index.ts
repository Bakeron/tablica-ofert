import type {
  AddOutcome,
  AiStatus,
  AppStatus,
  AssistKind,
  Criteria,
  Machine,
  RateReport,
  FetchReport,
  SecretHints,
  SecretName,
  SourceStatus,
  Interest,
  OfferDetail,
  OfferInput,
  OfferRow,
  Profile,
  ProfileInput,
  Settings,
  Stage,
  View,
  ViewCounts,
} from "./types";

/** The calls the UI makes. In the app they go to Rust; in a plain browser (design preview) to a mock. */
export interface Api {
  appStatus(): Promise<AppStatus>;
  /** `profile = null` - all profiles together (one fitting profile is enough).
   *  `showLow` - also offers hidden by "hide offers scored below X". */
  listOffers(view: View, profile: number | null, showLow?: boolean): Promise<OfferRow[]>;
  countOffers(profile: number | null): Promise<ViewCounts>;
  getOffer(id: number): Promise<OfferDetail>;
  addOffer(offer: OfferInput): Promise<AddOutcome>;
  setStage(id: number, stage: Stage): Promise<void>;
  setInterest(id: number, interest: Interest | null): Promise<void>;
  setFavorite(id: number, favorite: boolean): Promise<void>;
  setNotes(id: number, notes: string): Promise<void>;
  trashOffer(id: number): Promise<void>;
  restoreOffer(id: number): Promise<void>;
  purgeOffers(id: number | null): Promise<number>;
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
  sourceStatus(): Promise<SourceStatus[]>;
  /** Logs in to the alert mailbox (read-only) and returns the number of messages. */
  mailTest(): Promise<number>;
  /** "Fetch now" - every enabled source that may run. */
  fetchOffers(): Promise<FetchReport>;
  /** Background fetch finished (the app collects at start and every few hours). Returns "stop listening". */
  onFetched(listener: (report: FetchReport) => void): Promise<() => void>;
  aiStatus(): Promise<AiStatus>;
  /** AI reads waiting offers now. */
  aiRateNow(): Promise<RateReport>;
  /** "Rate again" for one offer. */
  aiRateOffer(id: number): Promise<RateReport>;
  /** Text written by AI on a click (message, questions, gaps) - never sent anywhere. */
  aiAssist(id: number, kind: AssistKind): Promise<string>;
  /** Filters proposed from a description - the user picks what to keep. */
  aiSuggestCriteria(description: string): Promise<Criteria>;
  /** A clearer version of the user's own text. */
  aiTidy(text: string): Promise<string>;
  checkHardware(): Promise<Machine>;
  /** AI finished reading offers in the background. */
  onRated(listener: (report: RateReport) => void): Promise<() => void>;
}

function insideTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function tauriApi(): Promise<Api> {
  const { invoke } = await import("@tauri-apps/api/core");
  return {
    appStatus: () => invoke("app_status"),
    listOffers: (view, profile, showLow) => invoke("list_offers", { view, profile, showLow: showLow ?? false }),
    countOffers: (profile) => invoke("count_offers", { profile }),
    getOffer: (id) => invoke("get_offer", { id }),
    addOffer: (offer) => invoke("add_offer", { offer }),
    setStage: (id, stage) => invoke("set_stage", { id, stage }),
    setInterest: (id, interest) => invoke("set_interest", { id, interest }),
    setFavorite: (id, favorite) => invoke("set_favorite", { id, favorite }),
    setNotes: (id, notes) => invoke("set_notes", { id, notes }),
    trashOffer: (id) => invoke("trash_offer", { id }),
    restoreOffer: (id) => invoke("restore_offer", { id }),
    purgeOffers: (id) => invoke("purge_offers", { id }),
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
    sourceStatus: () => invoke("source_status"),
    mailTest: () => invoke("mail_test"),
    fetchOffers: () => invoke("fetch_offers"),
    onFetched: async (listener) => {
      const { listen } = await import("@tauri-apps/api/event");
      return listen<FetchReport>("offers-fetched", (e) => listener(e.payload));
    },
    aiStatus: () => invoke("ai_status"),
    aiRateNow: () => invoke("ai_rate_now"),
    aiRateOffer: (id) => invoke("ai_rate_offer", { id }),
    aiAssist: (id, kind) => invoke("ai_assist", { id, kind }),
    aiSuggestCriteria: (description) => invoke("ai_suggest_criteria", { description }),
    aiTidy: (text) => invoke("ai_tidy", { text }),
    checkHardware: () => invoke("check_hardware"),
    onRated: async (listener) => {
      const { listen } = await import("@tauri-apps/api/event");
      return listen<RateReport>("offers-rated", (e) => listener(e.payload));
    },
  };
}

export async function createApi(): Promise<Api> {
  if (insideTauri()) return tauriApi();
  // Only for `npm run dev` in a normal browser - never part of the installed app's data path.
  const { createMockApi } = await import("./mock");
  return createMockApi();
}
