import { createRequire } from "node:module";

import type {
  Asset,
  BBox,
  Block,
  BatchResult as BatchResultShape,
  Confidence,
  Document,
  ExtractOptions,
  FigureBlock,
  ImageObject,
  Line,
  Metadata,
  Page,
  SourceAnchor,
  Span,
  TableBlock,
  TableCell,
  TextBlock,
  Warning,
} from "./types.js";

type NativeBinding = {
  version(): string;
  parseTextJson(text: string): string;
  toMarkdown(text: string): string;
  toJson(text: string): string;
  toLatex(text: string): string;
  detectFormat(path: string): string;
  loadPathJson(path: string): string;
  loadPathJsonWithOptions(path: string, optionsJson?: string | null): string;
  loadManyJson(paths: string[]): string;
  documentToMarkdown(documentJson: string): string;
  documentToJson(documentJson: string): string;
  documentToLatex(documentJson: string): string;
};

const require = createRequire(import.meta.url);
const native = loadNativeBinding();

export const version = native.version();

export function parseText(text: string): Document {
  return JSON.parse(native.parseTextJson(text)) as Document;
}

export class DonglerDocument {
  readonly metadata: Metadata;
  readonly pages: Page[];
  readonly #data: Document;

  constructor(data: Document) {
    this.#data = data;
    this.metadata = data.metadata;
    this.pages = data.pages;
  }

  toObject(): Document {
    return JSON.parse(JSON.stringify(this.#data)) as Document;
  }

  toMarkdown(): string {
    return native.documentToMarkdown(JSON.stringify(this.#data));
  }

  toJson(): string {
    return native.documentToJson(JSON.stringify(this.#data));
  }

  toLatex(): string {
    return native.documentToLatex(JSON.stringify(this.#data));
  }
}

export type BatchResult = BatchResultShape<DonglerDocument>;

export function load(path: string, options?: ExtractOptions): DonglerDocument {
  const optionsJson = options ? JSON.stringify(options) : null;
  return new DonglerDocument(
    JSON.parse(native.loadPathJsonWithOptions(path, optionsJson)) as Document
  );
}

export function loadMany(paths: string[]): BatchResult[] {
  const results = JSON.parse(native.loadManyJson(paths)) as BatchResultShape[];

  return results.map((result) => ({
    path: result.path,
    ok: result.ok,
    document: result.document
      ? new DonglerDocument(result.document as Document)
      : null,
    error: result.error,
  }));
}

export function toMarkdown(text: string): string {
  return native.toMarkdown(text);
}

export function toJson(text: string): string {
  return native.toJson(text);
}

export function toLatex(text: string): string {
  return native.toLatex(text);
}

export function detectFormat(path: string): string {
  return native.detectFormat(path);
}

export type {
  Asset,
  BBox,
  Block,
  Confidence,
  Document,
  ExtractOptions,
  FigureBlock,
  ImageObject,
  Line,
  Metadata,
  Page,
  SourceAnchor,
  Span,
  TableBlock,
  TableCell,
  TextBlock,
  Warning,
};

function loadNativeBinding(): NativeBinding {
  const candidates = ["../dongler.node", "../dongler_node.node"];
  const failures: string[] = [];

  for (const candidate of candidates) {
    try {
      return require(candidate) as NativeBinding;
    } catch (error) {
      failures.push(`${candidate}: ${(error as Error).message}`);
    }
  }

  throw new Error(
    `Unable to load Dongler native binding. Tried: ${failures.join("; ")}`
  );
}
