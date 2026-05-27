import Heading from "@theme/Heading";
import Link from "@docusaurus/Link";
import Layout from "@theme/Layout";

import styles from "./index.module.css";

const features = [
  {
    title: "Predictable document objects",
    body: "Consistent JSON for text, tables, metadata, and layout-aware structure as format support expands.",
  },
  {
    title: "Batch at scale",
    body: "Process directories of documents with per-file success and error results for reliable pipelines.",
  },
  {
    title: "Native. Bindings. Simple.",
    body: "A Rust core powers the CLI plus Python and TypeScript packages through the same extraction model.",
  },
];

const docsPreview = [
  {
    title: "Introduction",
    to: "/docs/intro",
    body: "What Dongler is, what works today, and what is planned.",
  },
  {
    title: "Quick start",
    to: "/docs/quickstart",
    body: "Install packages, load documents, render Markdown, LaTeX, and JSON.",
  },
  {
    title: "API",
    to: "/docs/api",
    body: "Use the object model from Rust, Python, and TypeScript.",
  },
];

export default function Home() {
  return (
    <Layout
      title="Document extraction for clean Markdown and LaTeX"
      description="Dongler is a Rust-native document extraction engine with Python and TypeScript bindings."
    >
      <main>
        <section className={styles.hero}>
          <div className={styles.heroInner}>
            <div className={styles.heroCopy}>
              <p className={styles.brandWord}>Dongler</p>
              <Heading as="h1" className={styles.heroTitle}>
                Extract documents into clean <span>Markdown and LaTeX</span>.
              </Heading>
              <p className={styles.heroText}>
                A Rust-native extraction engine that turns complex documents into
                predictable, structured document objects. Built for batch
                workloads. Designed for clean output. Bindings for Python and
                TypeScript.
              </p>
              <div className={styles.actions}>
                <Link className="button button--primary button--lg" to="/docs/intro">
                  Read the docs <span aria-hidden="true">-&gt;</span>
                </Link>
                <Link className="button button--secondary button--lg" to="/docs/quickstart">
                  Quick start <span aria-hidden="true">-&gt;</span>
                </Link>
              </div>
            </div>

            <div className={styles.previewPanel} aria-label="Markdown output preview">
              <div className={styles.panelBar}>
                <span>Input</span>
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
{`# Quarterly Report
## Executive Summary

Dongler extracts tables, lists, and
structure with high fidelity.

### Key Results
- Revenue increased 18% YoY
- Operating margin improved to 24%
- Cash position remains strong

### Financials
| Metric | Q1 FY24 | Q2 FY24 | Q3 FY24 |
| --- | --- | --- | --- |
| Revenue | 12.4M | 14.7M | 16.6M |`}
                </pre>
                <div className={styles.documentPreview}>
                  <h2>Quarterly Report</h2>
                  <h3>Executive Summary</h3>
                  <p>
                    Dongler extracts tables, lists, and structure with high
                    fidelity.
                  </p>
                  <h3>Key Results</h3>
                  <ul>
                    <li>Revenue increased 18% YoY</li>
                    <li>Operating margin improved to 24%</li>
                    <li>Cash position remains strong</li>
                  </ul>
                  <table>
                    <thead>
                      <tr>
                        <th>Metric</th>
                        <th>Q1 FY24</th>
                        <th>Q3 FY24</th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr>
                        <td>Revenue</td>
                        <td>12.4M</td>
                        <td>16.6M</td>
                      </tr>
                      <tr>
                        <td>Margin</td>
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
              Built for real-world document workloads.
            </Heading>
            <p>
              Dongler focuses on predictable structure and clean output so your
              pipelines stay reliable and downstream work stays simple.
            </p>
          </div>
          <div className={styles.featureGrid}>
            {features.map((item) => (
              <article className={styles.featureItem} key={item.title}>
                <span className={styles.featureIcon} aria-hidden="true" />
                <h3>{item.title}</h3>
                <p>{item.body}</p>
              </article>
            ))}
          </div>
        </section>

        <section className={styles.docsBand} aria-labelledby="docs-preview">
          <div className={styles.docsContent}>
            <Heading as="h2" id="docs-preview">
              Start with the path-first workflow.
            </Heading>
            <p>
              Load a document path once, inspect structured metadata, then render
              Markdown, LaTeX, or JSON from the same object.
            </p>
            <pre className={styles.installBlock}>
{`import dongler

doc = dongler.load("document.txt")
markdown = doc.to_markdown()
latex = doc.to_latex()`}
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
