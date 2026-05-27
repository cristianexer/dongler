import clsx from "clsx";
import Heading from "@theme/Heading";
import Link from "@docusaurus/Link";
import Layout from "@theme/Layout";
import useBaseUrl from "@docusaurus/useBaseUrl";

import styles from "./index.module.css";

const outputs = [
  {
    title: "Path-first API",
    body: "Load one document path, then render Markdown, LaTeX, or JSON from the same object.",
  },
  {
    title: "PDF-first roadmap",
    body: "The engine is being built around PDF text, tables, layout, and metadata extraction.",
  },
  {
    title: "Rust-native core",
    body: "Python and TypeScript bindings call the same Rust implementation instead of re-parsing documents.",
  },
];

const packages = [
  ["Rust", "dongler-core and the dongler CLI"],
  ["Python", "PyO3 bindings that return native dict/list/string values"],
  ["TypeScript", "NAPI-RS bindings with typed ESM exports"],
];

export default function Home() {
  const logoSrc = useBaseUrl("/img/dongler-logo.png");

  return (
    <Layout
      title="PDF-first document extraction"
      description="Dongler is a Rust-native document extraction engine with Python and TypeScript bindings."
    >
      <main>
        <section className={styles.hero}>
          <div className={styles.heroInner}>
            <div className={styles.heroCopy}>
              <img
                className={styles.heroLogo}
                src={logoSrc}
                alt="Dongler logo"
              />
              <p className={styles.kicker}>PDF-first document extraction</p>
              <Heading as="h1" className={styles.title}>
                Extract documents into Markdown and LaTeX.
              </Heading>
              <p className={styles.subtitle}>
                Dongler is a Rust-native extraction engine for developers who need
                predictable document objects, batch processing, and clean output.
                The current release supports text files and establishes the API
                for the upcoming PDF text, table, layout, and metadata engine.
              </p>
              <div className={styles.actions}>
                <Link className="button button--primary button--lg" to="/docs/intro">
                  Read the docs
                </Link>
                <Link className="button button--secondary button--lg" to="/docs/quickstart">
                  Quick start
                </Link>
              </div>
            </div>
          </div>
        </section>

        <section className={styles.section}>
          <div className={styles.grid}>
            {outputs.map((item) => (
              <article className={styles.card} key={item.title}>
                <h2>{item.title}</h2>
                <p>{item.body}</p>
              </article>
            ))}
          </div>
        </section>

        <section className={clsx(styles.section, styles.split)}>
          <div>
            <p className={styles.kicker}>One core, three ecosystems</p>
            <Heading as="h2">One extraction model across every package.</Heading>
            <p>
              The Python and TypeScript packages are thin native bindings over
              the Rust core. Users get the same path-based document workflow in
              every ecosystem: load, inspect metadata, render, and batch.
            </p>
          </div>
          <div className={styles.packageList}>
            {packages.map(([name, body]) => (
              <div className={styles.packageRow} key={name}>
                <strong>{name}</strong>
                <span>{body}</span>
              </div>
            ))}
          </div>
        </section>
      </main>
    </Layout>
  );
}
