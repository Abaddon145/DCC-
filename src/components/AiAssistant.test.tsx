import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AiAssistant } from "./AiAssistant";
import { aiApi, aiDefaults, aiSelectionKeys, type AiSession } from "../lib/ai";
import { api } from "../lib/api";

vi.mock("../lib/ai", async original => ({ ...await original<typeof import("../lib/ai")>(), aiApi: { prepare: vi.fn(), get: vi.fn(), list: vi.fn(), rename: vi.fn(), remove: vi.fn(), generate: vi.fn(), cancel: vi.fn(), apply: vi.fn(), undo: vi.fn(), candidates: vi.fn(), search: vi.fn() } }));
vi.mock("../lib/api", () => ({ api: { listProjects: vi.fn(), getTranslationSettings: vi.fn() } }));
const base: AiSession = {
  id: "session", name: "整理岩石", kind: "organize", status: "draft", message: "准备完成", updatedAt: "now",
  request: { ...aiDefaults("zh-CN", { kind: "organize", assetIds: ["a"] }), prompt: "整理岩石" },
  context: { entries: [{ id: "a", kind: "asset", name: "Rock", description: "", tags: [], category: null, software: [], versions: [], formats: ["uasset"], license: "" }], project: null, categories: [], folders: [], existingTags: [] },
  output: null, estimatedTokens: 10000,
};
const ready: AiSession = { ...base, status: "ready", output: { title: "整理岩石", summary: "整理建议", directions: [], needs: [], tasks: [], changes: [{ id: "a", kind: "asset", field: "description", value: "岩石模型参考", reason: "基于已选元数据" }], recommendations: [], searches: [] } };

describe("AI assistant consent and drafts", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(aiApi.list).mockResolvedValue({ items: [], total: 0 });
    vi.mocked(api.listProjects).mockResolvedValue([]);
    vi.mocked(api.getTranslationSettings).mockResolvedValue({ provider: "deepseek", configured: true, deepseekConfigured: true, fabAutoTranslate: false, contentLanguage: "zh-CN" });
    vi.mocked(aiApi.prepare).mockResolvedValue(base);
    vi.mocked(aiApi.generate).mockResolvedValue(ready);
    vi.mocked(aiApi.cancel).mockResolvedValue();
    vi.spyOn(window, "confirm").mockReturnValue(true);
  });
  it("does not send while preparing; generation and application require separate decisions", async () => {
    const changed = vi.fn();
    render(<AiAssistant language="zh-CN" launch={{ kind: "organize", assetIds: ["a"] }} notify={vi.fn()} onChanged={changed} onSettings={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("创作要求"), { target: { value: "整理岩石" } });
    fireEvent.click(screen.getByRole("button", { name: "检查发送范围" }));
    await screen.findByRole("button", { name: "确认发送并生成" });
    expect(aiApi.generate).not.toHaveBeenCalled();
    expect(aiApi.apply).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "确认发送并生成" }));
    await screen.findByText("岩石模型参考");
    expect(screen.getByRole("button", { name: "确认应用所选" })).toBeDisabled();
    fireEvent.click(screen.getByRole("checkbox"));
    vi.mocked(aiApi.apply).mockResolvedValue("已应用");
    vi.mocked(aiApi.get).mockResolvedValue({ ...ready, status: "applied" });
    fireEvent.click(screen.getByRole("button", { name: "确认应用所选" }));
    await waitFor(() => expect(aiApi.apply).toHaveBeenCalledWith("session", ["change:0"]));
    await waitFor(() => expect(changed).toHaveBeenCalled());
  });
  it("unconfigured credentials allow offline drafting but prevent sending", async () => {
    vi.mocked(api.getTranslationSettings).mockResolvedValue({ provider: "baidu", configured: true, deepseekConfigured: false, fabAutoTranslate: false, contentLanguage: "zh-CN" });
    render(<AiAssistant language="zh-CN" notify={vi.fn()} onChanged={vi.fn()} onSettings={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("创作要求"), { target: { value: "制作场景" } });
    fireEvent.click(screen.getByRole("button", { name: "检查发送范围" }));
    expect(await screen.findByRole("button", { name: "确认发送并生成" })).toBeDisabled();
    expect(aiApi.generate).not.toHaveBeenCalled();
  });
  it("history remains readable without automatically retrying a failed request", async () => {
    vi.mocked(aiApi.list).mockResolvedValue({ items: [{ id: "session", name: "整理岩石", kind: "organize", status: "failed", updatedAt: "now" }], total: 1 });
    vi.mocked(aiApi.get).mockResolvedValue({ ...base, status: "failed", message: "额度已达到上限" });
    render(<AiAssistant language="zh-CN" notify={vi.fn()} onChanged={vi.fn()} onSettings={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /整理岩石/ }));
    await screen.findByText("额度已达到上限");
    expect(screen.getByText(/旧版本未记录完整请求消息/)).toBeInTheDocument();
    expect(aiApi.generate).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "确认发送并生成" })).not.toBeInTheDocument();
  });
  it("cancel is available while the asynchronous generation is pending", async () => {
    vi.mocked(aiApi.generate).mockImplementation(() => new Promise(() => {}));
    render(<AiAssistant language="zh-CN" notify={vi.fn()} onChanged={vi.fn()} onSettings={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("创作要求"), { target: { value: "制作场景" } });
    fireEvent.click(screen.getByRole("button", { name: "检查发送范围" }));
    fireEvent.click(await screen.findByRole("button", { name: "确认发送并生成" }));
    fireEvent.click(await screen.findByRole("button", { name: "取消生成" }));
    expect(aiApi.cancel).toHaveBeenCalledWith("session");
    expect(aiApi.apply).not.toHaveBeenCalled();
  });
  it("re-prepares rejected output offline and shows actual usage without automatically sending",async()=>{
    vi.mocked(aiApi.list).mockResolvedValue({items:[{...base,status:"failed"}],total:1});vi.mocked(aiApi.get).mockResolvedValue({...base,status:"failed",message:"不允许 changes，未应用",actualTokens:123});
    render(<AiAssistant language="zh-CN" notify={vi.fn()} onChanged={vi.fn()} onSettings={vi.fn()}/>);fireEvent.click(await screen.findByRole("button",{name:/整理岩石/}));await screen.findByText("123 Token");fireEvent.click(screen.getByRole("button",{name:"重新准备请求"}));await screen.findByRole("button",{name:"确认发送并生成"});expect(aiApi.prepare).toHaveBeenCalledWith(base.request);expect(aiApi.generate).not.toHaveBeenCalled();expect(aiApi.apply).not.toHaveBeenCalled();
  });
  it("search only runs after confirming its concrete local query", async () => {
    const searchSession: AiSession = { ...ready, request: { ...base.request, kind: "search" }, kind: "search", output: { ...ready.output!, changes: [], searches: [{ kind: "asset", query: "tag:Rock" }] } };
    vi.mocked(aiApi.list).mockResolvedValue({ items: [{ ...searchSession }], total: 1 });
    vi.mocked(aiApi.get).mockResolvedValue(searchSession);
    vi.mocked(aiApi.search).mockResolvedValue({ items: [], total: 0, offset: 0 });
    render(<AiAssistant language="zh-CN" notify={vi.fn()} onChanged={vi.fn()} onSettings={vi.fn()} />);
    fireEvent.click(await screen.findByRole("button", { name: /整理岩石/ }));
    await screen.findByText("tag:Rock");
    expect(aiApi.search).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "运行本地搜索" }));
    await waitFor(() => expect(aiApi.search).toHaveBeenCalledWith("session", 0));
    expect(aiApi.apply).not.toHaveBeenCalled();
  });
  it("contextual selection is copied and selectable keys only name known output rows", () => {
    expect(aiDefaults("en", { kind: "recommend", projectId: "p", mediaIds: ["m"] })).toMatchObject({ projectId: "p", mediaIds: ["m"], locale: "en", mode: "fast" });
    expect(aiSelectionKeys(ready.output!)).toEqual(["change:0"]);
  });
});
