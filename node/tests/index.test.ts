import { describe, expect, it } from "vitest";

import { detectFormat, parseText, toMarkdown } from "../src/index";
import type { TextBlock } from "../src/types";

describe("dongler TypeScript bindings", () => {
  it("parses text into a typed document", () => {
    const document = parseText("Hello from Dongler");
    const block = document.pages[0]?.blocks[0] as TextBlock;

    expect(document.metadata.format).toBe("text");
    expect(block.type).toBe("text");
    expect(block.text).toBe("Hello from Dongler");
  });

  it("renders markdown using the native binding", () => {
    expect(toMarkdown("Hello from Dongler")).toBe("Hello from Dongler");
  });

  it("detects txt files", () => {
    expect(detectFormat("notes.txt")).toBe("text");
  });
});
