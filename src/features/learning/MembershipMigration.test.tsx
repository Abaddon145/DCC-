import {fireEvent,render,screen,waitFor} from "@testing-library/react";
import {beforeEach,expect,it,vi} from "vitest";
import {MembershipMigration} from "./MembershipMigration";
import {learningApi} from "./api";

vi.mock("./api",()=>({learningApi:{listMembershipPreview:vi.fn(),applyMembershipChanges:vi.fn()}}));
beforeEach(()=>vi.clearAllMocks());

it("previews an explicit move and submits only the confirmed choice",async()=>{
  vi.mocked(learningApi.listMembershipPreview).mockResolvedValue({items:[{mediaId:"m1",name:"教程",materials:true,learning:false,hasCourse:false,projectCount:1,assetCount:0,snapshot:"s1"}],total:1,offset:0,limit:50});
  vi.mocked(learningApi.applyMembershipChanges).mockResolvedValue();
  const confirm=vi.spyOn(window,"confirm").mockReturnValue(true);
  render(<MembershipMigration libraryId="lib"/>);
  const choice=await screen.findByLabelText("教程的归属");
  fireEvent.change(choice,{target:{value:"learning"}});
  fireEvent.click(screen.getByRole("button",{name:"确认调整归属"}));
  await waitFor(()=>expect(learningApi.applyMembershipChanges).toHaveBeenCalledWith("lib",[{mediaId:"m1",expectedMaterials:true,expectedLearning:false,expectedSnapshot:"s1",materials:false,learning:true}]));
  expect(confirm).toHaveBeenCalledOnce();confirm.mockRestore();
});

it("does not submit cancelled confirmation and blocks removing an active course",async()=>{
  vi.mocked(learningApi.listMembershipPreview).mockResolvedValue({items:[{mediaId:"m1",name:"教程",materials:true,learning:true,hasCourse:true,projectCount:0,assetCount:0,snapshot:"s1"}],total:1,offset:0,limit:50});
  const confirm=vi.spyOn(window,"confirm").mockReturnValue(false);
  render(<MembershipMigration libraryId="lib"/>);
  const choice=await screen.findByLabelText("教程的归属");
  expect(screen.getByRole("option",{name:"仅素材库"})).toBeDisabled();
  fireEvent.change(choice,{target:{value:"learning"}});
  fireEvent.click(screen.getByRole("button",{name:"确认调整归属"}));
  expect(learningApi.applyMembershipChanges).not.toHaveBeenCalled();confirm.mockRestore();
});

it("requires rereading after a stale preview failure",async()=>{
  vi.mocked(learningApi.listMembershipPreview).mockResolvedValue({items:[{mediaId:"m1",name:"教程",materials:true,learning:false,hasCourse:false,projectCount:0,assetCount:0,snapshot:"s1"}],total:1,offset:0,limit:50});
  vi.mocked(learningApi.applyMembershipChanges).mockRejectedValueOnce("视频已变化");
  const confirm=vi.spyOn(window,"confirm").mockReturnValue(true);
  render(<MembershipMigration libraryId="lib"/>);
  fireEvent.change(await screen.findByLabelText("教程的归属"),{target:{value:"learning"}});
  fireEvent.click(screen.getByRole("button",{name:"确认调整归属"}));
  expect(await screen.findByRole("alert")).toHaveTextContent("请重新读取");
  expect(screen.queryByRole("button",{name:"确认调整归属"})).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button",{name:"重新读取"}));
  expect(await screen.findByLabelText("教程的归属")).toHaveValue("keep");confirm.mockRestore();
});
