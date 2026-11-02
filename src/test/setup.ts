import "@testing-library/jest-dom/vitest";
import { cleanup, configure } from "@testing-library/react";
import { afterEach } from "vitest";

// Default findBy/waitFor is 1000ms. On a loaded windows-latest runner the same
// queries pass once the module is warm and fail on the cold cases (play button,
// outline node, debounced asset search).
configure({ asyncUtilTimeout: 8_000 });

afterEach(cleanup);
