/** @type {import('@docusaurus/plugin-content-docs').SidebarsConfig} */
const sidebars = {
  docs: [
    "intro",
    "quickstart",
    "pdf-workflow",
    "batch-processing",
    "api",
    "bindings",
    "cli",
    {
      type: "category",
      label: "Architecture",
      items: ["architecture", "pdf-roadmap"],
    },
  ],
};

module.exports = sidebars;
