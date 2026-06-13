use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: &str = "dongler.ir.v2";

/// How a page was routed by the pipeline triage stage (IR v2). `None` on
/// documents produced by the legacy fast path, which keeps v1 deserializable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    /// Text-layer characters cover the page; no OCR needed.
    BornDigital,
    /// No usable text layer; the page is an image and must be OCR'd.
    Scanned,
    /// Partial text layer (e.g. scan with embedded OCR); decided per region.
    Hybrid,
}

/// Where a block's text came from. Recorded in [`Provenance`] so consumers can
/// audit (and filter) text by trustworthiness — the deterministic invariant is
/// that `Vlm` text only appears after passing the escalation validators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextSource {
    /// Pulled verbatim from the PDF text layer (deterministic, cannot hallucinate).
    TextLayer,
    /// Produced by an OCR model on a rasterized region.
    Ocr,
    /// Produced by a vision-language model (validator-gated).
    Vlm,
    /// Derived by a heuristic in the legacy engine (no model).
    Heuristic,
}

/// Per-block provenance attached by the pipeline (IR v2). Optional so legacy
/// documents remain valid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub text_source: TextSource,
    /// Model identifier, e.g. `"docling-layout-heron@v2"`. `None` for text-layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
}

/// The closed vocabulary of text-block kinds (IR v2). This is a *helper* over the
/// serialized `TextBlock::kind` string rather than a hard field-type change, so
/// v1 documents — including the ones that emit the buggy `"heading"` kind — still
/// deserialize. New pipeline code should construct kinds via [`BlockKind::as_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Heading(u8),
    Paragraph,
    ListItem,
    Code,
    Formula,
    Caption,
    PageHeader,
    PageFooter,
    Footnote,
}

impl BlockKind {
    /// Canonical serialized form used in `TextBlock::kind`.
    pub fn as_str(&self) -> String {
        match self {
            BlockKind::Heading(level) => format!("heading_{}", (*level).clamp(1, 6)),
            BlockKind::Paragraph => "paragraph".to_owned(),
            BlockKind::ListItem => "list_item".to_owned(),
            BlockKind::Code => "code".to_owned(),
            BlockKind::Formula => "formula".to_owned(),
            BlockKind::Caption => "caption".to_owned(),
            BlockKind::PageHeader => "page_header".to_owned(),
            BlockKind::PageFooter => "page_footer".to_owned(),
            BlockKind::Footnote => "footnote".to_owned(),
        }
    }

    /// Tolerant parse of a serialized kind string. Unknown or legacy values
    /// (including v1's bare `"heading"` and `"list"`) map to their closest v2
    /// equivalent, never failing — this is what keeps v1 deserialization total.
    pub fn parse(kind: &str) -> BlockKind {
        if let Some(rest) = kind.strip_prefix("heading_") {
            if let Ok(level) = rest.parse::<u8>() {
                return BlockKind::Heading(level.clamp(1, 6));
            }
        }
        match kind {
            "heading" | "title" => BlockKind::Heading(1),
            "list" | "list_item" => BlockKind::ListItem,
            "code" => BlockKind::Code,
            "formula" | "equation" => BlockKind::Formula,
            "caption" => BlockKind::Caption,
            "page_header" | "header" => BlockKind::PageHeader,
            "page_footer" | "footer" => BlockKind::PageFooter,
            "footnote" => BlockKind::Footnote,
            _ => BlockKind::Paragraph,
        }
    }

    /// Heading level if this kind is a heading.
    pub fn heading_level(&self) -> Option<u8> {
        match self {
            BlockKind::Heading(level) => Some(*level),
            _ => None,
        }
    }

    /// Whether the renderer should drop this kind from default Markdown output
    /// (page furniture, per the olmOCR convention the PRD adopts).
    pub fn is_page_furniture(&self) -> bool {
        matches!(self, BlockKind::PageHeader | BlockKind::PageFooter)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    #[serde(default = "default_schema_version")]
    pub schema_version: String,
    pub metadata: Metadata,
    pub pages: Vec<Page>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<Asset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Page {
    pub number: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<i32>,
    /// Pipeline triage classification (IR v2). `None` on legacy fast-path output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<Route>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BBox>,
    pub blocks: Vec<Block>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageObject>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<Asset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    Text(TextBlock),
    Table(TableBlock),
    Figure(FigureBlock),
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TextBlock {
    pub text: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BBox>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<Line>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_anchors: Vec<SourceAnchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
    /// Pipeline provenance (IR v2). `None` on legacy fast-path output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TableBlock {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub caption: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BBox>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<TableCell>,
    /// Pre-rendered HTML table preserving row/col spans (IR v2). When present the
    /// Markdown renderer embeds it verbatim (the PRD's default table form).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_anchors: Vec<SourceAnchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
    /// Pipeline provenance (IR v2). `None` on legacy fast-path output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct FigureBlock {
    pub alt_text: Option<String>,
    pub caption: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BBox>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_anchors: Vec<SourceAnchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<Confidence>,
    /// Pipeline provenance (IR v2). `None` on legacy fast-path output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    pub format: String,
    pub engine: String,
    pub source: Option<String>,
    pub title: Option<String>,
    pub character_count: usize,
    pub word_count: usize,
    pub block_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_size_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pdf_version: Option<String>,
    #[serde(default)]
    pub encrypted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchResult {
    pub path: String,
    pub ok: bool,
    pub document: Option<Document>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractOptions {
    #[serde(default = "default_include_geometry")]
    pub include_geometry: bool,
    #[serde(default = "default_include_assets")]
    pub include_assets: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_parallelism: Option<usize>,
    #[serde(default)]
    pub suppress_headers_footers: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            include_geometry: true,
            include_assets: true,
            max_parallelism: None,
            suppress_headers_footers: false,
            password: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BBox>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BBox>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f32>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub bold: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub italic: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableCell {
    pub row: usize,
    pub column: usize,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BBox>,
    #[serde(default)]
    pub is_header: bool,
    /// Number of grid columns this cell spans (1 for an ordinary cell). A value
    /// greater than 1 marks a horizontally merged cell; the spanned-over column
    /// positions are omitted from `cells`.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub col_span: usize,
    /// Number of grid rows this cell spans (1 for an ordinary cell).
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub row_span: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceAnchor {
    pub page_number: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pdf_object_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BBox>,
    pub extraction_method: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Confidence {
    pub score: f32,
    #[serde(default)]
    pub calibrated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Warning {
    pub code: String,
    pub severity: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_anchor: Option<SourceAnchor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BBox>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageObject {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox: Option<BBox>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
}

pub fn default_schema_version() -> String {
    SCHEMA_VERSION.to_owned()
}

fn default_include_geometry() -> bool {
    true
}

fn default_include_assets() -> bool {
    true
}

fn one() -> usize {
    1
}

fn is_one(value: &usize) -> bool {
    *value == 1
}

fn is_false(value: &bool) -> bool {
    !*value
}
