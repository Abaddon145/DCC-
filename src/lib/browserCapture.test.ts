import { afterEach, describe, expect, it, vi } from "vitest";
import { base64, CHUNK_SIZE, extensionFor, folderPath, IMAGE_LIMIT, publicUrl, sourceUrl, resultText, VIDEO_LIMIT } from "../../browser-extension/capture-utils.js";

describe("browser capture extension helpers", () => {
  it("accepts public HTTP/HTTPS URLs without fragments but refuses unsafe address forms", () => {
    expect(publicUrl("https://example.com/image.png?size=2#preview")).toBe("https://example.com/image.png?size=2");
    expect(publicUrl("http://example.com/video.mp4")).toBe("http://example.com/video.mp4");
    for (const url of ["file:///C:/private.png", "javascript:alert(1)", "https://user:secret@example.com/file",
      "http://localhost/image", "http://pictures.local/image", "http://intranet.internal/image",
      "http://127.0.0.1/image", "http://2130706433/image", "http://0x7f000001/image", "http://[::1]/image",
      "http://example.com:8888/image", "not a URL"]) {
      expect(() => publicUrl(url), url).toThrow();
    }
  });

  it("retains useful source query fields but strips credentials before leaving the browser", () => {
    expect(sourceUrl("https://example.com/view?v=123&lang=zh&access_token=secret&SESSION_ID=private&X-Amz-Signature=signed&x-goog-credential=private#token"))
      .toBe("https://example.com/view?v=123&lang=zh");
    expect(publicUrl("https://example.com/image?token=needed-for-fetch")).toContain("token=needed-for-fetch");
    for (const name of ["token", "refresh_token", "id-token", "auth_token", "Authorization", "auth", "password", "passwd", "pwd", "cookie", "session", "sid", "jwt", "api_key", "key", "signature", "sig", "credential", "credentials", "secret", "client_secret", "code"]) {
      expect(sourceUrl(`https://example.com/?${name}=private&id=1`), name).toBe("https://example.com/?id=1");
    }
  });

  it("shows safe folder breadcrumbs and terminates cycles or missing parents", () => {
    const root = { id: "root", name: "参考", kind: "image", parentId: null };
    const child = { id: "child", name: "建筑", kind: "image", parentId: "root" };
    expect(folderPath(child, [root, child])).toBe("参考 / 建筑");
    expect(folderPath(child, [child])).toBe("建筑");
    expect(folderPath(child, [{ ...root, kind: "video" }, child])).toBe("建筑");
    expect(folderPath(child, [{ ...root, parentId: "child" }, child])).toBe("参考 / 建筑");
  });

  it("derives a supported extension from response MIME rather than the URL suffix", () => {
    expect(extensionFor("IMAGE/PNG; charset=binary", "image")).toBe("png");
    expect(extensionFor("image/jpeg", "image")).toBe("jpg");
    expect(extensionFor("video/mp4", "video")).toBe("mp4");
    expect(extensionFor("video/webm", "video")).toBe("webm");
    expect(extensionFor("text/html", "image")).toBeNull();
    expect(extensionFor("video/mp4", "image")).toBeNull();
    expect(extensionFor("image/svg+xml", "image")).toBeNull();
    expect(IMAGE_LIMIT).toBe(25 * 1024 * 1024);
    expect(VIDEO_LIMIT).toBe(512 * 1024 * 1024);
    expect(CHUNK_SIZE).toBe(128 * 1024);
  });

  it("encodes bytes without truncation across its internal chunk boundary", () => {
    const bytes = Uint8Array.from({ length: 8193 }, (_, index) => index % 256);
    const decoded = atob(base64(bytes));
    expect(decoded.length).toBe(bytes.length);
    expect(Array.from(decoded, value => value.charCodeAt(0))).toEqual(Array.from(bytes));
    expect(base64(new Uint8Array())).toBe("");
  });

  it("does not claim downloads succeeded for queued work or source-only links", () => {
    expect(resultText({ status: "running" })).toContain("尚未确认保存成功");
    expect(resultText({ status: "completed", result: { status: "saved" } })).toBe("文件已保存到栈藏");
    expect(resultText({ status: "completed", result: { status: "duplicate" } })).toContain("内容重复");
    expect(resultText({ status: "completed", result: { status: "link" } })).toBe("仅保存来源链接，未下载视频文件");
    expect(resultText({ status: "failed", message: "图片已超过限制" })).toBe("处理失败：图片已超过限制");
  });
});

describe("browser capture worker safety", () => {
  afterEach(() => { vi.useRealTimers(); vi.unstubAllGlobals(); vi.resetModules(); });

  async function worker(session: Record<string, unknown> = {}, failHello = false) {
    let receive: (value: unknown) => void = () => {};
    let listener: (message: unknown, sender: unknown, respond: (value: unknown) => void) => void = () => {};
    const messages: Record<string, unknown>[] = [];
    const event = { addListener: vi.fn() };
    const port = {
      onMessage: { addListener: (callback: typeof receive) => { receive = callback; } }, onDisconnect: event,
      postMessage(message: Record<string, unknown>) {
        messages.push(message);
        const value = message.action === "hello" ? { started: false, connection: { libraryId: "library", enabled: true } }
          : message.action === "result" ? { status: "completed", result: { status: "saved" } } : {};
        queueMicrotask(() => receive(message.action === "hello" && failHello
          ? { requestId: message.requestId, ok: false, error: "Specified native messaging host not found." }
          : { requestId: message.requestId, ok: true, value }));
      },
    };
    const chrome = {
      runtime: { id: "extension", getURL: (path: string) => `chrome-extension://extension/${path}`, connectNative: vi.fn(() => port),
        onInstalled: event, onMessage: { addListener: (callback: typeof listener) => { listener = callback; } } },
      contextMenus: { onClicked: event }, action: { setBadgeText: vi.fn(), setTitle: vi.fn(), openPopup: vi.fn() },
      storage: { local: { get: async () => ({ confirmedLibrary: "library" }) }, session: {
        get: async () => session, set: async (value: Record<string, unknown>) => { Object.assign(session, value); },
      } },
      tabs: { get: vi.fn(), query: vi.fn(), captureVisibleTab: vi.fn() }, scripting: { executeScript: vi.fn() },
    };
    vi.stubGlobal("chrome", chrome);
    const path = "../../browser-extension/background.js";
    await import(path);
    const send = (message: unknown) => new Promise(resolve => listener(message, { id: "extension", url: chrome.runtime.getURL("popup.html") }, resolve));
    return { messages, chrome, send, session };
  }

  it("does not export resource URLs and clears stale busy state after worker restart", async () => {
    const state = await worker({ progress: { busy: true } });
    await state.send({ action: "hello" });
    expect(state.session.progress).toMatchObject({ busy: false, error: true, message: expect.stringContaining("未确认保存成功") });
    vi.stubGlobal("fetch", vi.fn(async () => new Response(new Uint8Array([1, 2]), { headers: { "content-type": "image/png" } })));
    const result = await state.send({ action: "capture", items: [{ kind: "image", title: "image", pageUrl: "https://example.com/?id=1&token=secret", mediaUrl: "https://example.com/image?signature=private" }] });
    expect(result).toMatchObject({ ok: true });
    const begin = state.messages.find(message => message.action === "begin");
    expect(begin?.capture).toMatchObject({ pageUrl: "https://example.com/?id=1", mediaUrl: "" });
  });

  it("keeps unsaved selections and opens connection setup when the native host is unavailable", async () => {
    const state = await worker({}, true);
    const items = [{ kind: "image", title: "未保存图片", pageUrl: "https://example.com/", mediaUrl: "https://example.com/image.png" }];
    expect(await state.send({ action: "capture", items })).toMatchObject({ ok: false });
    expect(state.session.pendingItems).toEqual(items);
    expect(state.chrome.action.openPopup).toHaveBeenCalledOnce();
    expect(state.messages.some(message => message.action === "begin")).toBe(false);
  });

  it("creates a new native port when a previous send fails instead of keeping a broken connection", async () => {
    const state = await worker();
    let oldDisconnect = () => {};
    state.chrome.runtime.connectNative.mockReturnValueOnce({ onMessage: { addListener: vi.fn() }, onDisconnect: { addListener: vi.fn((callback: () => void) => { oldDisconnect = callback; }) },
      postMessage: () => { throw new Error("Attempting to use a disconnected port object"); },
    });
    expect(await state.send({ action: "hello" })).toMatchObject({ ok: false });
    expect(await state.send({ action: "hello" })).toMatchObject({ ok: true });
    oldDisconnect();
    expect(await state.send({ action: "hello" })).toMatchObject({ ok: true });
    expect(state.chrome.runtime.connectNative).toHaveBeenCalledTimes(2);
  });

  it("times out an optional poster instead of silently committing a source-only success", async () => {
    vi.useFakeTimers();
    const state = await worker();
    vi.stubGlobal("fetch", vi.fn((_url: string, options: RequestInit) => new Promise((_resolve, reject) => {
      options.signal?.addEventListener("abort", () => reject(options.signal?.reason), { once: true });
    })));
    const job = state.send({ action: "capture", items: [{ kind: "link", title: "video", pageUrl: "https://example.com/watch", posterUrl: "https://example.com/poster.png" }] });
    await vi.advanceTimersByTimeAsync(61000);
    expect(await job).toMatchObject({ ok: true, value: [{ status: "failed", message: expect.stringContaining("资源读取超时") }] });
    expect(state.messages.some(message => message.action === "begin")).toBe(false);
    expect(state.session.progress).toMatchObject({ busy: false, error: true });
  });

  it("rejects screenshots when the same tab navigates during capture", async () => {
    const state = await worker();
    state.chrome.tabs.get.mockResolvedValue({ id: 1, windowId: 2, url: "https://example.com/a" });
    state.chrome.scripting.executeScript.mockResolvedValue([{ result: { width: 20, height: 20 } }]);
    state.chrome.tabs.query.mockResolvedValueOnce([{ id: 1, url: "https://example.com/a" }]).mockResolvedValueOnce([{ id: 1, url: "https://example.com/b" }]);
    state.chrome.tabs.captureVisibleTab.mockResolvedValue("data:image/png;base64,AA==");
    expect(await state.send({ action: "screenshot", tabId: 1 })).toMatchObject({ ok: false, error: expect.stringContaining("网页已切换") });
    expect(state.session.pendingItems).toBeUndefined();
  });
});
