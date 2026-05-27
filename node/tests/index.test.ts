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

  it("loads a PDF path into a rich document object", () => {
    const path = writeFixture("invoice.pdf", minimalPdf("Hello PDF"));

    const document = load(path);
    const block = document.pages[0]?.blocks[0] as TextBlock;

    expect(document.metadata.format).toBe("pdf");
    expect(document.pages[0]?.width).toBe(612);
    expect(block.text).toBe("Hello PDF");
    expect(block.source_anchors?.[0]?.page_number).toBe(1);
  });

  it("loads many paths with per-file results", () => {
    const textPath = writeFixture("batch-notes.txt", "Batch document");
    const pdfPath = writeFixture("batch-invoice.pdf", minimalPdf("Batch PDF"));

    const results = loadMany([textPath, pdfPath]);

    expect(results).toHaveLength(2);
    expect(results[0].path).toBe(textPath);
    expect(results[0].ok).toBe(true);
    expect(results[0].document?.toMarkdown()).toBe("Batch document");
    expect(results[0].error).toBeNull();

    expect(results[1].path).toBe(pdfPath);
    expect(results[1].ok).toBe(true);
    expect(results[1].document?.metadata.format).toBe("pdf");
    expect(results[1].error).toBeNull();
  });
});

function writeFixture(name: string, contents: string | Buffer): string {
  const dir = mkdtempSync(join(tmpdir(), "dongler-test-"));
  const path = join(dir, name);
  writeFileSync(path, contents);
  return path;
}

function minimalPdf(text: string): Buffer {
  const content = `BT /F1 12 Tf 72 720 Td (${text}) Tj ET`;
  return Buffer.from(
    `%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>
endobj
4 0 obj
<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>
endobj
5 0 obj
<< /Length ${content.length} >>
stream
${content}
endstream
endobj
trailer
<< /Root 1 0 R >>
%%EOF
`
  );
}
