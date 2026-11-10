import {fireEvent,render,screen} from "@testing-library/react";
import {expect,it,vi} from "vitest";
import {HoverDismissDetails} from "./HoverDismissDetails";

it("dismisses on outside press while preserving inside interaction",()=>{
  const action=vi.fn();render(<><HoverDismissDetails closeOnLeave={false}><summary>打开菜单</summary><button onClick={action}>菜单操作</button></HoverDismissDetails><button>外部</button></>);
  fireEvent.click(screen.getByText("打开菜单"));
  const details=screen.getByText("打开菜单").closest("details")!;
  expect(details.open).toBe(true);
  fireEvent.pointerDown(screen.getByText("菜单操作"));fireEvent.click(screen.getByText("菜单操作"));
  expect(action).toHaveBeenCalledOnce();expect(details.open).toBe(true);
  fireEvent.pointerDown(screen.getByText("外部"));expect(details.open).toBe(false);
});
it("closes on Escape and restores focus to the trigger",()=>{
  render(<HoverDismissDetails closeOnLeave={false}><summary>打开菜单</summary><input aria-label="菜单输入"/></HoverDismissDetails>);
  const summary=screen.getByText("打开菜单");fireEvent.click(summary);screen.getByLabelText("菜单输入").focus();
  fireEvent.keyDown(screen.getByLabelText("菜单输入"),{key:"Escape"});
  expect(summary.closest("details")!.open).toBe(false);expect(summary).toHaveFocus();
});
