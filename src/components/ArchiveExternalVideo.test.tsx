import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../lib/api";
import type { MediaEntryDetail } from "../types";
import { MediaDetail } from "./MediaLibraryView";

vi.mock("../lib/api", () => ({
  mediaLibraryFileUrl: (id: string) => `http://dcc-media.localhost/library/file/${id}`,
  mediaLibraryBundleUrl: (id: string, path: string) => `http://dcc-media.localhost/library/bundle/${id}/${path}`,
  api: {
    listProjects: vi.fn(),
    listReferenceBoards: vi.fn(),
    getLibraryLocations: vi.fn(),
    archiveExternalVideo: vi.fn(),
  },
}));
vi.mock("./ModelViewer", () => ({ ModelViewer: () => null }));

const libraryId = "11111111-1111-4111-8111-111111111111";
const detail: MediaEntryDetail = {
  id: "22222222-2222-4222-8222-222222222222",
  kind: "video",
  folderId: null,
  name: "外部课程",
  description: "",
  author: "",
  sourceUrl: "",
  license: "",
  favorite: false,
  processingStatus: "ready",
  processingMessage: "",
  format: "mp4",
  fileSize: 128,
  tags: [],
  thumbnailFileId: null,
  primaryFileId: "file",
  createdAt: "2026-10-29",
  updatedAt: "2026-10-29",
  assetIds: [],
  projectIds: [],
  files: [{
    id: "file",
    entryId: "22222222-2222-4222-8222-222222222222",
    role: "main",
    logicalPath: "course.mp4",
    originalName: "course.mp4",
    mimeType: "video/mp4",
    fileSize: 128,
    checksum: "abc",
    width: 64,
    height: 64,
    durationMs: 1000,
    storageMode: "external",
    sourceRevision: 1,
  }],
};

describe("external video archive action", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.getLibraryLocations).mockResolvedValue({
      current: { path: "/tmp/library", name: "测试库", libraryId, available: true, isCurrent: true, retainedCopy: false, sizeBytes: 1, lastOpenedAt: null },
      recent: [],
      startupWarning: null,
    });
    vi.mocked(api.archiveExternalVideo).mockResolvedValue("task");
  });

  it("queues an archive copy and tells the user the source stays", async () => {
    const notify = vi.fn();
    vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<MediaDetail detail={detail} canAddProject={false} canAddBoard={false} onClose={vi.fn()} onChanged={async () => {}} onNotify={notify} />);
    fireEvent.click(screen.getByRole("button", { name: "归档复制到本库" }));
    await waitFor(() => expect(api.archiveExternalVideo).toHaveBeenCalledTimes(1));
    const [calledLibrary, calledEntry, operation] = vi.mocked(api.archiveExternalVideo).mock.calls[0];
    expect(calledLibrary).toBe(libraryId);
    expect(calledEntry).toBe(detail.id);
    expect(operation).toMatch(/^[0-9a-f-]{36}$/);
    expect(notify).toHaveBeenCalledWith(expect.stringContaining("源文件仍保留"));
  });

  it("does not offer archive for a managed video", () => {
    const managed: MediaEntryDetail = {
      ...detail,
      files: [{ ...detail.files[0], storageMode: "managed", sourceRevision: null }],
    };
    render(<MediaDetail detail={managed} canAddProject={false} canAddBoard={false} onClose={vi.fn()} onChanged={async () => {}} onNotify={vi.fn()} />);
    expect(screen.queryByRole("button", { name: "归档复制到本库" })).not.toBeInTheDocument();
  });
});
