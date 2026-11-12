/** How connecting to Google Drive ended after "back" stopped the waiting. It takes one last look
 *  first (`poll`), so a login approved a moment before "back" still connects; errors never throw -
 *  a failed look or status counts as "not connected". */
export async function stoppedConnect(
  poll: () => Promise<unknown>,
  status: () => Promise<{ connected: boolean }>,
): Promise<"sync.connected" | "sync.connectStopped"> {
  await poll().catch(() => false);
  const connected = await status().then((s) => s.connected).catch(() => false);
  return connected ? "sync.connected" : "sync.connectStopped";
}
