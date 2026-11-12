/** Whether a form holds anything the author typed: a field differs from the empty form, or one of
 *  the loose texts (link, pasted page) is more than spaces. While saving, nothing is "unsaved". */
export function formDirty<T extends object>(form: T, empty: T, texts: string[], busy = false): boolean {
  if (busy) return false;
  return JSON.stringify(form) !== JSON.stringify(empty) || texts.some((text) => text.trim() !== "");
}
