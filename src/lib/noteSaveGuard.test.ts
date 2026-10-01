import {describe,expect,it} from "vitest";
import {flushKnowledgeEdits,registerNoteSaveGuard,withKnowledgeBarrier,knowledgeChangesPaused} from "./noteSaveGuard";

describe("knowledge save boundary",()=>{
  it("flushes each editor once even when a render re-registers it while saving",async()=>{
    let calls=0;
    let unregister:()=>void;
    const save=async()=>{
      if(++calls>3)throw new Error("save boundary revisited the same editor");
      // The parent updates its tab snapshot before the async save settles.
      unregister();unregister=registerNoteSaveGuard(save);
      await Promise.resolve();
    };
    unregister=registerNoteSaveGuard(save);
    try{
      await expect(withKnowledgeBarrier(async()=>"switched")).resolves.toBe("switched");
      expect(calls).toBe(1);expect(knowledgeChangesPaused()).toBe(false);
      await flushKnowledgeEdits();expect(calls).toBe(2);
    }finally{unregister();}
  });
});
