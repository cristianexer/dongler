const lightCodeTheme = require("prism-react-renderer").themes.oneLight;
const darkCodeTheme = require("prism-react-renderer").themes.oneDark;

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: "Dongler",
  tagline: "Fast PDF parsing to Markdown, LaTeX, and JSON",
  favicon: "img/dongler-logo.png",

  url: "https://cristianexer.github.io",
  baseUrl: "/dongler/",
  organizationName: "cristianexer",
  projectName: "dongler",
  trailingSlash: false,
  headTags: [
    {
      tagName: "meta",
      attributes: {
        name: "keywords",
        content:
          "PDF parser, PDF to Markdown, document extraction, Markdown extraction, LaTeX extraction, Rust PDF parser, Python PDF parser, Node.js PDF parser",
      },
    },
    {
      tagName: "meta",
      attributes: {
        name: "author",
        content: "Daniel Fat",
      },
    },
    {
      tagName: "meta",
      attributes: {
        name: "google-site-verification",
        content: "7jJ6oNFmSvCO10VgyTcEQ6I4bOztehoTI4d78mOy8-g",
      },
    },
    {
      tagName: "meta",
      attributes: {
        name: "msvalidate.01",
        content: "75399C72B829EF6C68317EBB1E2D0380",
      },
    },
  ],

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
        sitemap: {
          changefreq: "weekly",
          priority: 0.7,
          ignorePatterns: ["/tags/**"],
          filename: "sitemap.xml",
        },
        theme: {
          customCss: require.resolve("./src/css/custom.css"),
        },
      },
    ],
  ],

  themeConfig: {
    image: "img/dongler-social-card.svg",
    colorMode: {
      defaultMode: "dark",
      respectPrefersColorScheme: false,
    },
    navbar: {
      title: "Dongler",
      logo: {
        alt: "Dongler",
        src: "img/dongler-mark.svg",
      },
      items: [
        { to: "/docs/intro", label: "Docs", position: "left" },
        { to: "/docs/quickstart", label: "Quick start", position: "left" },
        { to: "/docs/api", label: "API", position: "left" },
        {
          href: "https://github.com/cristianexer/dongler",
          position: "right",
          className: "header-github-link",
          "aria-label": "GitHub repository",
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
            { label: "LLM Context", href: "https://cristianexer.github.io/dongler/llms.txt" },
          ],
        },
        {
          title: "Packages",
          items: [
            { label: "Rust", href: "https://crates.io/crates/dongler" },
            { label: "Python", href: "https://pypi.org/project/dongler/" },
            { label: "npm", href: "https://www.npmjs.com/package/@cristianexer/dongler" },
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
      copyright: `MIT licensed. Maintained by Daniel Fat and Dongler contributors. Copyright © ${new Date().getFullYear()}.`,
    },
    prism: {
      theme: lightCodeTheme,
      darkTheme: darkCodeTheme,
      additionalLanguages: ["rust", "python", "bash", "json"],
    },
  },
};

module.exports = config;
