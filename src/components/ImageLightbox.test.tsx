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
