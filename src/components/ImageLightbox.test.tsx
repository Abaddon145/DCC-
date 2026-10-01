import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ImageLightbox } from "./ImageLightbox";

vi.mock("./ImagePreview", () => ({
  ImagePreview: ({ alt }: { alt: string }) => <img alt={alt} />,
  preloadImage: vi.fn().mockResolvedValue("data:image/png;base64,test")
}));

const images = [
  { id: "1", originalName: "first.png", sortOrder: 0, isCover: true },
  { id: "2", originalName: "second.png", sortOrder: 1, isCover: false }
];

describe("ImageLightbox", () => {
  it("uses the managed note protocol for note images",()=>{const id="11111111-1111-4111-8111-111111111111";render(<ImageLightbox images={[{id,originalName:"笔记图片",noteAttachmentId:id}]} initialIndex={0} title="笔记" onClose={vi.fn()}/>);expect(screen.getAllByAltText("笔记图片")[0]).toHaveAttribute("src",`http://dcc-media.localhost/note/${id}`);});
  it("switches images with controls and keyboard", () => {
    render(<ImageLightbox images={images} initialIndex={0} title="素材" onClose={vi.fn()} />);
    expect(screen.getByText(/first\.png/)).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("下一张"));
    expect(screen.getByText(/second\.png/)).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "ArrowLeft" });
    expect(screen.getByText(/first\.png/)).toBeInTheDocument();
  });

  it("clamps and resets zoom", () => {
    render(<ImageLightbox images={images} initialIndex={0} title="素材" onClose={vi.fn()} />);
    fireEvent.click(screen.getByLabelText("放大"));
    expect(screen.getByText("125%")).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("适应窗口"));
    expect(screen.getByText("100%")).toBeInTheDocument();
  });
});
