//! `omni seo check`/`omni seo report`: real, honest technical-SEO checks
//! against an already-built `web`-like target's `index.html`.
//!
//! Deliberately a lightweight **string-based heuristic scan**, not a real
//! HTML/DOM parser — this project does not take on an HTML-parsing
//! dependency for this. It is accurate for the output `web-static` itself
//! actually produces (see `crates/omni-web-adapter/src/lib.rs`), but could
//! be fooled by unusual hand-written markup a component's own template
//! injects (e.g. a `>` inside a quoted attribute value). Documented here
//! rather than silently assumed to be a full parser — see `docs/seo.md`.
//!
//! This command never claims a ranking outcome. It reports the presence
//! or absence of a fixed set of real, well-known technical-SEO signals —
//! nothing about competition, content quality, or what a search engine
//! will actually do with them.

use std::fs;
use std::path::Path;

pub struct Finding {
    pub check: &'static str,
    pub passed: bool,
    pub detail: String,
}

pub struct SeoReport {
    pub target: String,
    pub findings: Vec<Finding>,
}

impl SeoReport {
    pub fn all_passed(&self) -> bool {
        self.findings.iter().all(|f| f.passed)
    }
}

fn extract_between<'a>(html: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let start_idx = html.find(start)? + start.len();
    let end_idx = html[start_idx..].find(end)? + start_idx;
    Some(&html[start_idx..end_idx])
}

fn count_occurrences(html: &str, needle: &str) -> usize {
    html.matches(needle).count()
}

/// Runs the real checks against `html`'s actual content.
fn analyze(target: &str, html: &str) -> SeoReport {
    let mut findings = Vec::new();

    match extract_between(html, "<title>", "</title>") {
        Some(t) if !t.trim().is_empty() => findings.push(Finding {
            check: "title",
            passed: true,
            detail: format!("present: \"{}\"", t.trim()),
        }),
        _ => findings.push(Finding {
            check: "title",
            passed: false,
            detail: "missing or empty <title>".to_string(),
        }),
    }

    let has_description = html.contains("<meta name=\"description\"");
    findings.push(Finding {
        check: "meta-description",
        passed: has_description,
        detail: if has_description {
            "present".to_string()
        } else {
            "missing -- set 'seo.description' in the web app manifest".to_string()
        },
    });

    let has_viewport = html.contains("<meta name=\"viewport\"");
    findings.push(Finding {
        check: "viewport",
        passed: has_viewport,
        detail: if has_viewport {
            "present".to_string()
        } else {
            "missing -- page may not be reported mobile-friendly".to_string()
        },
    });

    let has_canonical = html.contains("rel=\"canonical\"");
    findings.push(Finding {
        check: "canonical-url",
        passed: has_canonical,
        detail: if has_canonical {
            "present".to_string()
        } else {
            "missing -- set 'seo.site_url' or 'seo.canonical_url'".to_string()
        },
    });

    let h1_count = count_occurrences(html, "<h1");
    findings.push(Finding {
        check: "single-h1",
        passed: h1_count == 1,
        detail: format!("found {h1_count} <h1> tag(s) (exactly one is recommended)"),
    });

    let img_count = count_occurrences(html, "<img");
    let img_with_alt = count_occurrences(html, "<img").min(count_occurrences(html, "alt="));
    // Approximate (string-based, not parsed per-tag): flags the common
    // all-or-nothing case accurately; a mix of some-with/some-without alt
    // on a page with multiple <img> tags may not be attributed precisely.
    let imgs_missing_alt = img_count.saturating_sub(img_with_alt);
    findings.push(Finding {
        check: "image-alt-text",
        passed: imgs_missing_alt == 0,
        detail: if img_count == 0 {
            "no <img> tags found".to_string()
        } else {
            format!("{img_count} <img> tag(s), {imgs_missing_alt} apparently missing 'alt='")
        },
    });

    let has_robots_noindex = html.contains("content=\"noindex");
    findings.push(Finding {
        check: "indexable",
        passed: !has_robots_noindex,
        detail: if has_robots_noindex {
            "page declares 'noindex' -- will not be indexed by search engines".to_string()
        } else {
            "indexable (no noindex directive)".to_string()
        },
    });

    SeoReport {
        target: target.to_string(),
        findings,
    }
}

fn load_built_html(root: &Path, output_dir: &str, target: &str) -> Result<String, String> {
    let path = root.join(output_dir).join(target).join("index.html");
    fs::read_to_string(&path).map_err(|e| {
        format!(
            "could not read '{}': {e}\n  these checks apply to an HTML page at '<output>/<target>/index.html'; run 'omni build {target}' first if it hasn't been built, or skip this command if '{target}' doesn't produce HTML at all",
            path.display()
        )
    })
}

fn print_report(report: &SeoReport) {
    println!("SEO check for target '{}':\n", report.target);
    for f in &report.findings {
        let mark = if f.passed { "PASS" } else { "FAIL" };
        println!("  [{mark}] {}: {}", f.check, f.detail);
    }
    println!();
    if report.all_passed() {
        println!("All checks passed.");
    } else {
        println!("Some checks failed. These are real technical-SEO signals, not a ranking guarantee -- see docs/seo.md.");
    }
}

fn print_report_json(report: &SeoReport) {
    let json = serde_json::json!({
        "target": report.target,
        "all_passed": report.all_passed(),
        "findings": report.findings.iter().map(|f| serde_json::json!({
            "check": f.check,
            "passed": f.passed,
            "detail": f.detail,
        })).collect::<Vec<_>>(),
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&json).unwrap_or_default()
    );
}

fn run_common(cwd: &Path, target: &str, json: bool) -> Result<SeoReport, i32> {
    let root = match omni_core::loader::find_project_root(cwd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return Err(1);
        }
    };
    let (loaded, _diags) = match omni_core::loader::load_project(&root) {
        Ok(ok) => ok,
        Err(e) => {
            eprintln!("error: {e}");
            return Err(1);
        }
    };
    let html = match load_built_html(&root, &loaded.file.build.output, target) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("error: E-SEO-NOT-BUILT: {e}");
            return Err(1);
        }
    };
    let report = analyze(target, &html);
    if !json {
        print_report(&report);
    }
    Ok(report)
}

/// `omni seo check <target>`: exits non-zero if any check fails -- a real
/// gate, usable in CI.
pub fn check(cwd: &Path, target: &str, json: bool) -> i32 {
    match run_common(cwd, target, json) {
        Err(code) => code,
        Ok(report) => {
            if json {
                print_report_json(&report);
            }
            if report.all_passed() {
                0
            } else {
                1
            }
        }
    }
}

/// `omni seo report <target>`: same checks, always exits 0 -- informational,
/// for "show me what's there" rather than gating a build.
pub fn report(cwd: &Path, target: &str, json: bool) -> i32 {
    match run_common(cwd, target, json) {
        Err(code) => code,
        Ok(report) => {
            if json {
                print_report_json(&report);
            }
            0
        }
    }
}
