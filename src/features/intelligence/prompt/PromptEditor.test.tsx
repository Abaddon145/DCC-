import {render,screen,fireEvent,cleanup,waitFor} from "@testing-library/react";
import {it,expect,vi,beforeEach,afterEach} from "vitest";
import {PromptEditor} from "./PromptEditor";
import {intelligenceApi} from "../api";
vi.mock("../api",()=>({intelligenceApi:{getPrompt:vi.fn(),upsertPrompt:vi.fn()}}));
const saved={asset:{id:"p",name:"城市",description:"说明",assetType:"prompt" as const,origin:{kind:"entity" as const,id:"e"},tags:[],thumbnail:null,deleted:false,createdAt:"now",updatedAt:"now"},content:"old content",model:"model",parameters:{seed:1},revision:3,createdAt:"now",updatedAt:"now"};
beforeEach(()=>{vi.clearAllMocks();vi.mocked(intelligenceApi.getPrompt).mockResolvedValue(saved);vi.mocked(intelligenceApi.upsertPrompt).mockResolvedValue(saved);});afterEach(cleanup);
it("创建 Prompt 校验参数对象后保存，不调用生成服务",async()=>{
 const done=vi.fn();render(<PromptEditor libraryId="lib" onSaved={done}/>);fireEvent.change(screen.getByLabelText("Prompt 名称"),{target:{value:"城市"}});fireEvent.change(screen.getByLabelText("Prompt 内容"),{target:{value:"new content"}});fireEvent.change(screen.getByLabelText("参数 JSON"),{target:{value:"[]"}});fireEvent.click(screen.getByRole("button",{name:"保存 Prompt"}));expect(await screen.findByRole("alert")).toBeTruthy();expect(done).not.toHaveBeenCalled();
 fireEvent.change(screen.getByLabelText("参数 JSON"),{target:{value:'{"seed":1}'}});fireEvent.click(screen.getByRole("button",{name:"保存 Prompt"}));await waitFor(()=>expect(done).toHaveBeenCalledWith("p"));expect(intelligenceApi.upsertPrompt).toHaveBeenCalledWith("lib",expect.objectContaining({assetId:null,expectedRevision:null,content:"new content",parameters:{seed:1}}));
});
it("冲突保留草稿，重新读取前确认，复制只复制内容",async()=>{
 vi.mocked(intelligenceApi.upsertPrompt).mockRejectedValue(new Error("修订冲突"));const dirty=vi.fn();Object.defineProperty(navigator,"clipboard",{value:{writeText:vi.fn().mockResolvedValue(undefined)},configurable:true});render(<PromptEditor libraryId="lib" assetId="p" onSaved={vi.fn()} onDirty={dirty}/>);await screen.findByDisplayValue("old content");fireEvent.change(screen.getByLabelText("Prompt 内容"),{target:{value:"my draft"}});fireEvent.click(screen.getByRole("button",{name:"保存 Prompt"}));await screen.findByText(/修订冲突/);expect(screen.getByDisplayValue("my draft")).toBeTruthy();expect(dirty).toHaveBeenLastCalledWith(true);
 fireEvent.click(screen.getByRole("button",{name:"复制内容"}));await waitFor(()=>expect(navigator.clipboard.writeText).toHaveBeenCalledWith("my draft"));
 fireEvent.click(screen.getByRole("button",{name:"重新读取 Prompt"}));expect(screen.getByRole("alertdialog",{name:"放弃当前草稿"})).toBeTruthy();expect(intelligenceApi.getPrompt).toHaveBeenCalledTimes(1);fireEvent.click(screen.getByRole("button",{name:"放弃草稿并读取"}));await screen.findByDisplayValue("old content");expect(dirty).toHaveBeenLastCalledWith(false);
});
