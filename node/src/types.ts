export interface Document {
  metadata: Metadata;
  pages: Page[];
}

export interface Page {
  number: number;
  blocks: Block[];
}

export type Block = TextBlock | TableBlock;

export interface TextBlock {
  type: "text";
  text: string;
  kind: string;
}

export interface TableBlock {
  type: "table";
  headers: string[];
  rows: string[][];
  caption: string | null;
}

export interface Metadata {
  format: string;
  engine: string;
  source: string | null;
  title: string | null;
  character_count: number;
  word_count: number;
  block_count: number;
}
