import { createRequire } from "node:module";

import type {
  Block,
  Document,
  Metadata,
  Page,
  TableBlock,
  TextBlock,
} from "./types.js";

type NativeBinding = {
  version(): string;
  parseTextJson(text: string): string;
  toMarkdown(text: string): string;
  toJson(text: string): string;
  toLatex(text: string): string;
  detectFormat(path: string): string;
};

const require = createRequire(import.meta.url);
const native = loadNativeBinding();

export const version = native.version();

export function parseText(text: string): Document {
  return JSON.parse(native.parseTextJson(text)) as Document;
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

export type { Block, Document, Metadata, Page, TableBlock, TextBlock };

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
