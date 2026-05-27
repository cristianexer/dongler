export interface Document {
  schema_version: string;
  metadata: Metadata;
  pages: Page[];
  assets?: Asset[];
  warnings?: Warning[];
}

export interface Page {
  number: number;
  width?: number;
  height?: number;
  rotation?: number;
  bbox?: BBox;
  blocks: Block[];
  images?: ImageObject[];
  assets?: Asset[];
  warnings?: Warning[];
}

export type Block = TextBlock | TableBlock | FigureBlock;

export interface TextBlock {
  type: "text";
  text: string;
  kind: string;
  bbox?: BBox;
  lines?: Line[];
  source_anchors?: SourceAnchor[];
  confidence?: Confidence;
}

export interface TableBlock {
  type: "table";
  headers: string[];
  rows: string[][];
  caption: string | null;
  bbox?: BBox;
  cells?: TableCell[];
  source_anchors?: SourceAnchor[];
  confidence?: Confidence;
}

export interface FigureBlock {
  type: "figure";
  alt_text: string | null;
  caption: string | null;
  bbox?: BBox;
  image_ref?: string | null;
  source_anchors?: SourceAnchor[];
  confidence?: Confidence;
}

export interface Metadata {
  format: string;
  engine: string;
  source: string | null;
  title: string | null;
  character_count: number;
  word_count: number;
  block_count: number;
  file_size_bytes?: number;
  pdf_version?: string | null;
  encrypted?: boolean;
}

export interface BatchResult<TDocument = Document> {
  path: string;
  ok: boolean;
  document: TDocument | null;
  error: string | null;
}

export interface ExtractOptions {
  include_geometry?: boolean;
  include_assets?: boolean;
  max_parallelism?: number | null;
  suppress_headers_footers?: boolean;
  password?: string | null;
}

export interface BBox {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Line {
  text: string;
  bbox?: BBox;
  spans?: Span[];
}

export interface Span {
  text: string;
  bbox?: BBox;
  font?: string | null;
  size?: number | null;
}

export interface TableCell {
  row: number;
  column: number;
  text: string;
  bbox?: BBox;
  is_header: boolean;
}

export interface SourceAnchor {
  page_number: number;
  pdf_object_ids?: string[];
  bbox?: BBox;
  extraction_method: string;
}

export interface Confidence {
  score: number;
  calibrated: boolean;
}

export interface Warning {
  code: string;
  severity: string;
  message: string;
  source_anchor?: SourceAnchor | null;
}

export interface Asset {
  id: string;
  kind: string;
  object_id?: string | null;
  bbox?: BBox;
  width?: number | null;
  height?: number | null;
}

export interface ImageObject {
  id: string;
  object_id?: string | null;
  bbox?: BBox;
  width?: number | null;
  height?: number | null;
}
