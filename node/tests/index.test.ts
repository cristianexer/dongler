import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { detectFormat, load, loadMany, parseText, toMarkdown } from "../src/index";
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

  it("loads a text path into a document object with render methods", () => {
    const path = writeFixture("notes.txt", "Hello from a file\n\nSecond paragraph");

    const document = load(path);

    expect(document.metadata.format).toBe("text");
    expect(document.metadata.source).toBe(path);
    expect(document.toMarkdown()).toBe("Hello from a file\n\nSecond paragraph");
    expect(document.toLatex()).toContain("Hello from a file");
    expect(JSON.parse(document.toJson()).metadata.block_count).toBe(2);
  });

  it("throws a planned-format error for PDF paths", () => {
    const path = writeFixture("invoice.pdf", "%PDF planned fixture");

    expect(() => load(path)).toThrow(/pdf extraction/);
  });

  it("loads many paths with per-file results", () => {
    const textPath = writeFixture("batch-notes.txt", "Batch document");
    const pdfPath = writeFixture("batch-invoice.pdf", "%PDF planned fixture");

    const results = loadMany([textPath, pdfPath]);

    expect(results).toHaveLength(2);
    expect(results[0].path).toBe(textPath);
    expect(results[0].ok).toBe(true);
    expect(results[0].document?.toMarkdown()).toBe("Batch document");
    expect(results[0].error).toBeNull();

    expect(results[1].path).toBe(pdfPath);
    expect(results[1].ok).toBe(false);
    expect(results[1].document).toBeNull();
    expect(results[1].error).toMatch(/pdf extraction/);
  });
});

function writeFixture(name: string, contents: string): string {
  const dir = mkdtempSync(join(tmpdir(), "dongler-test-"));
  const path = join(dir, name);
  writeFileSync(path, contents);
  return path;
}
