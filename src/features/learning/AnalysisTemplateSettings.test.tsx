import {fireEvent,render,screen,waitFor} from "@testing-library/react";
import {expect,it,vi} from "vitest";
import {AnalysisTemplateSettings} from "./AnalysisTemplateSettings";
import {learningApi} from "./api";
vi.mock("./api",()=>({learningApi:{saveCourseAnalysisTemplate:vi.fn(),deleteCourseAnalysisTemplate:vi.fn()}}));
it("saves a custom analysis template in settings",async()=>{
  vi.mocked(learningApi.saveCourseAnalysisTemplate).mockResolvedValue({id:"new",name:"材质",focus:"节点",version:1});
  const onChange=vi.fn();render(<AnalysisTemplateSettings libraryId="lib" templates={[]} onChange={onChange}/>);
  fireEvent.change(screen.getByLabelText("模板名称"),{target:{value:"材质"}});
  fireEvent.change(screen.getByLabelText("分析要求"),{target:{value:"节点"}});
  fireEvent.click(screen.getByRole("button",{name:"保存模板"}));
  await waitFor(()=>expect(learningApi.saveCourseAnalysisTemplate).toHaveBeenCalledWith("lib",null,null,"材质","节点"));
  expect(onChange).toHaveBeenCalled();
});
