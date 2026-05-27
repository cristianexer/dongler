const lightCodeTheme = require("prism-react-renderer").themes.github;
const darkCodeTheme = require("prism-react-renderer").themes.dracula;

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: "Dongler",
  tagline: "Structure from messy documents",
  favicon: "img/dongler-logo.png",

  url: "https://cristianexer.github.io",
  baseUrl: "/dongler/",
  organizationName: "cristianexer",
  projectName: "dongler",
  trailingSlash: false,

  onBrokenLinks: "throw",
  markdown: {
    hooks: {
      onBrokenMarkdownLinks: "warn",
    },
  },

  i18n: {
    defaultLocale: "en",
    locales: ["en"],
  },

  presets: [
    [
      "classic",
      {
        docs: {
          path: "../docs",
          routeBasePath: "docs",
          sidebarPath: require.resolve("./sidebars.js"),
          editUrl: "https://github.com/cristianexer/dongler/tree/main/",
        },
        blog: false,
        theme: {
          customCss: require.resolve("./src/css/custom.css"),
        },
      },
    ],
  ],

  themeConfig: {
    image: "img/dongler-social-card.svg",
    navbar: {
      title: "Dongler",
      logo: {
        alt: "Dongler",
        src: "img/dongler-logo.png",
      },
      items: [
        { to: "/docs/intro", label: "Docs", position: "left" },
        { to: "/docs/pdf-workflow", label: "PDF Workflow", position: "left" },
        {
          href: "https://github.com/cristianexer/dongler",
          label: "GitHub",
          position: "right",
        },
      ],
    },
    footer: {
      style: "dark",
      links: [
        {
          title: "Docs",
          items: [
            { label: "Introduction", to: "/docs/intro" },
            { label: "Quick Start", to: "/docs/quickstart" },
            { label: "PDF Workflow", to: "/docs/pdf-workflow" },
            { label: "Architecture", to: "/docs/architecture" },
          ],
        },
        {
          title: "Packages",
          items: [
            { label: "Rust", href: "https://crates.io/crates/dongler" },
            { label: "Python", href: "https://pypi.org/project/dongler/" },
            { label: "npm", href: "https://www.npmjs.com/package/dongler" },
          ],
        },
        {
          title: "Project",
          items: [
            { label: "GitHub", href: "https://github.com/cristianexer/dongler" },
            { label: "License", href: "https://github.com/cristianexer/dongler/blob/main/LICENSE" },
          ],
        },
      ],
      copyright: `Created by Daniel Fat. Copyright © ${new Date().getFullYear()} Dongler contributors.`,
    },
    prism: {
      theme: lightCodeTheme,
      darkTheme: darkCodeTheme,
      additionalLanguages: ["rust", "python", "bash", "json"],
    },
  },
};

module.exports = config;
