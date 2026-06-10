import { useEffect, useRef, useState } from "react";
import Heading from "@theme/Heading";
import Head from "@docusaurus/Head";
import Link from "@docusaurus/Link";
import Layout from "@theme/Layout";
import useDocusaurusContext from "@docusaurus/useDocusaurusContext";
import { FontAwesomeIcon } from "@fortawesome/react-fontawesome";
import {
  faArrowRight,
  faBolt,
  faChartLine,
  faCode,
  faDiagramProject,
  faGaugeHigh,
  faLayerGroup,
  faLock,
  faMicrochip,
  faRulerCombined,
  faTableCells,
  faTerminal,
} from "@fortawesome/free-solid-svg-icons";
import hljs from "highlight.js/lib/common";
import "highlight.js/styles/atom-one-dark.css";

import styles from "./index.module.css";

const siteUrl = "https://cristianexer.github.io/dongler";
const seoDescription =
  "Dongler is a fast, Rust-native document extraction engine. Parse PDFs and 15+ formats into Markdown, LaTeX, and JSON locally — from Python, TypeScript, Rust, or the CLI.";
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
  author: { "@type": "Person", name: "Daniel Fat" },
  programmingLanguage: ["Rust", "Python", "TypeScript"],
  offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
};

const languages = [
  {
    id: "python",
    label: "Python",
    hl: "python",
    install: "pip install dongler",
    code: `import dongler

doc = dongler.load("report.pdf")
print(doc.to_markdown())`,
  },
  {
    id: "typescript",
    label: "TypeScript",
    hl: "typescript",
    install: "npm install @cristianexer/dongler",
    code: `import { load } from "@cristianexer/dongler";

const doc = load("report.pdf");
console.log(doc.toMarkdown());`,
  },
  {
    id: "rust",
    label: "Rust",
    hl: "rust",
    install: "cargo add dongler-core",
    code: `use dongler_core::load_path;

let doc = load_path("report.pdf")?;
println!("{}", doc.to_markdown()?);`,
  },
  {
    id: "cli",
    label: "CLI",
    hl: "bash",
    install: "cargo install dongler",
    code: `$ dongler extract report.pdf --format markdown
# → clean Markdown, streamed to stdout

$ dongler extract report.pdf --format json`,
  },
];

function highlight(code, language) {
  try {
    return hljs.highlight(code, { language }).value;
  } catch (error) {
    return code;
  }
}

function Counter({ value }) {
  const match = String(value).match(/^(\d+)(.*)$/);
  const target = match ? parseInt(match[1], 10) : 0;
  const suffix = match ? match[2] : String(value);
  const [shown, setShown] = useState(target);
  const ref = useRef(null);
  useEffect(() => {
    const el = ref.current;
    if (!el || target === 0 || typeof IntersectionObserver === "undefined") return;
    let raf = 0;
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries[0].isIntersecting) return;
        io.disconnect();
        const duration = 1100;
        const start = performance.now();
        const tick = (now) => {
          const p = Math.min(1, (now - start) / duration);
          const eased = 1 - Math.pow(1 - p, 3);
          setShown(Math.round(target * eased));
          if (p < 1) raf = requestAnimationFrame(tick);
        };
        setShown(0);
        raf = requestAnimationFrame(tick);
      },
      { threshold: 0.5 },
    );
    io.observe(el);
    return () => {
      io.disconnect();
      cancelAnimationFrame(raf);
    };
  }, [target]);
  return (
    <span ref={ref}>
      {shown}
      {suffix}
    </span>
  );
}

const stats = [
  { value: "90+", label: "pages / second", sub: "release build, single core" },
  { value: "15+", label: "input formats", sub: "PDF, Office, web, email" },
  { value: "4", label: "languages", sub: "Python · TS · Rust · CLI" },
  { value: "0", label: "cloud calls", sub: "fully local & deterministic" },
];

const formats = [
  "PDF",
  "DOCX",
  "XLSX",
  "PPTX",
  "ODT",
  "ODS",
  "ODP",
  "HTML",
  "XML",
  "EML",
  "Markdown",
  "TeX",
  "CSV",
  "TSV",
  "JSON",
  "JSONL",
];

const pipeline = [
  {
    icon: faLayerGroup,
    title: "Any document",
    body: "PDF and 15+ formats — born-digital or messy.",
  },
  {
    icon: faMicrochip,
    title: "Dongler engine",
    body: "Parse · font metrics · reading order · tables.",
  },
  {
    icon: faCode,
    title: "Structured output",
    body: "Markdown, LaTeX, or a typed JSON document.",
  },
];

const engine = [
  {
    icon: faMicrochip,
    title: "A custom parser, not a wrapper",
    body: "A from-scratch Rust PDF engine with its own tokenizer, font decoding, and CMap/ToUnicode handling — no pdfium, no poppler, no native bindings.",
  },
  {
    icon: faRulerCombined,
    title: "Font-metric bounding boxes",
    body: "Glyph boxes derived from real font ascent/descent and the text matrix, rotation-aware, so geometry stays tight under scaling and /Rotate.",
  },
  {
    icon: faDiagramProject,
    title: "Reading-order reconstruction",
    body: "Multi-column layouts are detected and re-sequenced into natural reading order instead of raw stream order.",
  },
  {
    icon: faTableCells,
    title: "Table structure with spans",
    body: "Ruled, aligned, and implied tables are recovered into a real cell grid — including merged column headers.",
  },
  {
    icon: faChartLine,
    title: "Benchmarked, not hand-waved",
    body: "Accuracy is measured with TEDS, GriTS, CER/WER, edit-similarity, and bbox IoU across a 1,400-PDF benchmark suite.",
  },
  {
    icon: faLock,
    title: "Local & deterministic",
    body: "No hosted service, API key, or model dependency in the default path. The same input always yields the same structured output.",
  },
];

const languageCards = [
  { label: "Python", icon: faCode, install: "pip install dongler", note: "Ingestion jobs & notebooks" },
  { label: "TypeScript", icon: faLayerGroup, install: "npm install @cristianexer/dongler", note: "Services & queues" },
  { label: "Rust", icon: faBolt, install: "cargo add dongler-core", note: "The core API, directly" },
  { label: "CLI", icon: faTerminal, install: "cargo install dongler", note: "Inspect & pipe anywhere" },
];

function LanguageSwitcher() {
  const [active, setActive] = useState("python");
  const current = languages.find((l) => l.id === active) ?? languages[0];
  return (
    <div className={styles.codeCard}>
      <div className={styles.codeTabs} role="tablist" aria-label="Language">
        {languages.map((lang) => (
          <button
            key={lang.id}
            type="button"
            role="tab"
            aria-selected={lang.id === active}
            className={lang.id === active ? styles.codeTabActive : styles.codeTab}
            onClick={() => setActive(lang.id)}
          >
            {lang.label}
          </button>
        ))}
      </div>
      <pre className={styles.codeBody}>
        <code
          className={`hljs language-${current.hl}`}
          dangerouslySetInnerHTML={{ __html: highlight(current.code, current.hl) }}
        />
      </pre>
      <div className={styles.codeFoot}>
        <span className={styles.codeDot} aria-hidden="true" />
        <code>{current.install}</code>
      </div>
    </div>
  );
}

export default function Home() {
  const { siteConfig } = useDocusaurusContext();
  const version = siteConfig.customFields?.donglerVersion;
  useEffect(() => {
    if (typeof IntersectionObserver === "undefined") return;
    document.documentElement.classList.add("dg-reveal-ready");
    const io = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (entry.isIntersecting) {
            entry.target.classList.add("is-visible");
            io.unobserve(entry.target);
          }
        });
      },
      { rootMargin: "0px 0px -8% 0px", threshold: 0.08 },
    );
    document.querySelectorAll(".dg-reveal").forEach((el) => io.observe(el));
    return () => io.disconnect();
  }, []);
  return (
    <Layout
      title="Fast, Rust-native document extraction"
      description={seoDescription}
    >
      <Head>
        <link rel="canonical" href={`${siteUrl}/`} />
        <meta name="keywords" content={seoKeywords.join(", ")} />
        <meta property="og:title" content="Dongler: fast, Rust-native document extraction" />
        <meta property="og:description" content={seoDescription} />
        <meta property="og:url" content={`${siteUrl}/`} />
        <meta property="og:type" content="website" />
        <meta property="og:image" content={`${siteUrl}/img/dongler-social-card.svg`} />
        <meta name="twitter:card" content="summary_large_image" />
        <script type="application/ld+json">{JSON.stringify(softwareSchema)}</script>
      </Head>
      <main className={styles.page}>
        {/* ---------------------------------------------------------- hero */}
        <section className={styles.hero}>
          <div className={styles.heroGlow} aria-hidden="true" />
          <div className={styles.heroInner}>
            <div className={styles.heroCopy}>
              <span className={styles.eyebrow}>
                <span className={styles.eyebrowDot} aria-hidden="true" />
                Rust-native document engine
                {version ? <em>v{version}</em> : null}
              </span>
              <Heading as="h1" className={styles.heroTitle}>
                Documents in.
                <br />
                <span>Structure out.</span>
              </Heading>
              <p className={styles.heroText}>
                Dongler is a from-scratch Rust engine that turns PDFs and 15+
                formats into clean Markdown, LaTeX, and typed JSON — locally, in
                milliseconds, from Python, TypeScript, Rust, or the CLI.
              </p>
              <div className={styles.actions}>
                <Link className="button button--primary button--lg" to="/docs/quickstart">
                  Get started{" "}
                  <FontAwesomeIcon icon={faArrowRight} className={styles.btnIcon} />
                </Link>
                <Link
                  className="button button--secondary button--lg"
                  href="https://github.com/cristianexer/dongler"
                >
                  View on GitHub
                </Link>
              </div>
              <dl className={styles.heroStats}>
                {stats.map((s) => (
                  <div key={s.label} className={styles.heroStat}>
                    <dt>{s.value}</dt>
                    <dd>{s.label}</dd>
                  </div>
                ))}
              </dl>
            </div>
            <div className={styles.heroPanel}>
              <LanguageSwitcher />
              <p className={styles.heroPanelNote}>
                <FontAwesomeIcon icon={faGaugeHigh} aria-hidden="true" /> One API,
                identical output across every binding.
              </p>
            </div>
          </div>
        </section>

        {/* ------------------------------------------------------- marquee */}
        <section className={styles.marqueeWrap} aria-label="Supported input formats">
          <div className={styles.marqueeFade} />
          <div className={styles.marquee}>
            {[...formats, ...formats].map((fmt, i) => (
              <span className={styles.chip} key={`${fmt}-${i}`}>
                {fmt}
              </span>
            ))}
          </div>
        </section>

        {/* --------------------------------------------------------- stats */}
        <section className={styles.statsBand} aria-label="At a glance">
          <div className={styles.statsInner}>
            {stats.map((s, i) => (
              <div
                key={s.label}
                className={`${styles.statBox} dg-reveal`}
                style={{ transitionDelay: `${i * 70}ms` }}
              >
                <div className={styles.statValue}>
                  <Counter value={s.value} />
                </div>
                <div className={styles.statLabel}>{s.label}</div>
                <div className={styles.statSub}>{s.sub}</div>
              </div>
            ))}
          </div>
        </section>

        {/* ------------------------------------------------------ pipeline */}
        <section className={styles.band}>
          <div className={`${styles.sectionHead} dg-reveal`}>
            <span className={styles.kicker}>How it works</span>
            <Heading as="h2">One path, document to structure.</Heading>
            <p>
              Load a path once. The engine parses, lays out, and structures the
              document, then renders it in the format your pipeline needs.
            </p>
          </div>
          <div className={styles.pipeline}>
            {pipeline.map((step, i) => (
              <div
                className={`${styles.pipeStep} dg-reveal`}
                style={{ transitionDelay: `${i * 90}ms` }}
                key={step.title}
              >
                <span className={styles.pipeIcon} aria-hidden="true">
                  <FontAwesomeIcon icon={step.icon} />
                </span>
                <h3>{step.title}</h3>
                <p>{step.body}</p>
                {i < pipeline.length - 1 ? (
                  <span className={styles.pipeArrow} aria-hidden="true">
                    <FontAwesomeIcon icon={faArrowRight} />
                  </span>
                ) : null}
              </div>
            ))}
          </div>
        </section>

        {/* -------------------------------------------------------- engine */}
        <section className={styles.band}>
          <div className={`${styles.sectionHead} dg-reveal`}>
            <span className={styles.kicker}>The engine</span>
            <Heading as="h2">Sophisticated extraction, built from scratch.</Heading>
            <p>
              No cloud, no OCR fallback by default, no third-party PDF runtime —
              just a purpose-built Rust core measured against real benchmarks.
            </p>
          </div>
          <div className={styles.engineGrid}>
            {engine.map((item, i) => (
              <article
                className={`${styles.engineCard} dg-reveal`}
                style={{ transitionDelay: `${(i % 3) * 80}ms` }}
                key={item.title}
              >
                <span className={styles.engineIcon} aria-hidden="true">
                  <FontAwesomeIcon icon={item.icon} />
                </span>
                <h3>{item.title}</h3>
                <p>{item.body}</p>
              </article>
            ))}
          </div>
        </section>

        {/* ----------------------------------------------------- languages */}
        <section className={styles.band}>
          <div className={`${styles.sectionHead} dg-reveal`}>
            <span className={styles.kicker}>Everywhere you build</span>
            <Heading as="h2">Same engine. Four ways to call it.</Heading>
            <p>
              The Python and TypeScript packages are thin wrappers over the Rust
              core, so every binding returns the identical document model.
            </p>
          </div>
          <div className={styles.langGrid}>
            {languageCards.map((lang, i) => (
              <article
                className={`${styles.langCard} dg-reveal`}
                style={{ transitionDelay: `${i * 70}ms` }}
                key={lang.label}
              >
                <span className={styles.langIcon} aria-hidden="true">
                  <FontAwesomeIcon icon={lang.icon} />
                </span>
                <div>
                  <strong>{lang.label}</strong>
                  <span className={styles.langNote}>{lang.note}</span>
                </div>
                <code>{lang.install}</code>
              </article>
            ))}
          </div>
        </section>

        {/* ------------------------------------------------------------ cta */}
        <section className={styles.ctaBand}>
          <div className={`${styles.ctaInner} dg-reveal`}>
            <Heading as="h2">Start extracting in one line.</Heading>
            <p>Install, point it at a document, and read structured output back.</p>
            <div className={styles.actions}>
              <Link className="button button--primary button--lg" to="/docs/quickstart">
                Quick start{" "}
                <FontAwesomeIcon icon={faArrowRight} className={styles.btnIcon} />
              </Link>
              <Link className="button button--secondary button--lg" to="/docs/api">
                API reference
              </Link>
            </div>
          </div>
        </section>
      </main>
    </Layout>
  );
}
