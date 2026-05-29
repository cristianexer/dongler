import Heading from "@theme/Heading";
import Head from "@docusaurus/Head";
import Link from "@docusaurus/Link";
import Layout from "@theme/Layout";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import {
  faBolt,
  faCode,
  faFilePdf,
  faLayerGroup,
  faTableCells,
  faTerminal,
} from "@fortawesome/free-solid-svg-icons";

import styles from "./index.module.css";

const siteUrl = "https://cristianexer.github.io/dongler";
const seoDescription =
  "Dongler is a fast Rust-native PDF and document extraction package for developers. Parse PDFs and documents into Markdown, LaTeX, and JSON from Python, TypeScript, Rust, or the CLI.";
const seoKeywords = [
  "PDF parser",
  "PDF to Markdown",
  "document extraction",
  "Markdown extraction",
  "LaTeX extraction",
  "Rust PDF parser",
  "Python PDF parser",
  "Node.js PDF parser",
];

const softwareSchema = {
  "@context": "https://schema.org",
  "@type": "SoftwareApplication",
  name: "Dongler",
  applicationCategory: "DeveloperApplication",
  operatingSystem: "macOS, Linux, Windows",
  description: seoDescription,
  url: siteUrl,
  codeRepository: "https://github.com/cristianexer/dongler",
  license: "https://github.com/cristianexer/dongler/blob/main/LICENSE",
  author: {
    "@type": "Person",
    name: "Daniel Fat",
  },
  programmingLanguage: ["Rust", "Python", "TypeScript"],
  offers: {
    "@type": "Offer",
    price: "0",
    priceCurrency: "USD",
  },
};

const features = [
  {
    icon: faFilePdf,
    title: "PDF to Markdown in one call",
    body: "Load a PDF path and render Markdown, LaTeX, or JSON from the same document object.",
  },
  {
    icon: faBolt,
    title: "Native speed, local runtime",
    body: "A Rust core does the extraction locally, with no hosted service or model dependency in the default path.",
  },
  {
    icon: faLayerGroup,
    title: "Same API across stacks",
    body: "Use the CLI, Python, TypeScript, or Rust API without changing the extraction model.",
  },
  {
    icon: faTableCells,
    title: "Tables and structure",
    body: "Extract page blocks, simple tables, metadata, source anchors, images, and warnings for downstream code.",
  },
  {
    icon: faTerminal,
    title: "Pipeline-friendly errors",
    body: "Batch APIs return one result per file, so unsupported documents do not stop the whole job.",
  },
  {
    icon: faCode,
    title: "Developer-first output",
    body: "Markdown for RAG and review, LaTeX for technical documents, JSON when you need the full IR.",
  },
];

const docsPreview = [
  {
    title: "Quick start",
    to: "/docs/quickstart",
    body: "Install Dongler and parse your first PDF in Python, TypeScript, Rust, or the CLI.",
  },
  {
    title: "Developer guide",
    to: "/docs/developer-guide",
    body: "Choose the right runtime, handle errors, batch files, and inspect the document object.",
  },
  {
    title: "API",
    to: "/docs/api",
    body: "Reference the object API, batch results, renderers, and document IR fields.",
  },
];

export default function Home() {
  return (
    <Layout
      title="Fast PDF parsing to Markdown, LaTeX, and JSON"
      description={seoDescription}
    >
      <Head>
        <link rel="canonical" href={`${siteUrl}/`} />
        <meta name="keywords" content={seoKeywords.join(", ")} />
        <meta property="og:title" content="Dongler: fast PDF parsing to Markdown, LaTeX, and JSON" />
        <meta property="og:description" content={seoDescription} />
        <meta property="og:url" content={`${siteUrl}/`} />
        <meta property="og:type" content="website" />
        <meta property="og:image" content={`${siteUrl}/img/dongler-social-card.svg`} />
        <meta name="twitter:card" content="summary_large_image" />
        <script type="application/ld+json">{JSON.stringify(softwareSchema)}</script>
      </Head>
      <main>
        <section className={styles.hero}>
          <div className={styles.heroInner}>
            <div className={styles.heroCopy}>
              <p className={styles.brandWord}>Dongler</p>
              <Heading as="h1" className={styles.heroTitle}>
                Fast PDF parsing to clean <span>Markdown</span>.
              </Heading>
              <p className={styles.heroText}>
                Parse PDFs and other documents into Markdown, LaTeX, or JSON
                with a local Rust engine and simple bindings for Python,
                TypeScript, Rust, and the CLI.
              </p>
              <div className={styles.actions}>
                <Link className="button button--secondary button--lg" to="/docs/quickstart">
                  Quick start <span aria-hidden="true">-&gt;</span>
                </Link>
                <Link className="button button--primary button--lg" to="/docs/developer-guide">
                  Developer guide <span aria-hidden="true">-&gt;</span>
                </Link>
              </div>
              <div className={styles.commandRow} aria-label="Install commands">
                <code>pip install dongler</code>
                <code>npm install @cristianexer/dongler</code>
                <code>cargo install dongler</code>
              </div>
            </div>

            <div className={styles.previewPanel} aria-label="Markdown output preview">
              <div className={styles.panelBar}>
                <span>report.pdf</span>
                <span aria-hidden="true">-&gt;</span>
                <span>Document object</span>
                <span aria-hidden="true">-&gt;</span>
                <strong>Output</strong>
              </div>
              <div className={styles.panelTabs} aria-label="Output formats">
                <span className={styles.activeTab}>Markdown</span>
                <span>LaTeX</span>
              </div>
              <div className={styles.panelBody}>
                <pre className={styles.markdownSample}>
{`import dongler

doc = dongler.load("report.pdf")
markdown = doc.to_markdown()
data = doc.to_dict()

# Extracted Markdown

## Capital adequacy by class

| Class | Requirement | Available |
| --- | --- | --- |
| A | 12.4M | 16.6M |`}
                </pre>
                <div className={styles.documentPreview}>
                  <h2>Extracted Markdown</h2>
                  <h3>Capital adequacy by class</h3>
                  <p>
                    Parsed from a PDF into a structured document object and
                    rendered as Markdown.
                  </p>
                  <table>
                    <thead>
                      <tr>
                        <th>Class</th>
                        <th>Requirement</th>
                        <th>Available</th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr>
                        <td>A</td>
                        <td>12.4M</td>
                        <td>16.6M</td>
                      </tr>
                      <tr>
                        <td>B</td>
                        <td>18%</td>
                        <td>24%</td>
                      </tr>
                    </tbody>
                  </table>
                </div>
              </div>
            </div>
          </div>
        </section>

        <section className={styles.featureBand} aria-labelledby="workloads">
          <div className={styles.featureIntro}>
            <Heading as="h2" id="workloads">
              Built for document pipelines that need useful output quickly.
            </Heading>
            <p>
              Start with the common case: a developer has a PDF path and needs
              Markdown, LaTeX, or JSON without wiring together a stack of tools.
            </p>
          </div>
          <div className={styles.featureGrid}>
            {features.map((item) => (
              <article className={styles.featureItem} key={item.title}>
                <span className={styles.featureIcon} aria-hidden="true">
                  <FontAwesomeIcon icon={item.icon} />
                </span>
                <h3>{item.title}</h3>
                <p>{item.body}</p>
              </article>
            ))}
          </div>
        </section>

        <section className={styles.docsBand} aria-labelledby="docs-preview">
          <div className={styles.docsContent}>
            <Heading as="h2" id="docs-preview">
              Developer docs for the path-first workflow.
            </Heading>
            <p>
              Load a document path once, inspect structured metadata, handle
              warnings and batch errors, then render Markdown, LaTeX, or JSON.
            </p>
            <pre className={styles.installBlock}>
{`import dongler

doc = dongler.load("document.pdf")
markdown = doc.to_markdown()
json_payload = doc.to_json()`}
            </pre>
          </div>

          <div className={styles.docsList}>
            {docsPreview.map((item) => (
              <Link className={styles.docsLink} to={item.to} key={item.title}>
                <strong>{item.title}</strong>
                <span>{item.body}</span>
              </Link>
            ))}
          </div>
        </section>
      </main>
    </Layout>
  );
}
