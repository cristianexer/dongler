/** @type {import('@docusaurus/plugin-content-docs').SidebarsConfig} */
const sidebars = {
  docs: [
    {
      type: "category",
      label: "Getting Started",
      collapsible: false,
      items: ["intro", "quickstart"],
    },
    {
      type: "category",
      label: "Guides",
      collapsible: false,
      items: ["developer-guide", "pdf-workflow", "batch-processing"],
    },
    {
      type: "category",
      label: "Reference",
      collapsible: false,
      items: ["api", "cli", "bindings"],
    },
    {
      type: "category",
      label: "Evaluation",
      collapsible: false,
      items: ["evals"],
    },
  ],
};

module.exports = sidebars;
