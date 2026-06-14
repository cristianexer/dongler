//! Model registry (PRD §5.2). A typed, in-code catalogue of every model the
//! pipeline may use, recording the **weight license** and ONNX-artifact
//! provenance alongside the download URL + sha256. The license floor is
//! enforced in code: only `Permissive` weights are shippable as defaults;
//! restricted models are reference ceilings or opt-in plugins.

/// What a model does in the pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTask {
    Layout,
    Ocr,
    TableStructure,
    Formula,
    Vlm,
}

/// License class governing redistribution of the *weights*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LicenseClass {
    /// MIT / Apache-2.0 / CDLA-Permissive — shippable as a default.
    Permissive,
    /// AGPL / OpenRAIL revenue-capped / CC-BY-(NC|SA) — opt-in / reference only.
    Restricted,
    /// Not yet verified — must be confirmed in the E-phase before shipping.
    Unverified,
}

/// A registry entry.
#[derive(Debug, Clone, Copy)]
pub struct ModelEntry {
    pub name: &'static str,
    pub task: ModelTask,
    pub version: &'static str,
    /// Whether this is the pipeline's default for its task.
    pub default: bool,
    /// SHA-256 of the ONNX artifact (empty until pinned in the E-phase).
    pub sha256: &'static str,
    /// Download source (HF repo or release URL).
    pub source: &'static str,
    pub code_license: &'static str,
    pub weight_license: LicenseClass,
    /// Free-text note on ONNX availability / provenance.
    pub onnx_status: &'static str,
}

impl ModelEntry {
    /// A model is shippable as a bundled default only if its weights are
    /// verified permissive.
    pub fn is_shippable(&self) -> bool {
        matches!(self.weight_license, LicenseClass::Permissive)
    }
}

/// The full catalogue. SHA-256 values are pinned during the E-phase bake-offs;
/// they are intentionally empty here so a missing pin is obvious.
pub const REGISTRY: &[ModelEntry] = &[
    ModelEntry {
        name: "docling-layout-heron",
        task: ModelTask::Layout,
        version: "v2",
        default: true,
        sha256: "",
        source: "hf:docling-project/docling-layout-heron-onnx",
        code_license: "MIT",
        weight_license: LicenseClass::Permissive,
        onnx_status: "official ONNX export; RT-DETRv2, NMS-free",
    },
    ModelEntry {
        name: "PP-DocLayout_plus-L",
        task: ModelTask::Layout,
        version: "3.x",
        default: false,
        sha256: "",
        source: "hf:PaddlePaddle/PP-DocLayout_plus-L",
        code_license: "Apache-2.0",
        weight_license: LicenseClass::Permissive,
        onnx_status: "via paddle2onnx; bake-off contender (E2)",
    },
    ModelEntry {
        name: "DocLayout-YOLO",
        task: ModelTask::Layout,
        version: "DocStructBench",
        default: false,
        sha256: "",
        source: "hf:opendatalab/DocLayout-YOLO",
        code_license: "AGPL-3.0",
        weight_license: LicenseClass::Restricted,
        onnx_status: "reference ceiling only; contested AGPL (issue #110)",
    },
    ModelEntry {
        name: "PP-OCRv5",
        task: ModelTask::Ocr,
        version: "5",
        default: true,
        sha256: "",
        source: "hf:PaddlePaddle/PP-OCRv5_server_det+rec",
        code_license: "Apache-2.0",
        weight_license: LicenseClass::Permissive,
        onnx_status: "ONNX via RapidOCR/oar-ocr; PP-OCRv6 when ONNX lands",
    },
    ModelEntry {
        name: "slanet-1m-onnx",
        task: ModelTask::TableStructure,
        version: "1m",
        default: true,
        // sha256 of slanet_1m.onnx, pinned from the model card + verified on download.
        sha256: "8a8aa31bf964c1c05039f02814e5f425a354a37552eaca8e6d5dc513048759f5",
        source: "hf:bdatdo0601/slanet-1m-onnx",
        code_license: "MIT",
        weight_license: LicenseClass::Permissive,
        onnx_status: "official ONNX export (paddle2onnx, opset 16) of dimtri009/SLANet-1M; I/O verified from inference.yml",
    },
    ModelEntry {
        name: "SLANet-plus",
        task: ModelTask::TableStructure,
        version: "plus",
        default: false,
        sha256: "",
        source: "hf:RapidAI/RapidTable",
        code_license: "Apache-2.0",
        weight_license: LicenseClass::Permissive,
        onnx_status: "contender; RapidTable ONNX provenance unresolved (repo 401)",
    },
    ModelEntry {
        name: "table-transformer (TATR)",
        task: ModelTask::TableStructure,
        version: "v1.1-all",
        default: false,
        sha256: "",
        source: "hf:microsoft/table-transformer-structure-recognition-v1.1-all",
        code_license: "MIT",
        weight_license: LicenseClass::Permissive,
        onnx_status: "DETR-family; ONNX export friction; contender (E4)",
    },
    ModelEntry {
        name: "PP-FormulaNet_plus-L",
        task: ModelTask::Formula,
        version: "plus-L",
        default: true,
        sha256: "",
        source: "hf:PaddlePaddle/PP-FormulaNet_plus-L",
        code_license: "Apache-2.0",
        weight_license: LicenseClass::Permissive,
        onnx_status: "M5 stretch; via paddle2onnx [verify]",
    },
    ModelEntry {
        name: "granite-docling-258M",
        task: ModelTask::Vlm,
        version: "258M",
        default: true,
        sha256: "",
        source: "hf:ibm-granite/granite-docling-258M",
        code_license: "Apache-2.0",
        weight_license: LicenseClass::Permissive,
        onnx_status: "opt-in VLM escalation; GGUF/ONNX; off by default",
    },
];

/// Look up an entry by name.
pub fn get(name: &str) -> Option<&'static ModelEntry> {
    REGISTRY.iter().find(|e| e.name == name)
}

/// The default model for a task, if one is marked.
pub fn default_for(task: ModelTask) -> Option<&'static ModelEntry> {
    REGISTRY.iter().find(|e| e.task == task && e.default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_task_has_exactly_one_default() {
        for task in [
            ModelTask::Layout,
            ModelTask::Ocr,
            ModelTask::TableStructure,
            ModelTask::Formula,
            ModelTask::Vlm,
        ] {
            let defaults = REGISTRY.iter().filter(|e| e.task == task && e.default).count();
            assert_eq!(defaults, 1, "task {task:?} should have one default");
        }
    }

    #[test]
    fn all_defaults_are_shippable_permissive() {
        for e in REGISTRY.iter().filter(|e| e.default) {
            assert!(
                e.is_shippable(),
                "default model {} must have permissive weights",
                e.name
            );
        }
    }

    #[test]
    fn restricted_models_are_not_defaults() {
        for e in REGISTRY.iter().filter(|e| !e.is_shippable()) {
            assert!(
                !e.default,
                "restricted model {} must not be a shipped default",
                e.name
            );
        }
    }

    #[test]
    fn lookup_works() {
        assert_eq!(default_for(ModelTask::Layout).unwrap().name, "docling-layout-heron");
        assert!(get("DocLayout-YOLO").is_some());
        assert!(get("nonexistent").is_none());
    }
}
