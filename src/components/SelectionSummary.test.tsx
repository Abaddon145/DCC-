import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { api } from "../lib/api";
import { SelectionSummary } from "./SelectionSummary";
vi.mock("../lib/api",()=>({api:{getSelectionLabels:vi.fn()}}));
describe("explicit selection scope",()=>{
 it("shows hidden selections and includes them in the full list",async()=>{
   vi.mocked(api.getSelectionLabels).mockResolvedValue([{id:"a",name:"沙漠"},{id:"b",name:"石桥"}]);
   render(<SelectionSummary ids={new Set(["a","b"])} filterKey="desert" getMatchingIds={async()=>["a"]}/>);
   fireEvent.click(await screen.findByRole("button",{name:"当前条件 1 项 · 隐藏 1 项 · 查看清单"}));
   expect(await screen.findByText("石桥")).toBeInTheDocument();expect(api.getSelectionLabels).toHaveBeenCalledWith(["a","b"],false);
 });
 it("pages a large selection without loading every label",async()=>{
   vi.mocked(api.getSelectionLabels).mockResolvedValue([]);const ids=new Set(Array.from({length:201},(_,index)=>String(index)));
   render(<SelectionSummary ids={ids} filterKey="all" getMatchingIds={async()=>[...ids]} media/>);
   fireEvent.click(await screen.findByRole("button",{name:/当前条件 201/}));await waitFor(()=>expect(api.getSelectionLabels).toHaveBeenCalledWith([...ids].slice(0,100),true));
   fireEvent.click(screen.getByRole("button",{name:"下一页"}));await waitFor(()=>expect(api.getSelectionLabels).toHaveBeenLastCalledWith([...ids].slice(100,200),true));
 });
});
